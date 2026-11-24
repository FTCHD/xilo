use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, OnceLock};

use indexmap::IndexMap;
use xilo_proto::tlv;
use xilo_proto::{Message, MsgId};

use crate::schema::{FamilyFile, Input, Kind, ModelFile};
use crate::spec::{ModelSpec, parse_key};

mod builtin {
    include!(concat!(env!("OUT_DIR"), "/builtin.rs"));
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
#[error("{file}: {message}")]
pub struct SpecError {
    pub file: String,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct Registry {
    models: IndexMap<String, Arc<ModelSpec>>,
}

/// What we know about a device when picking its model.
#[derive(Clone, Debug, Default)]
pub struct Evidence<'a> {
    pub saved_model: Option<&'a str>,
    pub serial: Option<&'a str>,
    pub state: Option<&'a Message>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub code: String,
    pub score: f64,
    pub reason: &'static str,
}

impl Registry {
    pub fn builtin() -> &'static Registry {
        static REGISTRY: OnceLock<Registry> = OnceLock::new();
        REGISTRY.get_or_init(|| {
            let files: Vec<(String, String)> = builtin::FILES
                .iter()
                .map(|(p, c)| ((*p).to_string(), (*c).to_string()))
                .collect();
            let (registry, errors) = Self::from_files(&files);
            assert!(errors.is_empty(), "built-in models are invalid: {errors:?}");
            registry
        })
    }

    /// Built-in models plus TOML files from `dir`, which override built-ins.
    pub fn with_user_dir(dir: &Path) -> (Registry, Vec<SpecError>) {
        let mut files: Vec<(String, String)> = builtin::FILES
            .iter()
            .map(|(p, c)| ((*p).to_string(), (*c).to_string()))
            .collect();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&current) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "toml")
                    && let Ok(content) = std::fs::read_to_string(&path)
                {
                    let rel = path
                        .strip_prefix(dir)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .into_owned();
                    files.retain(|(p, _)| p != &rel);
                    files.push((rel, content));
                }
            }
        }
        Self::from_files(&files)
    }

    pub fn from_files(files: &[(String, String)]) -> (Registry, Vec<SpecError>) {
        let mut errors = Vec::new();
        let mut families = IndexMap::new();
        let mut models = Vec::new();
        for (path, content) in files {
            let err = |message: String| SpecError {
                file: path.clone(),
                message,
            };
            if path.ends_with("family.toml") {
                match toml::from_str::<FamilyFile>(content) {
                    Ok(f) => {
                        families.insert(f.family.id.clone(), f);
                    }
                    Err(e) => errors.push(err(e.to_string())),
                }
            } else {
                match toml::from_str::<ModelFile>(content) {
                    Ok(m) => models.push((path.clone(), m)),
                    Err(e) => errors.push(err(e.to_string())),
                }
            }
        }
        let mut registry = Registry::default();
        for (path, model) in models {
            let Some(family) = families.get(&model.model.family) else {
                errors.push(SpecError {
                    file: path,
                    message: format!("unknown family {}", model.model.family),
                });
                continue;
            };
            match resolve(family, model) {
                Ok(spec) => {
                    registry.models.insert(spec.code.clone(), Arc::new(spec));
                }
                Err(message) => errors.push(SpecError {
                    file: path,
                    message,
                }),
            }
        }
        (registry, errors)
    }

    pub fn get(&self, code: &str) -> Option<&Arc<ModelSpec>> {
        self.models.get(code)
    }

    pub fn all(&self) -> impl Iterator<Item = &Arc<ModelSpec>> {
        self.models.values()
    }

    /// Guesses the model from SOLIX manufacturer data (company 0xffff).
    /// Bytes 10..13 hold serial characters 3..6, which overlap the prefix.
    pub fn guess_from_advert(&self, data: &[u8]) -> Vec<&Arc<ModelSpec>> {
        let Some(chars) = data.get(10..12).and_then(|c| std::str::from_utf8(c).ok()) else {
            return Vec::new();
        };
        self.all()
            .filter(|m| {
                m.serial_prefixes
                    .iter()
                    .any(|p| p.get(3..5).is_some_and(|s| s.eq_ignore_ascii_case(chars)))
            })
            .collect()
    }

    /// Ranks models for a device, best first.
    pub fn detect(&self, evidence: &Evidence<'_>) -> Vec<Candidate> {
        if let Some(code) = evidence.saved_model
            && self.models.contains_key(code)
        {
            return vec![Candidate {
                code: code.into(),
                score: 1.0,
                reason: "saved",
            }];
        }
        let mut out: Vec<Candidate> = Vec::new();
        for spec in self.models.values() {
            let mut score = 0.0;
            let mut reason = "fingerprint";
            let from_state = evidence.state.and_then(|s| serial_in_state(spec, s));
            if let Some(serial) = evidence.serial.or(from_state.as_deref())
                && spec
                    .serial_prefixes
                    .iter()
                    .any(|p| serial.starts_with(p.as_str()))
            {
                score += 0.6;
                reason = "serial";
            }
            if let Some(state) = evidence.state {
                score += 0.4 * fingerprint(spec, state);
            }
            if score > 0.0 {
                out.push(Candidate {
                    code: spec.code.clone(),
                    score,
                    reason,
                });
            }
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        out
    }
}

