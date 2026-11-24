//! Decodes SolixBLE's telemetry fixtures with the imported model maps.

use xilo_models::{FieldMap, Registry, Value};
use xilo_proto::{Cmd, Message, tlv};

/// SolixBLE attribute names and the field ids they match here.
const SAME: &[(&str, &str)] = &[
    ("serial_number", "serial_number"),
    ("battery_percentage", "battery_soc"),
    ("battery_health", "battery_health"),
    ("temperature", "temperature"),
    ("ac_power_in", "ac_input_power"),
    ("ac_power_out", "ac_output_power"),
    ("usb_c1_power", "usb_c1_power"),
    ("usb_c2_power", "usb_c2_power"),
    ("usb_a1_power", "usb_a1_power"),
    ("error_code", "error_code"),
    ("usb_c1_voltage", "usb_c1_voltage"),
    ("usb_c1_current", "usb_c1_current"),
    ("usb_c2_power", "usb_c2_power"),
    ("usb_c3_power", "usb_c3_power"),
    ("power_out", "total_output_power"),
    ("pad_1_power", "pad_1_power"),
    ("pad_2_power", "pad_2_power"),
    ("pad_3_power", "pad_3_power"),
];

fn matches(ours: &Value, theirs: &serde_json::Value) -> bool {
    match (ours, theirs) {
        (Value::Text(a), serde_json::Value::String(b)) => a == b,
        (v, serde_json::Value::Number(n)) => match (v.as_f64(), n.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() < 0.051,
            _ => false,
        },
        _ => false,
    }
}

#[test]
fn imported_maps_agree_with_solixble() {
    let text = include_str!("vectors/solixble_values.json");
    let cases: Vec<serde_json::Value> = serde_json::from_str(text).expect("fixture json");
    let registry = Registry::builtin();
    let mut failures = Vec::new();
    for case in &cases {
        let code = case["model"].as_str().expect("model");
        let spec = registry.get(code).expect("model exists");
        let msg = u16::from_str_radix(case["msg"].as_str().expect("msg"), 16).expect("hex");
        let payload = hex::decode(case["payload"].as_str().expect("payload")).expect("hex");
        let message = Message {
            cmd: Cmd(0xc000 | msg),
            payload: tlv::parse(&payload, false, true),
        };
        let mut map = FieldMap::new();
        spec.decode(&message, &mut map);
        let mut compared = 0;
        for (theirs, ours) in SAME {
            let (Some(expected), Some(got)) = (case["expected"].get(*theirs), map.get(*ours))
            else {
                continue;
            };
            compared += 1;
            if !matches(got, expected) {
                failures.push(format!("{code} {ours}: got {got:?}, SolixBLE {expected}"));
            }
        }
        if compared < 2 {
            failures.push(format!(
                "{code} ({}): only {compared} shared fields decoded",
                case["source"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
