use indexmap::IndexMap;
use xilo_proto::tlv::{self, Item};
use xilo_proto::{Message, MsgId, Request};

use crate::schema::{
    CommandDef, Condition, Danger, ErrorDef, FamilyDef, FieldDef, Input, Kind, ModelStatus,
    ParamKind,
};
use crate::value::{FieldMap, Value};

#[derive(Clone, Debug)]
pub struct ModelSpec {
    pub code: String,
    pub name: String,
    pub status: ModelStatus,
    pub family: FamilyDef,
    pub serial_prefixes: Vec<String>,
    pub assets: String,
    pub fields: Vec<FieldDef>,
    pub commands: IndexMap<String, CommandDef>,
    pub errors: IndexMap<u32, ErrorDef>,
    pub(crate) state_msgs: Vec<MsgId>,
}

/// Proof that the user confirmed a dangerous command in the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Confirmed(());

impl Confirmed {
    pub fn by_user() -> Self {
        Self(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandRequest {
    pub command: String,
    pub value: Option<i64>,
    pub confirmed: Option<Confirmed>,
}

impl CommandRequest {
    pub fn new(command: impl Into<String>, value: Option<i64>) -> Self {
        Self {
            command: command.into(),
            value,
            confirmed: None,
        }
    }

    pub fn confirmed(mut self, proof: Confirmed) -> Self {
        self.confirmed = Some(proof);
        self
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum CommandError {
    #[error("this model has no command \"{0}\"")]
    Unknown(String),
    #[error("{0} is not verified on hardware, so Xilo only reads it")]
    ReadOnlyModel(String),
    #[error("a value is required")]
    MissingValue,
    #[error("{value} is not allowed; allowed: {allowed}")]
    InvalidValue { value: i64, allowed: String },
    #[error("{0}")]
    Gated(String),
    #[error("this action needs confirmation")]
    NeedsConfirmation,
}

impl ModelSpec {
    pub(crate) fn finish(mut self) -> Self {
        self.state_msgs = self
            .family
            .state_msgs
            .iter()
            .filter_map(|m| MsgId::parse(m))
            .collect();
        self
    }

    pub fn is_verified(&self) -> bool {
        self.status == ModelStatus::Verified
    }

    pub fn field(&self, id: &str) -> Option<&FieldDef> {
        self.fields.iter().find(|f| f.id == id)
    }

    pub fn command(&self, id: &str) -> Option<&CommandDef> {
        self.commands.get(id)
    }

    pub fn is_state_msg(&self, id: MsgId) -> bool {
        self.state_msgs.contains(&id)
    }

    fn applies(&self, field: &FieldDef, id: MsgId) -> bool {
        if field.msgs.is_empty() {
            self.state_msgs.contains(&id)
        } else {
            field.msgs.iter().any(|m| MsgId::parse(m) == Some(id))
        }
    }

    /// Updates `map` from a message and returns the ids that changed.
    pub fn decode(&self, message: &Message, map: &mut FieldMap) -> Vec<String> {
        let id = message.id();
        let mut changed = Vec::new();
        for field in &self.fields {
            if !self.applies(field, id) {
                continue;
            }
            let Some(key) = parse_key(&field.key) else {
                continue;
            };
            let Some(item) = message.payload.get(key) else {
                continue;
            };
            match decode_field(field, item) {
                Some(value) => {
                    if map.get(&field.id) != Some(&value) {
                        map.insert(field.id.clone(), value);
                        changed.push(field.id.clone());
                    }
                }
                None => {
                    if map.shift_remove(&field.id).is_some() {
                        changed.push(field.id.clone());
                    }
                }
            }
        }
        changed
    }

    /// Text for a value, using the field's options, scale and unit.
    pub fn format(&self, field_id: &str, value: &Value) -> String {
        let Some(field) = self.field(field_id) else {
            return value.to_string();
        };
        if let Some(raw) = value.as_i64()
            && let Some(label) = field.options.get(&raw.to_string())
        {
            return label.clone();
        }
        match (&field.unit, value) {
            (Some(unit), Value::Int(_) | Value::Float(_)) => format!("{value} {unit}"),
            _ => value.to_string(),
        }
    }

    pub fn error_text(&self, code: u32) -> Option<&ErrorDef> {
        self.errors.get(&code)
    }

    pub fn condition_holds(cond: &Condition, state: &FieldMap) -> bool {
        let current = state.get(&cond.field).and_then(Value::as_i64);
        match (cond.eq, cond.ne, current) {
            (Some(eq), _, Some(v)) => v == eq,
            (_, Some(ne), Some(v)) => v != ne,
            (None, None, Some(v)) => v != 0,
            _ => false,
        }
    }

    pub fn is_visible(&self, command: &CommandDef, state: &FieldMap) -> bool {
        command
            .visible_if
            .iter()
            .all(|c| Self::condition_holds(c, state))
    }

    /// The first unmet requirement, if any.
    pub fn gate(&self, command: &CommandDef, state: &FieldMap) -> Option<String> {
        command
            .requires
            .iter()
            .find(|c| !Self::condition_holds(c, state))
            .map(|c| {
                c.reason
                    .clone()
                    .unwrap_or_else(|| format!("requires {}", c.field))
            })
    }

    /// The current value of a command's setting, as the command's own value.
    pub fn current(&self, command: &CommandDef, state: &FieldMap) -> Option<i64> {
        let raw = state.get(command.state.as_deref()?)?.as_i64()?;
        if command.input == Input::Switch {
            Some(if raw == command.on_state {
                command.on_value
            } else {
                command.off_value
            })
        } else {
            Some(raw)
        }
    }

    /// The single place that turns a user request into protocol bytes.
    pub fn build(
        &self,
        request: &CommandRequest,
        state: &FieldMap,
        allow_unverified: bool,
    ) -> Result<Request, CommandError> {
        let command = self
            .commands
            .get(&request.command)
            .ok_or_else(|| CommandError::Unknown(request.command.clone()))?;
        let session_command = self.family.keepalive.as_ref().map(|k| k.command.as_str())
            == Some(command.id.as_str())
            || self.family.subscribe.as_deref() == Some(command.id.as_str())
            || self.family.status_request.as_deref() == Some(command.id.as_str());
        if !session_command && !self.is_verified() && !allow_unverified {
            return Err(CommandError::ReadOnlyModel(self.name.clone()));
        }
        if let Some(reason) = self.gate(command, state) {
            return Err(CommandError::Gated(reason));
        }
        if command.danger != Danger::None && request.confirmed.is_none() {
            return Err(CommandError::NeedsConfirmation);
        }
        let mut params = Vec::new();
        if let Some(param) = &command.param {
            let value = request.value.ok_or(CommandError::MissingValue)?;
            validate(command, value)?;
            let key = parse_key(&param.key).unwrap_or(0xa2);
            params.push((key, typed(param.kind, value)));
        }
        for fixed in &command.fixed {
            let key = parse_key(&fixed.key).unwrap_or(0xa2);
            params.push((key, typed(fixed.kind, fixed.value)));
        }
        let id =
            MsgId::parse(&command.msg).ok_or_else(|| CommandError::Unknown(command.id.clone()))?;
        Ok(Request { id, params })
    }

    /// The state change a successful command implies.
    pub fn optimistic(&self, request: &CommandRequest) -> Option<(String, Value)> {
        let command = self.commands.get(&request.command)?;
        let field = command.state.clone()?;
        let value = request.value?;
        let raw = if command.input == Input::Switch {
            if value == command.on_value {
                command.on_state
            } else if command.on_state == 1 {
                0
            } else {
                command.on_state - 1
            }
        } else {
            value
        };
        let kind = self.field(&field).map(|f| f.kind);
        let value = match kind {
            Some(Kind::Bool | Kind::Bit) => Value::Bool(raw != 0),
            _ => Value::Int(raw),
        };
        Some((field, value))
    }

    /// Encodes a value back into a payload item, for the simulator.
    pub fn encode_field(&self, field: &FieldDef, value: &Value) -> Option<(u8, u8, Vec<u8>)> {
        let key = parse_key(&field.key)?;
        let raw = match field.scale {
            Some(scale) => (value.as_f64()? / scale).round() as i64,
            None => value.as_i64().unwrap_or(0),
        };
        Some(match field.kind {
            Kind::U8 | Kind::I8 | Kind::Bool => (key, tlv::TYPE_U8, vec![raw as u8]),
            Kind::U16 | Kind::I16 => (key, tlv::TYPE_U16, (raw as u16).to_le_bytes().to_vec()),
            Kind::U32 => (key, tlv::TYPE_U32, (raw as u32).to_le_bytes().to_vec()),
            Kind::Text => (key, tlv::TYPE_STR, value.as_text()?.as_bytes().to_vec()),
            Kind::VersionDigits => {
                let digits: String = value
                    .to_string()
                    .chars()
                    .filter(char::is_ascii_digit)
                    .collect();
                let n: u16 = digits.parse().ok()?;
                (key, tlv::TYPE_U16, n.to_le_bytes().to_vec())
            }
            Kind::Hex | Kind::Bit => return None,
        })
    }
}

fn validate(command: &CommandDef, value: i64) -> Result<(), CommandError> {
    let allowed = match command.input {
        Input::Switch => vec![command.on_value, command.off_value],
        Input::Choice => command.options.iter().map(|o| o.value).collect(),
        Input::Number | Input::None => Vec::new(),
    };
    if !allowed.is_empty() {
        if allowed.contains(&value) {
            return Ok(());
        }
        return Err(CommandError::InvalidValue {
            value,
            allowed: allowed
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    let min = command.min.unwrap_or(0);
    let max = command.max.unwrap_or(i64::from(u32::MAX));
    let step = command.step.unwrap_or(1).max(1);
    if value < min || value > max || (value - min) % step != 0 {
        return Err(CommandError::InvalidValue {
            value,
            allowed: format!("{min}–{max} in steps of {step}"),
        });
    }
    Ok(())
}

fn typed(kind: ParamKind, value: i64) -> tlv::Value {
    match kind {
        ParamKind::U8 => tlv::Value::U8(value as u8),
        ParamKind::U16 => tlv::Value::U16(value as u16),
        ParamKind::U32 => tlv::Value::U32(value as u32),
    }
}

pub(crate) fn parse_key(key: &str) -> Option<u8> {
    u8::from_str_radix(key, 16).ok()
}

fn decode_field(field: &FieldDef, item: &Item) -> Option<Value> {
    let data = match field.offset {
        Some(offset) => item.data.get(offset..)?,
        None => item.data.as_slice(),
    };
    let width = match field.kind {
        Kind::U8 | Kind::I8 | Kind::Bool | Kind::Bit => 1,
        Kind::U16 | Kind::I16 | Kind::VersionDigits => 2,
        Kind::U32 => 4,
        Kind::Text | Kind::Hex => data.len(),
    };
    let unsigned = || -> Option<u64> {
        let bytes = if field.offset.is_some() {
            data.get(..width)?
        } else {
            &data[..data.len().min(8)]
        };
        if bytes.is_empty() {
            return None;
        }
        let mut buf = [0u8; 8];
        buf[..bytes.len()].copy_from_slice(bytes);
        Some(u64::from_le_bytes(buf))
    };
    let value = match field.kind {
        Kind::U8 | Kind::U16 | Kind::U32 => {
            let raw = unsigned()?;
            if field.na == Some(raw) {
                return None;
            }
            scaled(field, raw as i64)
        }
        Kind::I8 => scaled(field, i64::from(*data.first()? as i8)),
        Kind::I16 => scaled(
            field,
            i64::from(i16::from_le_bytes([*data.first()?, *data.get(1)?])),
        ),
        Kind::Bool => Value::Bool(unsigned()? != 0),
        Kind::Bit => Value::Bool((data.first()? >> field.bit.unwrap_or(0)) & 1 == 1),
        Kind::Text => Value::Text(item_text(data)),
        Kind::Hex => Value::Bytes(data.to_vec()),
        Kind::VersionDigits => {
            let raw = unsigned()?;
            let digits = raw.to_string();
            Value::Text(
                digits
                    .chars()
                    .map(String::from)
                    .collect::<Vec<_>>()
                    .join("."),
            )
        }
    };
    Some(value)
}

fn scaled(field: &FieldDef, raw: i64) -> Value {
    match field.scale {
        Some(scale) => Value::Float(raw as f64 * scale),
        None => Value::Int(raw),
    }
}

fn item_text(data: &[u8]) -> String {
    let trimmed: Vec<u8> = data
        .iter()
        .copied()
        .skip_while(|b| *b == 0)
        .take_while(|b| *b != 0)
        .collect();
    String::from_utf8_lossy(&trimmed).into_owned()
}