fn serial_in_state(spec: &ModelSpec, message: &Message) -> Option<String> {
    let field = spec.field("serial_number")?;
    if !spec.is_state_msg(message.id()) {
        return None;
    }
    let item = message.payload.get(parse_key(&field.key)?)?;
    if item.ty != Some(tlv::TYPE_STR) {
        return None;
    }
    Some(item.text())
}

/// How well a state message matches a model: keys and their type bytes.
pub fn fingerprint(spec: &ModelSpec, message: &Message) -> f64 {
    if !spec.is_state_msg(message.id()) {
        return 0.0;
    }
    let mut expected: HashMap<u8, Option<u8>> = HashMap::new();
    for field in spec.fields.iter().filter(|f| f.msgs.is_empty()) {
        let Some(key) = parse_key(&field.key) else {
            continue;
        };
        let ty = if field.offset.is_some() {
            None
        } else {
            type_byte(field.kind)
        };
        expected.entry(key).or_insert(ty);
    }
    let seen: HashSet<u8> = message.payload.items.iter().map(|i| i.key).collect();
    if expected.is_empty() || seen.is_empty() {
        return 0.0;
    }
    let mut score = 0.0;
    for item in &message.payload.items {
        match expected.get(&item.key) {
            Some(Some(ty)) if item.ty == Some(*ty) => score += 1.0,
            Some(Some(_)) => score -= 2.0,
            Some(None) => score += 1.0,
            None => {}
        }
    }
    let union = expected.keys().filter(|k| !seen.contains(k)).count() + seen.len();
    (score / union as f64).max(0.0)
}

fn type_byte(kind: Kind) -> Option<u8> {
    match kind {
        Kind::U8 | Kind::I8 | Kind::Bool => Some(tlv::TYPE_U8),
        Kind::U16 | Kind::I16 | Kind::VersionDigits => Some(tlv::TYPE_U16),
        Kind::U32 | Kind::I32 => Some(tlv::TYPE_U32),
        Kind::Text => Some(tlv::TYPE_STR),
        Kind::Bit | Kind::Hex | Kind::F32 | Kind::PrefixedText | Kind::Auto => None,
    }
}

fn resolve(family: &FamilyFile, model: ModelFile) -> Result<ModelSpec, String> {
    let mut fields = family.field.clone();
    for f in model.field {
        fields.retain(|x| x.id != f.id);
        fields.push(f);
    }
    fields.retain(|f| !model.model.drop_fields.contains(&f.id));

    let mut commands = IndexMap::new();
    for c in family.command.iter().cloned().chain(model.command) {
        commands.insert(c.id.clone(), c);
    }
    commands.retain(|id, _| !model.model.drop_commands.contains(id));

    let mut errors = IndexMap::new();
    for e in family.error.iter().cloned().chain(model.error) {
        errors.insert(e.code, e);
    }

    let field_ids: HashSet<&str> = fields.iter().map(|f| f.id.as_str()).collect();
    for f in &fields {
        if parse_key(&f.key).is_none() {
            return Err(format!("field {}: bad key {}", f.id, f.key));
        }
        for m in &f.msgs {
            MsgId::parse(m).ok_or_else(|| format!("field {}: bad message {m}", f.id))?;
        }
    }
    for c in commands.values() {
        MsgId::parse(&c.msg).ok_or_else(|| format!("command {}: bad message {}", c.id, c.msg))?;
        if let Some(state) = &c.state
            && !field_ids.contains(state.as_str())
        {
            return Err(format!("command {}: unknown state field {state}", c.id));
        }
        for cond in c.requires.iter().chain(&c.visible_if) {
            if !field_ids.contains(cond.field.as_str()) {
                return Err(format!("command {}: unknown field {}", c.id, cond.field));
            }
        }
        match c.input {
            Input::Choice if c.options.is_empty() => {
                return Err(format!("command {}: choice without options", c.id));
            }
            Input::Switch | Input::Choice | Input::Number if c.param.is_none() => {
                return Err(format!("command {}: input without param", c.id));
            }
            _ => {}
        }
    }
    let fam = &family.family;
    for periodic in [&fam.keepalive, &fam.poll].into_iter().flatten() {
        if !commands.contains_key(&periodic.command) {
            return Err(format!("periodic command {} missing", periodic.command));
        }
    }
    let assets = model
        .model
        .assets
        .clone()
        .unwrap_or_else(|| model.model.code.to_lowercase());
    Ok(ModelSpec {
        code: model.model.code,
        name: model.model.name,
        status: model.model.status,
        family: fam.clone(),
        serial_prefixes: model.model.serial_prefixes,
        assets,
        fields,
        commands,
        errors,
        state_msgs: Vec::new(),
    }
    .finish())
}
