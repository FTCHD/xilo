use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::time::Instant;
use xilo_models::{CommandRequest, FieldMap, Kind, ModelSpec, Value};
use xilo_proto::fragment::fragment;
use xilo_proto::frame::{DIR_DEVICE, FUNCTION_HANDSHAKE, FUNCTION_SESSION};
use xilo_proto::tlv::{self, Item};
use xilo_proto::{Cipher, Cmd, Frame, KeyPair, MsgId, StreamDecoder};

use crate::transport::{
    Advert, Connector, DeviceAddr, Link, LinkEvent, LinkInfo, Scanner, TransportError,
};

const DEVICE_KEY: [u8; 32] = [
    0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0x01, 0x02,
    0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x12, 0x13,
];

/// USB ports switch off after this much time below 1 W, like the C300X.
#[derive(Clone, Copy, Debug)]
pub struct UsbIdle {
    pub after: Duration,
}

pub struct SimDevice {
    pub spec: Arc<ModelSpec>,
    pub state: FieldMap,
    pub serial: String,
    pub ignore: HashSet<u16>,
    pub usb_idle: Option<UsbIdle>,
    pub usb_dead: bool,
    pub drop_next_connect: bool,
    pub received: Vec<MsgId>,
    usb_idle_since: Option<Instant>,
    keys: KeyPair,
    cipher: Option<Cipher>,
    decoder: StreamDecoder,
}

impl SimDevice {
    pub fn new(spec: Arc<ModelSpec>, serial: &str) -> Self {
        let mut state = FieldMap::new();
        for (id, value) in [
            ("battery_soc", Value::Int(80)),
            ("battery_state", Value::Int(0)),
            ("temperature", Value::Int(25)),
            ("serial_number", Value::Text(serial.to_string())),
            ("firmware", Value::Text("1.2.5.7".into())),
            ("ac_switch", Value::Bool(false)),
            ("dc_switch", Value::Bool(false)),
            ("light_mode", Value::Int(0)),
            ("screen_on", Value::Bool(true)),
            ("screen_brightness", Value::Int(2)),
            ("screen_timeout", Value::Int(60)),
            ("device_timeout", Value::Int(0)),
            ("ac_charge_power", Value::Int(300)),
            ("ac_frequency", Value::Int(50)),
            ("temperature_unit", Value::Int(0)),
            ("car_saving_mode", Value::Int(1)),
            ("ac_smart_mode", Value::Int(1)),
            ("port_memory", Value::Bool(true)),
            ("port_memory_visible", Value::Int(1)),
            ("upgrading", Value::Bool(false)),
        ] {
            if spec.field(id).is_some() {
                state.insert(id.into(), value);
            }
        }
        Self {
            spec,
            state,
            serial: serial.to_string(),
            ignore: HashSet::new(),
            usb_idle: None,
            usb_dead: false,
            drop_next_connect: false,
            received: Vec::new(),
            usb_idle_since: None,
            keys: KeyPair::from_bytes(&DEVICE_KEY)
                .unwrap_or_else(|_| crate::identity::new_key_pair()),
            cipher: None,
            decoder: StreamDecoder::new(),
        }
    }

    pub fn set(&mut self, id: &str, value: Value) {
        self.state.insert(id.into(), value);
    }

    fn reset_link(&mut self) {
        self.cipher = None;
        self.decoder.reset();
    }

    /// Advances simulated time and returns pushes the device would send.
    pub fn tick(&mut self, now: Instant) -> Vec<Vec<u8>> {
        let Some(idle) = self.usb_idle else {
            return Vec::new();
        };
        let usb_power: i64 = [
            "usb_c1_power",
            "usb_c2_power",
            "usb_c3_power",
            "usb_a_power",
        ]
        .iter()
        .filter_map(|id| self.state.get(*id).and_then(Value::as_i64))
        .sum();
        if usb_power >= 1 || self.usb_dead {
            self.usb_idle_since = None;
            return Vec::new();
        }
        let since = *self.usb_idle_since.get_or_insert(now);
        if now.duration_since(since) >= idle.after {
            self.usb_dead = true;
            for id in [
                "usb_c1_state",
                "usb_c2_state",
                "usb_c3_state",
                "usb_a_state",
            ] {
                if self.spec.field(id).is_some() {
                    self.state.insert(id.into(), Value::Int(0));
                }
            }
        }
        Vec::new()
    }

