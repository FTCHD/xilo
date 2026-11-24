use std::time::{Duration, Instant};

use crate::cmd::{Cmd, MsgId};
use crate::crypto::{Cipher, CryptoError, KeyPair};
use crate::fragment::{Reassembler, fragment};
use crate::frame::{FUNCTION_HANDSHAKE, FUNCTION_SESSION, Frame, FrameError, StreamDecoder};
use crate::tlv::{self, Item, Payload, TimestampStyle, Value};

/// Base timestamp agreed in SolixBLE's handshake (`42ad8c69`).
pub const SOLIX_BASE_TIMESTAMP: u32 = 0x698c_ad42;

/// The authentication frame SolixBLE sends after the key exchange. Devices
/// accept it even though it was encrypted with another session's key.
pub const CANNED_AUTH_FRAME: &str = "ff095a000300014022580bc0532a53c739adf3da7b994a7b5f221bcc16bab6392c215cb4faaf41d9d58e2c81c016e474c78eed5569147cb74a1f22ca2b3fad2e209dbbcfbdaca352034a6c479f055f68581b5f1e22348809f526";

const CAPABILITY_ECDH: u8 = 0x40;
const SILENCE_RETRY: Duration = Duration::from_secs(15);
const GIVE_UP: Duration = Duration::from_secs(90);
const AUTH_WAIT: Duration = Duration::from_millis(1500);
const PROBE_AFTER: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimestampMode {
    /// Current UTC seconds, as the Anker app sends.
    Utc,
    /// A fixed base plus the seconds since the session started, as SolixBLE sends.
    SessionBase(u32),
}

/// Which handshake and encryption the device uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Flavor {
    /// ECDH with AES-CBC, plain handshake frames.
    #[default]
    Zx,
    /// AES-GCM with a static key for the handshake (Prime chargers and power banks).
    Prime,
}

#[derive(Clone)]
pub struct ZxConfig {
    pub client_id: String,
    pub keys: KeyPair,
    pub timestamps: TimestampMode,
    pub max_write: usize,
    pub flavor: Flavor,
    /// Try the other flavor when the device does not answer the first frame.
    pub probe: bool,
    /// POSIX time zone sent to Prime devices, for example `<+01>-1`.
    pub timezone: String,
    pub country: String,
}

