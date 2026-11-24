use xilo_proto::tlv::{self, Value as TlvValue};
use xilo_proto::{Cmd, Message, MsgId};

use crate::{CommandError, CommandRequest, Confirmed, Evidence, FieldMap, Registry, Value};

const C300_TELEMETRY: &str = "a10131a2050300000000a3050300000000a40302ffffa503020000a603025b00a703020000a803020000a903020000aa03020000ab03020000ac03020000ad03020000ae03025b00af03020000b003020100b103021b04b20302fc01b30302fc01b403021c00b503027b00b603021b04b7020101b8020100b9020125ba020100bb020164bc020164bd020100be020100bf020100c0020101c1020100c2020100c3020100c4020100c51100415a5653424a30453339323030303438c603024a01c70302a005c803022c01c903023c00ca03020000cb020101cc020100cd020102ce020132cf020100d0020100d1020100d2020100f7050301000000f815040101010100010000000000000000000000000000f9020102";

const C1000_TELEMETRY: &str = "a10131a2050300000000a3050300000000a40302d104a503020000a603020000a703020000a803020000a903020000aa03020000ab03020000ac03020000ad03020000ae03020000af03020000b003020300b103020000b203020100b30302a600b403020000b50302ff01b60302ff01b703020000b803029a00b903020000ba0302a600bb03020000bc020100bd020123be020100bf020101c0020100c1020164c2020100c3020164c4020100c5020100c6020100c7020100c8020100c9020100ca020100cb020100cc020100cd020100ce020100cf020100d0110041504339464530453237333030323735e5020100f7050301000000f815040202010100010000000000000000000000000000f9020102fd0b0041313736315f33304168";

fn message(cmd: u16, plain: &str) -> Message {
    let cmd = Cmd(cmd);
    Message {
        cmd,
        payload: tlv::parse(&hex::decode(plain).expect("hex"), cmd.is_response(), true),
    }
}

fn decoded(code: &str, cmd: u16, plain: &str) -> FieldMap {
    let spec = Registry::builtin().get(code).expect("model");
    let mut map = FieldMap::new();
    spec.decode(&message(cmd, plain), &mut map);
    map
}

#[test]
fn builtin_models_load() {
    let r = Registry::builtin();
    for code in ["A1722", "A1723", "A1761"] {
        assert!(r.get(code).is_some(), "{code}");
    }
}

#[test]
fn decodes_c300_telemetry() {
    let m = decoded("A1723", 0xc840, C300_TELEMETRY);
    assert_eq!(m["battery_soc"], Value::Int(100));
    assert_eq!(m["serial_number"], Value::Text("AZVSBJ0E39200048".into()));
    assert_eq!(m["firmware"], Value::Text("1.0.5.1".into()));
    assert_eq!(m["temperature"], Value::Int(37));
    assert_eq!(m["ac_output_power"], Value::Int(91));
    assert_eq!(m["device_timeout"], Value::Int(1440));
    assert_eq!(m["screen_timeout"], Value::Int(300));
    assert_eq!(m["ac_charge_power"], Value::Int(330));
    assert_eq!(m["ac_switch"], Value::Bool(true));
    assert_eq!(m["car_saving_mode"], Value::Int(1));
    assert_eq!(m["port_memory"], Value::Bool(true));
    assert_eq!(m["port_memory_visible"], Value::Int(1));
    assert!(!m.contains_key("remaining_time"), "0xffff means n/a");
}

#[test]
fn decodes_c1000_telemetry() {
    let m = decoded("A1761", 0xc840, C1000_TELEMETRY);
    assert_eq!(m["battery_soc"], Value::Int(100));
    assert_eq!(m["serial_number"], Value::Text("APC9FE0E27300275".into()));
    assert_eq!(m["firmware"], Value::Text("1.6.6".into()));
    assert_eq!(m["temperature"], Value::Int(35));
    assert_eq!(m["car_saving_mode"], Value::Int(2));
    assert!(matches!(m["remaining_time"], Value::Float(v) if (v - 123.3).abs() < 0.01));
}

#[test]
fn switch_push_uses_its_own_keys() {
    let mut m = decoded("A1723", 0xc840, C300_TELEMETRY);
    let spec = Registry::builtin().get("A1723").expect("model");
    let changed = spec.decode(
        &message(0x4401, "a10131a2020100a3020101a4020103a5020101"),
        &mut m,
    );
    assert_eq!(m["ac_switch"], Value::Bool(false));
    assert_eq!(m["dc_switch"], Value::Bool(true));
    assert_eq!(m["light_mode"], Value::Int(3));
    assert_eq!(
        m["ac_timer_remaining"],
        Value::Int(0),
        "a2 in 0401 is not the timer"
    );
    assert!(changed.contains(&"ac_switch".to_string()));
}