    /// Simulates a phone drawing power from a USB port.
    pub fn draw_usb(&mut self, watts: i64) -> bool {
        if self.usb_dead {
            return false;
        }
        self.state.insert("usb_c3_power".into(), Value::Int(watts));
        self.state
            .insert("usb_c3_state".into(), Value::Int(i64::from(watts > 0)));
        true
    }

    fn receive(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        self.decoder.push(bytes, &mut frames);
        let mut out = Vec::new();
        for frame in frames.into_iter().flatten() {
            out.extend(self.on_frame(frame));
        }
        out
    }

    fn on_frame(&mut self, frame: Frame) -> Vec<Vec<u8>> {
        if frame.function == FUNCTION_HANDSHAKE && !frame.cmd.is_encrypted() {
            return self.handshake(frame);
        }
        if frame.cmd.is_encrypted() && frame.function == FUNCTION_HANDSHAKE {
            return Vec::new();
        }
        let Some(cipher) = &self.cipher else {
            return Vec::new();
        };
        let Ok(plain) = cipher.open(&frame.body) else {
            return Vec::new();
        };
        let payload = tlv::parse(&plain, false, true);
        let id = frame.cmd.id();
        self.received.push(id);
        if self.ignore.contains(&id.0) {
            return Vec::new();
        }
        match id.0 {
            0x0040 => self.state_reply(0xc840),
            0x0041 => {
                let mut body = tlv::SESSION_HEADER.to_vec();
                body[2] = 0x31;
                body.insert(0, 0x00);
                for (key, id) in [(0xa2u8, "ac_timer_remaining"), (0xa3, "dc_timer_remaining")] {
                    let v = self.state.get(id).and_then(Value::as_i64).unwrap_or(0) as u32;
                    body.extend([key, 5, tlv::TYPE_U32]);
                    body.extend(v.to_le_bytes());
                }
                vec![self.session_frame(0x4841, &body)]
            }
            _ => self.command(id, &payload),
        }
    }

    fn command(&mut self, id: MsgId, payload: &tlv::Payload) -> Vec<Vec<u8>> {
        let value = payload.get(0xa2).and_then(Item::uint).map(|v| v as i64);
        let command = self
            .spec
            .commands
            .values()
            .filter(|c| MsgId::parse(&c.msg) == Some(id))
            .find(|c| {
                c.input != xilo_models::Input::Switch
                    || value.is_none_or(|v| v == c.on_value || v == c.off_value)
            })
            .cloned();
        let Some(command) = command else {
            return Vec::new();
        };
        if let Some((field, new)) = self
            .spec
            .optimistic(&CommandRequest::new(command.id.clone(), value))
        {
            self.state.insert(field, new);
        }
        if command.id == "dc_switch" && value == Some(1) && self.usb_dead {
            self.usb_dead = false;
            self.usb_idle_since = None;
        }
        let ack = [0x00, 0xa1, 0x01, 0x31];
        let mut out = vec![self.session_frame(id.response().0 | 0x4000, &ack)];
        out.extend(self.state_reply(0xc405));
        out
    }

