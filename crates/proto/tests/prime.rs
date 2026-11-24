//! Reproduces the Prime handshake recorded by SolixBLE (MIT, Harvey Lelliott)
//! in `SolixBLE/prime_device.py` and `tests/const.py`.

use std::time::{Duration, Instant};

use xilo_proto::crypto::{Cipher, KeyPair};
use xilo_proto::frame::Frame;
use xilo_proto::tlv;
use xilo_proto::{Flavor, Input, MsgId, Now, Output, Request, TimestampMode, ZxConfig, ZxSession};

const PRIVATE_KEY: &str = "754744d72984c378bc4fa77d7fcdf6bbb6d9df119fa9be4948eb8a3b4cd6071f";
const CLIENT_ID: &str = "79ebed35-dc9c-4904-b40c-72c4e863aa10";
const BASE: u32 = 0x69b5_79ef;
const SECRET: &str = "09486817d949a232b58b47a43cc72d045a617a26f3999d30e1d27e38eae52265";

/// What the client sends, then what the device answers.
const TRANSCRIPT: &[(&str, &str)] = &[
    (
        "ff09200003000140010a82d0ab535303e3aa9f0c2f9c868465bc8476f556fb7d",
        "ff091e000300014801ab273ed3e27270c3f4d676ac7d69a00572793732a6",
    ),
    (
        "ff09270003000140030a82d0ab53538ab3de100ac9bb87a0b8e36c1dd8167a9c25a9839d9a14d5",
        "ff092b000300014803ab273ed0443800b35db54c6d4a6ec3d48171a04ea7ebce8bf749e5e48c5d991a5e67",
    ),
    (
        "ff09200003000140290a82d0ab535303e3aa9f0c2f9c868465bc8476f556fb55",
        "ff0958000300014829ab273ed144326ada9fc66fa02508c5ddf549ade014d1eeb252fea1057c15b00985ab8a724fa3830e8e5b27acbaa1224fd2172c0439d27aaf9e62a66bda5c41c424f23c5c8d7df8d3b89422ddff2266",
    ),
    (
        "ff092d0003000140050a82d0ab53538ab3de100ae04aca6791257881a90164eac7460450e0c82f2c03de4f9604",
        "ff091b000300014805abab709a595a803dd04246b78a927453cf65",
    ),
    (
        "ff095c0003000140210ac6ea31e4300bb2877d6ddeb628b0d7be8d768333f00ceab5454d20fbd97e091457b1f3b6efb6511eb9e98ac2b2c46eee211ae359ad246e1ae9886b4a29e41eddd5a5064d8b9ffdbfb43eb6b8e307fcde9de7",
        "ff095d000300014821ab277fc01de436d341de628c79c1384d0aea25ce030622fa3ca0808ce5d1b7365ec1b1753a11ab78fba3ca07dda95cd57c93d1267b1222bef9908f7633a758ab924eba63ee01e715be5b9c3b082e6d81c2204241",
    ),
    (
        "ff094000030001402257ec69586f3500c8f858e0ba047f237f4e2ed8c50d2f39ba3587e4010275bea22242936f08788849272fb3f4cf7493be4a60bb9c9f0693",
        "ff091b000300014822f60b45600839b2c171b33dc5790ed64ae32d",
    ),
    (
        "ff094600030001402757ec69586f3501e8cf6185d8c4035707377af9af3a2e40b02b86e7531974f1c22440de6e43705566b77cf940e235b65abf4d413ece5f2c3781712f3742",
        "ff091b000300014827f60b45600839b2c171b33dc5790ed64ae328",
    ),
];

const SUBSCRIBE: [&str; 2] = [
    "ff09230003000f420057e9b8dfdeacda7991d3eb7f12093e55ff002aa9799bcc9216e3",
    "ff09530003000f420a57e9b883d958e48e5b7de48d980206577e2dafbb3d604dea3686f3011969f0db2311906d142b5730ee2bfb11e3fbbe7485aac8877995310669156ec74645c962b419e579b385fd079967",
];