#[test]
fn builds_and_validates_commands() {
    let spec = Registry::builtin().get("A1723").expect("model");
    let state = decoded("A1723", 0xc840, C300_TELEMETRY);

    let req = spec
        .build(&CommandRequest::new("dc_switch", Some(1)), &state)
        .expect("dc on");
    assert_eq!(req.id, MsgId(0x004b));
    assert_eq!(req.params, vec![(0xa2, TlvValue::U8(1))]);

    let err = spec.build(&CommandRequest::new("ac_charge_power", Some(250)), &state);
    assert!(matches!(
        err,
        Err(CommandError::InvalidValue { value: 250, .. })
    ));

    let err = spec.build(&CommandRequest::new("factory_reset", None), &state);
    assert_eq!(err, Err(CommandError::NeedsConfirmation));
    let req = spec
        .build(
            &CommandRequest::new("factory_reset", None).confirmed(Confirmed::by_user()),
            &state,
        )
        .expect("reset");
    assert_eq!((req.id, req.params.len()), (MsgId(0x0051), 0));

    let err = spec.build(
        &CommandRequest::new("ac_frequency", Some(60)).confirmed(Confirmed::by_user()),
        &state,
    );
    assert_eq!(
        err,
        Err(CommandError::Gated("Turn off the AC output.".into()))
    );

    let req = spec
        .build(&CommandRequest::new("live_reporting", None), &state)
        .expect("live");
    assert_eq!(
        req.params,
        vec![(0xa2, TlvValue::U8(1)), (0xa3, TlvValue::U32(300))]
    );
}

#[test]
fn inverted_modes_and_optimistic_state() {
    let c1000 = Registry::builtin().get("A1761").expect("model");
    let req = c1000
        .build(
            &CommandRequest::new("car_saving_mode", Some(0)),
            &FieldMap::new(),
        )
        .expect("smart on");
    assert_eq!(req.params, vec![(0xa2, TlvValue::U8(0))]);
    assert_eq!(
        c1000.optimistic(&CommandRequest::new("car_saving_mode", Some(0))),
        Some(("car_saving_mode".into(), Value::Int(2)))
    );

    let c300x = Registry::builtin().get("A1723").expect("model");
    assert_eq!(
        c300x.optimistic(&CommandRequest::new("dc_switch", Some(1))),
        Some(("dc_switch".into(), Value::Bool(true)))
    );
    let state = decoded("A1723", 0xc840, C300_TELEMETRY);
    let cmd = c300x.command("car_saving_mode").expect("cmd");
    assert_eq!(c300x.current(cmd, &state), Some(0));
    assert!(c300x.is_visible(cmd, &state));
    assert!(c300x.is_visible(c300x.command("port_memory").expect("cmd"), &state));
}

#[test]
fn detects_models() {
    let r = Registry::builtin();
    let found = r.detect(&Evidence {
        serial: Some("ALFSBK1G33300077"),
        ..Default::default()
    });
    assert_eq!(found[0].code, "A1723");

    let state = message(0xc840, C1000_TELEMETRY);
    let found = r.detect(&Evidence {
        state: Some(&state),
        ..Default::default()
    });
    assert_eq!(found[0].code, "A1761", "{found:?}");

    let state = message(0xc840, C300_TELEMETRY);
    let found = r.detect(&Evidence {
        state: Some(&state),
        ..Default::default()
    });
    assert_eq!(
        found[0].code, "A1722",
        "the serial in the state picks the C300"
    );
}

#[test]
fn guesses_model_from_advert() {
    let r = Registry::builtin();
    let c300x = hex::decode("01007f1dabc8da01b10b53424b04").expect("hex");
    let c1000 = hex::decode("017ce91392a04a00b10539465104").expect("hex");
    let codes = |d: &[u8]| -> Vec<String> {
        r.guess_from_advert(d)
            .iter()
            .map(|m| m.code.clone())
            .collect()
    };
    assert!(codes(&c300x).contains(&"A1723".to_string()));
    assert_eq!(codes(&c1000), vec!["A1761".to_string()]);
    assert!(codes(&[1, 2]).is_empty());
}

#[test]
fn decodes_imported_kinds() {
    use crate::schema::{FieldDef, Kind};
    use xilo_proto::tlv::Item;
    let field = |kind: Kind| FieldDef {
        key: "a2".into(),
        id: "x".into(),
        label: "x".into(),
        kind,
        msgs: Vec::new(),
        unit: None,
        scale: None,
        na: None,
        options: Default::default(),
        offset: None,
        len: None,
        bit: None,
        mask: None,
        signed: false,
        confidence: Default::default(),
        hidden: false,
    };
    let item = |ty: u8, data: &[u8]| Item {
        key: 0xa2,
        ty: Some(ty),
        data: data.to_vec(),
    };
    let decode = |f: &FieldDef, i: &Item| crate::spec::decode_for_test(f, i);

    let mut signed = field(Kind::Auto);
    signed.signed = true;
    assert_eq!(
        decode(&signed, &item(0x02, &[0xfe, 0xff])),
        Some(Value::Int(-2))
    );
    assert_eq!(
        decode(&field(Kind::Auto), &item(0x02, &[0x10, 0x01])),
        Some(Value::Int(272))
    );
    assert_eq!(
        decode(&field(Kind::Auto), &item(0x00, b"ABC")),
        Some(Value::Text("ABC".into()))
    );
    assert_eq!(
        decode(&field(Kind::F32), &item(0x05, &1.5f32.to_le_bytes())),
        Some(Value::Float(1.5))
    );

    let mut packed = field(Kind::PrefixedText);
    packed.offset = Some(1);
    assert_eq!(
        decode(&packed, &item(0x06, b"\x00\x03ABCxyz")),
        Some(Value::Text("ABC".into()))
    );

    let mut masked = field(Kind::U8);
    masked.offset = Some(2);
    masked.mask = Some(0b0110);
    assert_eq!(
        decode(&masked, &item(0x04, &[0, 0, 0b0100])),
        Some(Value::Int(2))
    );

    let mut first = field(Kind::VersionDigits);
    first.offset = Some(0);
    first.len = Some(1);
    assert_eq!(
        decode(&first, &item(0x03, &[123, 9, 9, 9])),
        Some(Value::Text("1.2.3".into()))
    );
}
