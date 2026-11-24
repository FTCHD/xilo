use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::Deserialize;
use xilo_proto::frame::Frame;
use xilo_proto::tlv;
use xilo_proto::zx::{SOLIX_BASE_TIMESTAMP, ZxSession};
use xilo_proto::{
    Cipher, Input, KeyPair, MsgId, Now, Output, Phase, Request, SessionInfo, TimestampMode,
    ZxConfig,
};

#[derive(Deserialize)]
struct Vectors {
    negotiation: Negotiation,
    handshake: BTreeMap<String, Handshake>,
    decrypt: BTreeMap<String, Decrypt>,
    fragments: BTreeMap<String, Fragments>,
}

#[derive(Deserialize)]
struct Negotiation {
    private_key: String,
    commands: Vec<String>,
}

#[derive(Deserialize)]
struct Handshake {
    replies: Vec<String>,
    secret: String,
}

#[derive(Deserialize)]
struct Decrypt {
    ciphertext: String,
    secret: String,
    plaintext: String,
}

#[derive(Deserialize)]
struct Fragments {
    packets: Vec<String>,
    secret: String,
    expected: Option<BTreeMap<String, String>>,
}

const SOLIXBLE_CLIENT_ID: &str = "b2dc0b17-b75d-4abf-ba6e-ec7c997c23e7";

fn vectors() -> Vectors {
    toml::from_str(include_str!("vectors/solixble.toml")).expect("vectors parse")
}

fn h(s: &str) -> Vec<u8> {
    hex::decode(s).expect("hex")
}

fn secret(s: &str) -> [u8; 32] {
    h(s).try_into().expect("32 bytes")
}

fn config(v: &Vectors) -> ZxConfig {
    ZxConfig {
        client_id: SOLIXBLE_CLIENT_ID.into(),
        keys: KeyPair::from_bytes(&secret(&v.negotiation.private_key)).expect("key"),
        timestamps: TimestampMode::SessionBase(SOLIX_BASE_TIMESTAMP),
        max_write: 180,
    }
}

fn writes(out: &[Output]) -> Vec<String> {
    out.iter()
        .filter_map(|o| match o {
            Output::Write(b) => Some(hex::encode(b)),
            _ => None,
        })
        .collect()
}

#[test]
fn handshake_reproduces_solixble_frames_and_secrets() {
    let v = vectors();
    for (name, case) in &v.handshake {
        if name.starts_with("prime") {
            continue;
        }
        let t0 = Instant::now();
        let now = Now {
            instant: t0,
            unix: 0,
        };
        let mut session = ZxSession::new(config(&v));
        let mut out = Vec::new();
        session.handle(now, Input::Connected, &mut out);
        for reply in &case.replies {
            session.handle(now, Input::Notify(&h(reply)), &mut out);
        }
        assert_eq!(writes(&out), v.negotiation.commands, "{name}: frames");
        assert_eq!(session.phase(), Phase::Auth, "{name}");

        let device_key = Frame::decode(&h(case.replies.last().expect("reply"))).expect("frame");
        let payload = tlv::parse(&device_key.body, true, false);
        let shared = config(&v)
            .keys
            .shared_secret(&payload.get(0xa1).expect("a1").data)
            .expect("ecdh");
        assert_eq!(hex::encode(shared), case.secret, "{name}: secret");

        let mut out = Vec::new();
        let later = Now {
            instant: t0 + Duration::from_secs(2),
            unix: 0,
        };
        session.handle(later, Input::Tick, &mut out);
        let ready = out.iter().find_map(|o| match o {
            Output::Ready(info) => Some(info.clone()),
            _ => None,
        });
        let info = ready.expect("ready after auth wait");
        assert_eq!(info.module, "ESP32", "{name}");
        assert_eq!(info.mtu, 253, "{name}");
        assert!(info.serial.is_some(), "{name}");
    }
}

#[test]
fn decrypts_solixble_payloads() {
    for (name, case) in vectors().decrypt {
        let cipher = Cipher::cbc_from_secret(&secret(&case.secret));
        let plain = cipher.open(&h(&case.ciphertext)).expect("decrypt");
        assert_eq!(hex::encode(plain), case.plaintext, "{name}");
    }
}

#[test]
fn reassembles_and_decodes_fragmented_telemetry() {
    let v = vectors();
    for (name, case) in &v.fragments {
        let t0 = Instant::now();
        let mut session = ZxSession::with_secret(
            config(&v),
            &secret(&case.secret),
            SessionInfo::default(),
            t0,
        );
        let mut out = Vec::new();
        for packet in &case.packets {
            let now = Now {
                instant: t0,
                unix: 0,
            };
            session.handle(now, Input::Notify(&h(packet)), &mut out);
        }
        let messages: Vec<_> = out
            .iter()
            .filter_map(|o| match o {
                Output::Message(m) if m.id() == MsgId(0x0402) => Some(m.clone()),
                _ => None,
            })
            .collect();
        match &case.expected {
            None => assert!(messages.is_empty(), "{name}"),
            Some(expected) => {
                let last = messages.last().expect("telemetry");
                for (key, value) in expected {
                    let key = u8::from_str_radix(key, 16).expect("key");
                    let item = last.payload.get(key).expect("key present");
                    let raw = if key == 0xa1 {
                        item.data.clone()
                    } else {
                        item.raw()
                    };
                    assert_eq!(&hex::encode(raw), value, "{name}: {key:02x}");
                }
            }
        }
    }
}

#[test]
fn builds_status_request_frame() {
    let v = vectors();
    let t0 = Instant::now();
    let secret = secret(&v.handshake["c300_1"].secret);
    let mut session = ZxSession::with_secret(config(&v), &secret, SessionInfo::default(), t0);
    let mut out = Vec::new();
    let now = Now {
        instant: t0 + Duration::from_secs(5),
        unix: 0,
    };
    let request = Request {
        id: MsgId(0x0040),
        params: Vec::new(),
    };
    session.handle(now, Input::Send(request), &mut out);
    assert_eq!(
        writes(&out),
        vec!["ff091a0003000f4040b74b6bc833ba96e5af1fdd5a2898589309".to_string()]
    );
}
