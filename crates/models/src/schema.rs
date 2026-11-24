use indexmap::IndexMap;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FamilyFile {
    pub family: FamilyDef,
    #[serde(default)]
    pub field: Vec<FieldDef>,
    #[serde(default)]
    pub command: Vec<CommandDef>,
    #[serde(default)]
    pub error: Vec<ErrorDef>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ModelFile {
    pub model: ModelDef,
    #[serde(default)]
    pub field: Vec<FieldDef>,
    #[serde(default)]
    pub command: Vec<CommandDef>,
    #[serde(default)]
    pub error: Vec<ErrorDef>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FamilyDef {
    pub id: String,
    pub protocol: Protocol,
    pub gatt: String,
    /// Messages carrying the full or live state.
    pub state_msgs: Vec<String>,
    pub status_request: Option<String>,
    pub subscribe: Option<String>,
    pub keepalive: Option<Periodic>,
    pub poll: Option<Periodic>,
    #[serde(default = "default_dashboard")]
    pub dashboard: String,
}

fn default_dashboard() -> String {
    "generic".into()
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Zx,
    Prime,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Periodic {
    pub command: String,
    pub every_s: u64,
    pub skip_if: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ModelDef {
    pub code: String,
    pub name: String,
    pub family: String,
    pub status: ModelStatus,
    #[serde(default)]
    pub serial_prefixes: Vec<String>,
    pub assets: Option<String>,
    #[serde(default)]
    pub drop_commands: Vec<String>,
    #[serde(default)]
    pub drop_fields: Vec<String>,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    Verified,
    Unverified,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    U8,
    U16,
    U32,
    I8,
    I16,
    Bool,
    Bit,
    I32,
    F32,
    Text,
    /// A string whose first byte is its length.
    PrefixedText,
    Hex,
    VersionDigits,
    /// Decoded from the type byte the device sends.
    Auto,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    #[default]
    Verified,
    Likely,
    Unknown,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    pub key: String,
    pub id: String,
    pub label: String,
    pub kind: Kind,
    /// Messages this key is read from; empty means the family's state messages.
    #[serde(default)]
    pub msgs: Vec<String>,
    pub unit: Option<String>,
    pub scale: Option<f64>,
    /// Raw value meaning "not available".
    pub na: Option<u64>,
    #[serde(rename = "enum", default)]
    pub options: IndexMap<String, String>,
    /// Byte offset inside the value, for keys that pack several values.
    pub offset: Option<usize>,
    /// Byte count at the offset, when it differs from the kind's width.
    pub len: Option<usize>,
    pub bit: Option<u8>,
    /// Bits of the raw value to keep, shifted down to bit 0.
    pub mask: Option<u64>,
    /// Signed reading for `auto` fields.
    #[serde(default)]
    pub signed: bool,
    #[serde(default)]
    pub confidence: Confidence,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Function,
    General,
    Control,
    #[default]
    Hidden,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    None,
    Switch,
    Choice,
    Number,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Danger {
    #[default]
    None,
    Confirm,
    Destructive,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    U8,
    U16,
    U32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ParamDef {
    #[serde(default = "default_param_key")]
    pub key: String,
    pub kind: ParamKind,
}

fn default_param_key() -> String {
    "a2".into()
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FixedParam {
    pub key: String,
    pub kind: ParamKind,
    pub value: i64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct OptionDef {
    pub value: i64,
    pub label: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub field: String,
    pub eq: Option<i64>,
    pub ne: Option<i64>,
    pub reason: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CommandDef {
    pub id: String,
    pub msg: String,
    pub label: String,
    #[serde(default)]
    pub group: Group,
    pub input: Input,
    pub param: Option<ParamDef>,
    #[serde(default)]
    pub fixed: Vec<FixedParam>,
    #[serde(default)]
    pub options: Vec<OptionDef>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub step: Option<i64>,
    pub unit: Option<String>,
    /// Field that reflects the current setting.
    pub state: Option<String>,
    #[serde(default = "one")]
    pub on_value: i64,
    #[serde(default)]
    pub off_value: i64,
    /// Raw state value that means "on" for switches.
    #[serde(default = "one")]
    pub on_state: i64,
    #[serde(default)]
    pub danger: Danger,
    pub danger_text: Option<String>,
    #[serde(default)]
    pub requires: Vec<Condition>,
    #[serde(default)]
    pub visible_if: Vec<Condition>,
    pub tip: Option<String>,
}

fn one() -> i64 {
    1
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ErrorDef {
    pub code: u32,
    pub text: String,
    #[serde(default)]
    pub severity: Severity,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    #[default]
    Warning,
    Error,
}