    fn handshake(&mut self, frame: Frame) -> Vec<Vec<u8>> {
        let request = tlv::parse(&frame.body, false, false);
        let reply = |cmd: u16, body: Vec<u8>| {
            Frame {
                dir: 0x00,
                function: FUNCTION_HANDSHAKE,
                cmd: Cmd(cmd),
                body,
            }
            .encode()
        };
        match frame.cmd.0 {
            0x0001 => {
                self.reset_link();
                vec![reply(0x0801, vec![0x00, 0xa1, 0x01, 0x01])]
            }
            0x0003 => vec![reply(
                0x0803,
                vec![
                    0x00, 0xa1, 0x01, 0x02, 0xa2, 0x02, 0xfd, 0x00, 0xa3, 0x01, 0x44, 0xa4, 0x01,
                    0x01, 0xa5, 0x01, 0x02,
                ],
            )],
            0x0029 => {
                let mut body = vec![0x00, 0xa1, 0x01, 0x03];
                body.extend(tlv::build_untyped(&[
                    (0xa2, b"ESP32"),
                    (0xa3, b"0.0.0.3"),
                    (0xa4, self.serial.as_bytes()),
                    (0xa5, &[1, 2, 3, 4, 5, 6]),
                ]));
                vec![reply(0x0829, body)]
            }
            0x0005 => vec![reply(0x0805, vec![0x00])],
            0x0021 => {
                let Some(host) = request.get(0xa1) else {
                    return Vec::new();
                };
                let Ok(secret) = self.keys.shared_secret(&host.data) else {
                    return Vec::new();
                };
                self.cipher = Some(Cipher::cbc_from_secret(&secret));
                let mut body = vec![0x00];
                body.extend(tlv::build_untyped(&[(0xa1, &self.keys.public_xy())]));
                vec![reply(0x0821, body)]
            }
            _ => Vec::new(),
        }
    }

    fn session_frame(&self, cmd: u16, plain: &[u8]) -> Vec<u8> {
        let body = self
            .cipher
            .as_ref()
            .map(|c| c.seal(plain))
            .unwrap_or_default();
        Frame {
            dir: DIR_DEVICE,
            function: FUNCTION_SESSION,
            cmd: Cmd(cmd),
            body,
        }
        .encode()
    }

    /// State on every state message of the model, or on `preferred` if the model uses it.
    fn state_reply(&self, preferred: u16) -> Vec<Vec<u8>> {
        let msgs: Vec<u16> = self
            .spec
            .family
            .state_msgs
            .iter()
            .filter_map(|m| u16::from_str_radix(m, 16).ok())
            .collect();
        if msgs.contains(&(preferred & !0xc000)) {
            return self.state_frames(preferred);
        }
        msgs.iter()
            .flat_map(|m| self.state_frames(0xc000 | m))
            .collect()
    }

    fn state_frames(&self, cmd: u16) -> Vec<Vec<u8>> {
        let plain = self.state_plaintext(cmd & !0xc000);
        let body = self
            .cipher
            .as_ref()
            .map(|c| c.seal(&plain))
            .unwrap_or_default();
        let frame = Frame {
            dir: DIR_DEVICE,
            function: FUNCTION_SESSION,
            cmd: Cmd(cmd & !0x8000),
            body,
        };
        fragment(frame, 240)
            .into_iter()
            .map(|f| f.encode())
            .collect()
    }