fn h(s: &str) -> Vec<u8> {
    hex::decode(s).expect("hex")
}

fn config(flavor: Flavor, probe: bool) -> ZxConfig {
    let key: [u8; 32] = h(PRIVATE_KEY).try_into().expect("32 bytes");
    ZxConfig {
        client_id: CLIENT_ID.into(),
        keys: KeyPair::from_bytes(&key).expect("key"),
        timestamps: TimestampMode::SessionBase(BASE),
        max_write: 240,
        flavor,
        probe,
        timezone: "GMT0BST,M3.5.0/1,M10.5.0".into(),
        country: "GB".into(),
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
fn reproduces_the_solixble_prime_handshake() {
    let t0 = Instant::now();
    let now = Now {
        instant: t0,
        unix: BASE,
    };
    let mut session = ZxSession::new(config(Flavor::Prime, false));
    let mut out = Vec::new();
    session.handle(now, Input::Connected, &mut out);
    for (step, (sent, reply)) in TRANSCRIPT.iter().enumerate() {
        assert_eq!(writes(&out), vec![sent.to_string()], "client frame {step}");
        out.clear();
        session.handle(now, Input::Notify(&h(reply)), &mut out);
    }
    assert_eq!(
        writes(&out),
        SUBSCRIBE.map(str::to_string).to_vec(),
        "subscribe frames"
    );
    assert!(session.is_ready());
    assert!(out.iter().any(|o| matches!(o, Output::Ready(_))));
}

#[test]
fn prime_session_commands_and_telemetry() {
    let t0 = Instant::now();
    let now = Now {
        instant: t0,
        unix: BASE,
    };
    let mut session = ZxSession::new(config(Flavor::Prime, false));
    let mut out = Vec::new();
    session.handle(now, Input::Connected, &mut out);
    for (_, reply) in TRANSCRIPT {
        session.handle(now, Input::Notify(&h(reply)), &mut out);
    }
    out.clear();

    let secret: [u8; 32] = h(SECRET).try_into().expect("32 bytes");
    let cipher = Cipher::gcm_from_secret(&secret);
    session.handle(
        now,
        Input::Send(Request {
            id: MsgId(0x0207),
            params: vec![(0xa2, tlv::Value::U8(1))],
        }),
        &mut out,
    );
    let sent = writes(&out);
    let frame = Frame::decode(&h(&sent[0])).expect("frame");
    let plain = cipher.open(&frame.body).expect("decrypts");
    assert_eq!(hex::encode(plain), "a10121a2020101fe04f079b569");

    let push = Frame::new(
        0x0f,
        xilo_proto::Cmd(0x4300),
        cipher.seal(&h("a10131a2020150")),
    )
    .encode();
    out.clear();
    session.handle(now, Input::Notify(&push), &mut out);
    let message = out
        .iter()
        .find_map(|o| match o {
            Output::Message(m) => Some(m),
            _ => None,
        })
        .expect("telemetry message");
    assert_eq!(message.id(), MsgId(0x0300));
    assert_eq!(
        message.payload.get(0xa2).and_then(tlv::Item::uint),
        Some(0x50)
    );
}

#[test]
fn probe_switches_to_prime_when_zx_is_silent() {
    let t0 = Instant::now();
    let at = |s: u64| Now {
        instant: t0 + Duration::from_secs(s),
        unix: BASE,
    };
    let mut session = ZxSession::new(config(Flavor::Zx, true));
    let mut out = Vec::new();
    session.handle(at(0), Input::Connected, &mut out);
    assert!(
        writes(&out)[0].starts_with("ff0936000300010001"),
        "ZX hello first"
    );
    out.clear();
    session.handle(at(5), Input::Tick, &mut out);
    assert_eq!(session.flavor(), Flavor::Prime);
    assert_eq!(
        writes(&out),
        vec![TRANSCRIPT[0].0.to_string()],
        "Prime hello next"
    );
}
