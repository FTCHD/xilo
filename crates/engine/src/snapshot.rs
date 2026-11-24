use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::SystemTime;

use indexmap::IndexMap;
use xilo_models::{FieldMap, ModelSpec, Value};
use xilo_proto::{MsgId, Payload, SessionInfo};

use crate::transport::DeviceAddr;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LinkState {
    Disconnected,
    Connecting,
    Handshaking,
    Ready,
    Released,
}

impl LinkState {
    pub fn label(self) -> &'static str {
        match self {
            LinkState::Disconnected => "Disconnected",
            LinkState::Connecting => "Connecting...",
            LinkState::Handshaking => "Connecting...",
            LinkState::Ready => "Connected",
            LinkState::Released => "Released to phone",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub key: String,
    pub addr: DeviceAddr,
    pub name: Option<String>,
    pub model: Option<Arc<ModelSpec>>,
    pub link: LinkState,
    pub info: Option<SessionInfo>,
    pub fields: FieldMap,
    pub raw: IndexMap<MsgId, Payload>,
    pub updated: Option<SystemTime>,
    pub unsupported: BTreeSet<String>,
    pub pending: BTreeSet<String>,
    pub last_error: Option<String>,
    pub allow_unverified: bool,
}

impl Snapshot {
    pub fn new(key: String, addr: DeviceAddr) -> Self {
        Self {
            key,
            addr,
            name: None,
            model: None,
            link: LinkState::Disconnected,
            info: None,
            fields: FieldMap::new(),
            raw: IndexMap::new(),
            updated: None,
            unsupported: BTreeSet::new(),
            pending: BTreeSet::new(),
            last_error: None,
            allow_unverified: false,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.link == LinkState::Ready
    }

    pub fn get(&self, id: &str) -> Option<&Value> {
        self.fields.get(id)
    }

    pub fn int(&self, id: &str) -> Option<i64> {
        self.fields.get(id).and_then(Value::as_i64)
    }

    pub fn float(&self, id: &str) -> Option<f64> {
        self.fields.get(id).and_then(Value::as_f64)
    }

    pub fn flag(&self, id: &str) -> bool {
        self.fields
            .get(id)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn text(&self, id: &str) -> Option<&str> {
        self.fields.get(id).and_then(Value::as_text)
    }

    pub fn serial(&self) -> Option<&str> {
        self.text("serial_number")
            .or_else(|| self.info.as_ref().and_then(|i| i.serial.as_deref()))
    }

    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .or_else(|| self.model.as_ref().map(|m| m.name.clone()))
            .unwrap_or_else(|| "SOLIX device".into())
    }
}