    fn state_plaintext(&self, msg: u16) -> Vec<u8> {
        let mut out = vec![0xa1, 0x01, 0x31];
        let mut packed: HashMap<u8, Vec<u8>> = HashMap::new();
        let mut keys: Vec<(u8, u8, Vec<u8>)> = Vec::new();
        let applies = |f: &&xilo_models::schema::FieldDef| {
            f.msgs.is_empty() || f.msgs.iter().any(|m| u16::from_str_radix(m, 16) == Ok(msg))
        };
        for field in self.spec.fields.iter().filter(applies) {
            let Some(key) = u8::from_str_radix(&field.key, 16).ok() else {
                continue;
            };
            let value = self
                .state
                .get(&field.id)
                .cloned()
                .unwrap_or(match field.kind {
                    Kind::Text => Value::Text(String::new()),
                    Kind::VersionDigits => Value::Text("1.0.0.0".into()),
                    _ => Value::Int(0),
                });
            if let Some(offset) = field.offset {
                let buf = packed
                    .entry(key)
                    .or_insert_with(|| vec![0; if key == 0xca { 2 } else { 20 }]);
                if offset < buf.len() {
                    buf[offset] = value.as_i64().unwrap_or(0) as u8;
                }
                continue;
            }
            if field.kind == Kind::Bit {
                let bit = value.as_bool().unwrap_or(false);
                keys.retain(|k| k.0 != key);
                keys.push((key, tlv::TYPE_U32, u32::from(bit).to_le_bytes().to_vec()));
                continue;
            }
            if keys.iter().any(|k| k.0 == key) {
                continue;
            }
            if let Some((key, ty, data)) = self.spec.encode_field(field, &value) {
                keys.push((key, ty, data));
            }
        }
        for (key, buf) in packed {
            let ty = if buf.len() == 2 {
                tlv::TYPE_U16
            } else {
                tlv::TYPE_BYTES
            };
            keys.push((key, ty, buf));
        }
        keys.sort_by_key(|k| k.0);
        for (key, ty, data) in keys {
            out.push(key);
            out.push((data.len() + 1) as u8);
            out.push(ty);
            out.extend(data);
        }
        out
    }
}

#[derive(Default, Clone)]
pub struct SimConnector {
    devices: Arc<Mutex<HashMap<String, Arc<Mutex<SimDevice>>>>>,
}

impl SimConnector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&self, addr: &str, device: SimDevice) -> Arc<Mutex<SimDevice>> {
        let device = Arc::new(Mutex::new(device));
        lock(&self.devices).insert(addr.to_string(), device.clone());
        device
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[async_trait]
impl Connector for SimConnector {
    async fn connect(&self, addr: &DeviceAddr) -> Result<Box<dyn Link>, TransportError> {
        let device = lock(&self.devices)
            .get(&addr.id)
            .cloned()
            .ok_or(TransportError::NotFound)?;
        {
            let mut d = lock(&device);
            if d.drop_next_connect {
                d.drop_next_connect = false;
                return Err(TransportError::Connect("simulated failure".into()));
            }
            d.reset_link();
        }
        Ok(Box::new(SimLink {
            device,
            outbox: std::collections::VecDeque::new(),
            info: LinkInfo {
                gatt: "g1",
                max_write: 180,
            },
            closed: false,
        }))
    }
}

#[async_trait]
impl Scanner for SimConnector {
    async fn scan(&self, _window: Duration) -> Result<Vec<Advert>, TransportError> {
        Ok(lock(&self.devices)
            .iter()
            .map(|(addr, d)| Advert {
                addr: Some(DeviceAddr::new(addr.clone())),
                name: Some(lock(d).spec.name.clone()),
                rssi: Some(-50),
                services: vec![xilo_proto::gatt::G1.advertised.into()],
                ..Default::default()
            })
            .collect())
    }
}

pub struct SimLink {
    device: Arc<Mutex<SimDevice>>,
    outbox: std::collections::VecDeque<Vec<u8>>,
    info: LinkInfo,
    closed: bool,
}

#[async_trait]
impl Link for SimLink {
    fn info(&self) -> &LinkInfo {
        &self.info
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::Disconnected);
        }
        let replies = lock(&self.device).receive(bytes);
        self.outbox.extend(replies);
        Ok(())
    }

    async fn next_event(&mut self) -> Option<LinkEvent> {
        loop {
            if self.closed {
                return None;
            }
            if let Some(bytes) = self.outbox.pop_front() {
                return Some(LinkEvent::Notify(bytes));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            let pushes = lock(&self.device).tick(Instant::now());
            self.outbox.extend(pushes);
        }
    }

    async fn close(&mut self) {
        self.closed = true;
    }
}