#[derive(Clone, Copy, Debug)]
pub struct Now {
    pub instant: Instant,
    pub unix: u32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Request {
    pub id: MsgId,
    pub params: Vec<(u8, Value)>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SessionInfo {
    pub module: String,
    pub module_firmware: String,
    pub serial: Option<String>,
    pub mac: Option<[u8; 6]>,
    pub mtu: u16,
    pub capability: u8,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Message {
    pub cmd: Cmd,
    pub payload: Payload,
}

impl Message {
    pub fn id(&self) -> MsgId {
        self.cmd.id()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Trace {
    pub outgoing: bool,
    pub bytes: Vec<u8>,
    pub cmd: Option<Cmd>,
    pub plain: Option<Vec<u8>>,
    pub note: Option<String>,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ProtoError {
    #[error("the device did not finish the handshake")]
    HandshakeTimeout,
    #[error(
        "the device asked for an authentication scheme Xilo does not support (capability {0:#04x})"
    )]
    UnsupportedAuth(u8),
    #[error("key exchange failed: {0}")]
    Crypto(#[from] CryptoError),
    #[error("the session is not ready")]
    NotReady,
}

#[derive(Debug)]
pub enum Output {
    Write(Vec<u8>),
    Ready(SessionInfo),
    Message(Message),
    Trace(Trace),
    Failed(ProtoError),
}

pub enum Input<'a> {
    Connected,
    Notify(&'a [u8]),
    Tick,
    Send(Request),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Idle,
    Start,
    Capability,
    Info,
    SetCapability,
    PublicKey,
    Auth,
    Bind,
    LegacyKey,
    LegacyBind,
    Ready,
    Failed,
}

pub struct ZxSession {
    config: ZxConfig,
    flavor: Flavor,
    /// The device lacks ECDH, so the older key exchange is used.
    legacy: bool,
    phase: Phase,
    decoder: StreamDecoder,
    reassembler: Reassembler,
    cipher: Option<Cipher>,
    info: SessionInfo,
    started: Option<Instant>,
    last_rx: Option<Instant>,
    session_start: Option<Instant>,
    auth_sent: Option<Instant>,
}

impl ZxSession {
    pub fn new(config: ZxConfig) -> Self {
        Self {
            flavor: config.flavor,
            legacy: false,
            config,
            phase: Phase::Idle,
            decoder: StreamDecoder::new(),
            reassembler: Reassembler::new(),
            cipher: None,
            info: SessionInfo::default(),
            started: None,
            last_rx: None,
            session_start: None,
            auth_sent: None,
        }
    }

    /// A ready session for a known shared secret, used to replay captures.
    pub fn with_secret(
        config: ZxConfig,
        secret: &[u8; 32],
        info: SessionInfo,
        now: Instant,
    ) -> Self {
        let mut session = Self::new(config);
        session.cipher = Some(Cipher::cbc_from_secret(secret));
        session.info = info;
        session.phase = Phase::Ready;
        session.started = Some(now);
        session.session_start = Some(now);
        session
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn is_ready(&self) -> bool {
        self.phase == Phase::Ready
    }

    pub fn info(&self) -> &SessionInfo {
        &self.info
    }

    pub fn flavor(&self) -> Flavor {
        self.flavor
    }

    pub fn set_max_write(&mut self, max_write: usize) {
        self.config.max_write = max_write.max(20);
    }

    pub fn handle(&mut self, now: Now, input: Input<'_>, out: &mut Vec<Output>) {
        match input {
            Input::Connected => self.start(now, out),
            Input::Notify(bytes) => self.receive(now, bytes, out),
            Input::Tick => self.tick(now, out),
            Input::Send(request) => self.send(now, request, out),
        }
    }

    fn start(&mut self, now: Now, out: &mut Vec<Output>) {
        self.decoder.reset();
        self.reassembler.clear();
        self.cipher = None;
        self.legacy = false;
        self.info = SessionInfo::default();
        self.session_start = None;
        self.auth_sent = None;
        self.started = Some(now.instant);
        self.last_rx = Some(now.instant);
        self.phase = Phase::Start;
        let ts = self.handshake_timestamp(now, 0);
        match self.flavor {
            Flavor::Zx => {
                let body =
                    tlv::build_untyped(&[(0xa1, &ts), (0xa2, self.config.client_id.as_bytes())]);
                self.write_handshake(Cmd(0x0001), body, out);
            }
            Flavor::Prime => {
                self.cipher = Some(Cipher::prime_handshake());
                let body = tlv::build_untyped(&[(0xa1, &ts)]);
                self.write_handshake(Cmd(0x4001), body, out);
            }
        }
    }

    fn tick(&mut self, now: Now, out: &mut Vec<Output>) {
        self.reassembler.expire(now.instant);
        match self.phase {
            Phase::Idle | Phase::Ready | Phase::Failed => {}
            Phase::Auth if self.flavor == Flavor::Zx => {
                if self
                    .auth_sent
                    .is_some_and(|t| now.instant.duration_since(t) >= AUTH_WAIT)
                {
                    self.ready(out);
                }
            }
            Phase::Start
                if self.config.probe
                    && self
                        .last_rx
                        .is_some_and(|t| now.instant.duration_since(t) >= PROBE_AFTER) =>
            {
                self.flavor = match self.flavor {
                    Flavor::Zx => Flavor::Prime,
                    Flavor::Prime => Flavor::Zx,
                };
                let started = self.started;
                self.start(now, out);
                self.started = started;
            }
            _ => {
                let started = self.started.unwrap_or(now.instant);
                if now.instant.duration_since(started) >= GIVE_UP {
                    self.phase = Phase::Failed;
                    out.push(Output::Failed(ProtoError::HandshakeTimeout));
                    return;
                }
                let quiet = self
                    .last_rx
                    .is_some_and(|t| now.instant.duration_since(t) >= SILENCE_RETRY);
                if quiet {
                    let started = self.started;
                    self.start(now, out);
                    self.started = started;
                }
            }
        }
    }

    fn receive(&mut self, now: Now, bytes: &[u8], out: &mut Vec<Output>) {
        self.last_rx = Some(now.instant);
        let mut frames = Vec::new();
        self.decoder.push(bytes, &mut frames);
        for frame in frames {
            match frame {
                Ok(frame) => self.receive_frame(now, frame, out),
                Err(err) => out.push(Output::Trace(Trace {
                    outgoing: false,
                    bytes: bytes.to_vec(),
                    cmd: None,
                    plain: None,
                    note: Some(frame_note(&err)),
                })),
            }
        }
    }

    fn receive_frame(&mut self, now: Now, frame: Frame, out: &mut Vec<Output>) {
        let raw = frame.encode();
        let Some(frame) = self.reassembler.push(frame, now.instant) else {
            out.push(trace_in(raw, None, None, Some("fragment".into())));
            return;
        };
        let plain = if frame.cmd.is_encrypted() {
            match self.decrypt(&frame) {
                Some(plain) => plain,
                None => {
                    out.push(trace_in(
                        raw,
                        Some(frame.cmd),
                        None,
                        Some("decrypt failed".into()),
                    ));
                    return;
                }
            }
        } else {
            frame.body.clone()
        };
        out.push(trace_in(raw, Some(frame.cmd), Some(plain.clone()), None));

        if frame.function == FUNCTION_HANDSHAKE {
            let payload = tlv::parse(&plain, frame.cmd.is_response(), false);
            match self.flavor {
                Flavor::Zx => self.handshake_step(now, frame.cmd, payload, out),
                Flavor::Prime => self.prime_step(now, frame.cmd, payload, out),
            }
            return;
        }
        if !matches!(self.phase, Phase::Ready | Phase::Auth | Phase::Bind) {
            return;
        }
        if !frame.cmd.is_encrypted() {
            return;
        }
        let payload = tlv::parse(&plain, frame.cmd.is_response(), true);
        out.push(Output::Message(Message {
            cmd: frame.cmd,
            payload,
        }));
    }

    fn decrypt(&self, frame: &Frame) -> Option<Vec<u8>> {
        let cipher = self.cipher.as_ref()?;
        if matches!(cipher, Cipher::Cbc { .. }) && frame.body.len() % 16 == 15 {
            let mut fixed = Vec::with_capacity(frame.body.len() + 1);
            fixed.push(frame.cmd.lo());
            fixed.extend_from_slice(&frame.body);
            return cipher.open(&fixed).ok();
        }
        cipher.open(&frame.body).ok()
    }

    fn handshake_step(&mut self, now: Now, cmd: Cmd, payload: Payload, out: &mut Vec<Output>) {
        let id = self.config.client_id.clone();
        match (cmd.id().0, self.phase) {
            (0x0801, Phase::Start) => {
                self.phase = Phase::Capability;
                let ts = self.handshake_timestamp(now, 0);
                let body = tlv::build_untyped(&[
                    (0xa1, &ts),
                    (0xa2, id.as_bytes()),
                    (0xa3, &[0x20]),
                    (0xa4, &[0x00, 0xf0]),
                ]);
                self.write_handshake(Cmd(0x0003), body, out);
            }
            (0x0803, Phase::Capability) => {
                self.info.mtu = payload.get(0xa2).and_then(Item::uint).unwrap_or(0) as u16;
                self.info.capability = payload.get(0xa3).and_then(Item::uint).unwrap_or(0) as u8;
                self.legacy = self.info.capability & CAPABILITY_ECDH == 0;
                self.phase = Phase::Info;
                let ts = self.handshake_timestamp(now, 0);
                let body = tlv::build_untyped(&[(0xa1, &ts), (0xa2, id.as_bytes())]);
                self.write_handshake(Cmd(0x0029), body, out);
            }
            (0x0829, Phase::Info) => {
                self.read_info(&payload);
                self.session_start = Some(now.instant);
                self.phase = Phase::SetCapability;
                let capability = if self.legacy {
                    self.cipher = Some(legacy_cipher(
                        id.as_bytes(),
                        self.info.serial.as_deref().unwrap_or_default(),
                        None,
                    ));
                    self.info.capability
                } else {
                    CAPABILITY_ECDH
                };
                let ts = self.handshake_timestamp(now, 1);
                let body = tlv::build_untyped(&[
                    (0xa1, &ts),
                    (0xa2, id.as_bytes()),
                    (0xa3, &[0x20]),
                    (0xa4, &[0x00, 0xf0]),
                    (0xa5, &[capability]),
                ]);
                self.write_handshake(Cmd(0x0005), body, out);
            }
            (0x0805, Phase::SetCapability) if self.legacy => {
                self.phase = Phase::LegacyKey;
                let ts = self.handshake_timestamp(now, 1);
                let body = tlv::build_untyped(&[(0xa1, &ts), (0xa2, id.as_bytes())]);
                self.write_handshake(Cmd(0x4022), body, out);
            }
            (0x0822, Phase::LegacyKey) => {
                let Some(key) = payload.get(0xa1).and_then(|i| session_key(&i.data)) else {
                    self.phase = Phase::Failed;
                    out.push(Output::Failed(ProtoError::UnsupportedAuth(
                        self.info.capability,
                    )));
                    return;
                };
                self.cipher = Some(legacy_cipher(
                    id.as_bytes(),
                    self.info.serial.as_deref().unwrap_or_default(),
                    Some(key),
                ));
                self.phase = Phase::LegacyBind;
                let ts = self.handshake_timestamp(now, 1);
                let body = tlv::build_untyped(&[(0xa1, &ts), (0xa2, id.as_bytes())]);
                self.write_handshake(Cmd(0x4023), body, out);
            }
            (0x0823, Phase::LegacyBind) => self.ready(out),
            (0x0805, Phase::SetCapability) => {
                self.phase = Phase::PublicKey;
                let key = self.config.keys.public_xy();
                let body = tlv::build_untyped(&[(0xa1, &key)]);
                self.write_handshake(Cmd(0x0021), body, out);
            }
            (0x0821, Phase::PublicKey) => {
                let Some(device_key) = payload.get(0xa1) else {
                    return;
                };
                match self.config.keys.shared_secret(&device_key.data) {
                    Ok(secret) => {
                        self.cipher = Some(Cipher::cbc_from_secret(&secret));
                        self.phase = Phase::Auth;
                        self.auth_sent = Some(now.instant);
                        let canned = decode_hex(CANNED_AUTH_FRAME);
                        out.push(Output::Trace(Trace {
                            outgoing: true,
                            bytes: canned.clone(),
                            cmd: Some(Cmd(0x4022)),
                            plain: None,
                            note: Some("authentication".into()),
                        }));
                        out.push(Output::Write(canned));
                    }
                    Err(err) => {
                        self.phase = Phase::Failed;
                        out.push(Output::Failed(err.into()));
                    }
                }
            }
            (0x0822, Phase::Auth) => self.ready(out),
            _ => {}
        }
    }

    fn read_info(&mut self, payload: &Payload) {
        self.info.module = payload.get(0xa2).map(Item::text).unwrap_or_default();
        self.info.module_firmware = payload.get(0xa3).map(Item::text).unwrap_or_default();
        self.info.serial = payload.get(0xa4).map(Item::text).filter(|s| !s.is_empty());
        self.info.mac = payload
            .get(0xa5)
            .and_then(|i| <[u8; 6]>::try_from(i.data.as_slice()).ok());
    }

    /// The Prime handshake: the same steps as ZX, encrypted with a static
    /// AES-GCM key until the key exchange, then a session setup.
    fn prime_step(&mut self, now: Now, cmd: Cmd, payload: Payload, out: &mut Vec<Output>) {
        let ts = self.handshake_timestamp(now, 0);
        match (cmd.id().0, self.phase) {
            (0x0801, Phase::Start) => {
                self.phase = Phase::Capability;
                let body =
                    tlv::build_untyped(&[(0xa1, &ts), (0xa3, &[0x20]), (0xa4, &[0x00, 0xf0])]);
                self.write_handshake(Cmd(0x4003), body, out);
            }
            (0x0803, Phase::Capability) => {
                self.info.mtu = payload.get(0xa2).and_then(Item::uint).unwrap_or(0) as u16;
                self.info.capability = payload.get(0xa3).and_then(Item::uint).unwrap_or(0) as u8;
                self.phase = Phase::Info;
                self.write_handshake(Cmd(0x4029), tlv::build_untyped(&[(0xa1, &ts)]), out);
            }
            (0x0829, Phase::Info) => {
                self.read_info(&payload);
                self.session_start = Some(now.instant);
                self.phase = Phase::SetCapability;
                let body = tlv::build_untyped(&[
                    (0xa1, &ts),
                    (0xa3, &[0x20]),
                    (0xa4, &[0x29, 0x01]),
                    (0xa5, &[0x44]),
                    (0xa6, &[0x02]),
                ]);
                self.write_handshake(Cmd(0x4005), body, out);
            }
            (0x0805, Phase::SetCapability) => {
                self.phase = Phase::PublicKey;
                let key = self.config.keys.public_xy();
                self.write_handshake(Cmd(0x4021), tlv::build_untyped(&[(0xa1, &key)]), out);
            }
            (0x0821, Phase::PublicKey) => {
                let Some(device_key) = payload.get(0xa1) else {
                    return;
                };
                match self.config.keys.shared_secret(&device_key.data) {
                    Ok(secret) => {
                        self.cipher = Some(Cipher::gcm_from_secret(&secret));
                        self.phase = Phase::Auth;
                        let ts = self.handshake_timestamp(now, 1);
                        let body = tlv::build_untyped(&[
                            (0xa1, &ts),
                            (0xa3, &[0, 0, 0, 0]),
                            (0xa5, self.config.timezone.as_bytes()),
                        ]);
                        self.write_handshake(Cmd(0x4022), body, out);
                    }
                    Err(err) => {
                        self.phase = Phase::Failed;
                        out.push(Output::Failed(err.into()));
                    }
                }
            }
            (0x0822, Phase::Auth) => {
                self.phase = Phase::Bind;
                let ts = self.handshake_timestamp(now, 1);
                let id = self.config.client_id.clone();
                let body = tlv::build_untyped(&[(0xa1, &ts), (0xa2, id.as_bytes())]);
                self.write_handshake(Cmd(0x4027), body, out);
            }
            (0x0827, Phase::Bind) => {
                if let Some(status) = payload.status.filter(|s| *s != 0) {
                    out.push(Output::Trace(Trace {
                        outgoing: false,
                        bytes: Vec::new(),
                        cmd: Some(cmd),
                        plain: None,
                        note: Some(format!("bind status {status:#04x}")),
                    }));
                }
                self.phase = Phase::Ready;
                let country = self.config.country.clone();
                let id = self.config.client_id.clone();
                self.write_session(now, MsgId(0x0200), &[], out);
                self.write_session(
                    now,
                    MsgId(0x020a),
                    &[
                        (0xa2, Value::Bytes(country.into_bytes())),
                        (0xa3, Value::Bytes(id.into_bytes())),
                        (0xa5, Value::U8(1)),
                    ],
                    out,
                );
                out.push(Output::Ready(self.info.clone()));
            }
            _ => {}
        }
    }

    fn ready(&mut self, out: &mut Vec<Output>) {
        if self.phase == Phase::Ready {
            return;
        }
        self.phase = Phase::Ready;
        out.push(Output::Ready(self.info.clone()));
    }

    fn send(&mut self, now: Now, request: Request, out: &mut Vec<Output>) {
        if self.phase != Phase::Ready || self.cipher.is_none() {
            out.push(Output::Failed(ProtoError::NotReady));
            return;
        }
        self.write_session(now, request.id, &request.params, out);
    }

    fn write_session(&self, now: Now, id: MsgId, params: &[(u8, Value)], out: &mut Vec<Output>) {
        let Some(cipher) = self.cipher.as_ref() else {
            return;
        };
        let style = match self.flavor {
            Flavor::Zx => TimestampStyle::Typed,
            Flavor::Prime => TimestampStyle::Untyped,
        };
        let plain = tlv::build_request(params, self.session_timestamp(now), style);
        let body = cipher.seal(&plain);
        let frame = Frame::new(FUNCTION_SESSION, id.cmd(true), body);
        let max_chunk = self.config.max_write.saturating_sub(11).max(16);
        let frames = if frame.encode().len() > self.config.max_write {
            fragment(frame, max_chunk)
        } else {
            vec![frame]
        };
        for (i, f) in frames.into_iter().enumerate() {
            let bytes = f.encode();
            out.push(Output::Trace(Trace {
                outgoing: true,
                bytes: bytes.clone(),
                cmd: Some(f.cmd),
                plain: (i == 0).then(|| plain.clone()),
                note: None,
            }));
            out.push(Output::Write(bytes));
        }
    }

    fn write_handshake(&self, cmd: Cmd, body: Vec<u8>, out: &mut Vec<Output>) {
        let wire = match self.cipher.as_ref() {
            Some(cipher) if cmd.is_encrypted() => cipher.seal(&body),
            _ => body.clone(),
        };
        let bytes = Frame::new(FUNCTION_HANDSHAKE, cmd, wire).encode();
        out.push(Output::Trace(Trace {
            outgoing: true,
            bytes: bytes.clone(),
            cmd: Some(cmd),
            plain: Some(body),
            note: None,
        }));
        out.push(Output::Write(bytes));
    }

    fn handshake_timestamp(&self, now: Now, offset: u32) -> [u8; 4] {
        match self.config.timestamps {
            TimestampMode::Utc => now.unix.to_le_bytes(),
            TimestampMode::SessionBase(base) => base.wrapping_add(offset).to_le_bytes(),
        }
    }

    fn session_timestamp(&self, now: Now) -> u32 {
        match self.config.timestamps {
            TimestampMode::Utc => now.unix,
            TimestampMode::SessionBase(base) => {
                let elapsed = self
                    .session_start
                    .map_or(0, |t| now.instant.duration_since(t).as_secs() as u32);
                let offset = u32::from(self.flavor == Flavor::Prime);
                base.wrapping_add(elapsed).wrapping_add(offset)
            }
        }
    }
}

/// The older AES scheme from the app's `AesConferV1`: the key starts as the
/// first 16 characters of the client id and becomes the key the device sends
/// in `0822`; the IV is the first 16 characters of the serial number.
fn legacy_cipher(client_id: &[u8], serial: &str, key: Option<[u8; 16]>) -> Cipher {
    let mut default = [0u8; 16];
    let n = client_id.len().min(16);
    default[..n].copy_from_slice(&client_id[..n]);
    let iv = if serial.len() >= 16 {
        let mut iv = [0u8; 16];
        iv.copy_from_slice(&serial.as_bytes()[..16]);
        iv
    } else {
        default
    };
    Cipher::Cbc {
        key: key.unwrap_or(default),
        iv,
    }
}

/// The session key in `0822`, sent as 16 raw bytes or as 32 hex characters.
fn session_key(data: &[u8]) -> Option<[u8; 16]> {
    if let Ok(key) = <[u8; 16]>::try_from(data) {
        return Some(key);
    }
    let text = std::str::from_utf8(data).ok()?;
    let bytes = decode_hex(text.trim());
    <[u8; 16]>::try_from(bytes.as_slice()).ok()
}

fn trace_in(
    bytes: Vec<u8>,
    cmd: Option<Cmd>,
    plain: Option<Vec<u8>>,
    note: Option<String>,
) -> Output {
    Output::Trace(Trace {
        outgoing: false,
        bytes,
        cmd,
        plain,
        note,
    })
}

fn frame_note(err: &FrameError) -> String {
    format!("invalid frame: {err}")
}

pub(crate) fn decode_hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}
