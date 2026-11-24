use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command as Process;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

const DUMP: &str = include_str!("import_dump.py");

/// Models defined by hand or from SolixBLE, and devices that are not Anker hardware.
const SKIP: &[&str] = &[
    "A1722", "A1723", "A1761", "A2345", "SHEMP3", "A5101", "A5102", "A5103",
];

const NAMES: &[(&str, &str)] = &[
    ("A1725", "SOLIX C200(X)"),
    ("A1726", "SOLIX C300 DC"),
    ("A1727", "SOLIX C200 DC"),
    ("A1728", "SOLIX C300X DC"),
    ("A1729", "SOLIX C200X DC"),
    ("A1753", "SOLIX C800"),
    ("A1754", "SOLIX C800 Plus"),
    ("A1755", "SOLIX C800X"),
    ("A1763", "SOLIX C1000 Gen 2"),
    ("A1765", "SOLIX C1000X Gen 2"),
    ("A1783", "SOLIX C2000 Gen 2"),
    ("A1785", "SOLIX C2000X Gen 2"),
    ("AS220", "SOLIX S2000"),
    ("A1772", "SOLIX F1500"),
    ("A1780", "SOLIX F2000"),
    ("A1780P", "SOLIX F2000 Plus"),
    ("A1781", "SOLIX F2600"),
    ("A1790", "SOLIX F3800"),
    ("A1790P", "SOLIX F3800 Plus"),
    ("A1782", "SOLIX F3000"),
    ("A17C0", "Solarbank E1600"),
    ("A17C1", "Solarbank 2 Pro"),
    ("A17C2", "Solarbank 2 AC"),
    ("A17C3", "Solarbank 2 Plus"),
    ("A17C5", "Solarbank 3 Pro"),
    ("AE103", "Solarbank 4 Pro"),
    ("A17E1", "SOLIX E10"),
    ("AX170", "SOLIX E10 Power Dock"),
    ("A17X7", "Smart Meter"),
    ("AE1X0", "Smart Meter Gen 2"),
    ("A17X7US", "Smart Meter (US)"),
    ("AE1R0", "Smart Meter P1"),
    ("A17X8", "Smart Plug"),
    ("AE100", "Power Dock"),
    ("A17A4", "EverFrost 2 40L"),
    ("A17A5", "EverFrost 2 58L"),
    ("A2345", "Prime Charger 250W"),
    ("A91B2", "Prime Charging Station 240W"),
    ("AS200", "Alternator Charger"),
    ("A17B1", "Home Power Panel"),
    ("A5191", "EV Charger"),
    ("A7320", "SOLIX Smart Generator 5500"),
];

/// anker-solix-api field names that match fields the app already knows.
const FIELD_IDS: &[(&str, &str)] = &[
    ("main_battery_soc", "battery_soc"),
    ("battery_soc", "battery_soc"),
    ("temperature", "temperature"),
    ("ac_input_power", "ac_input_power"),
    ("ac_output_power", "ac_output_power"),
    ("output_power_total", "total_output_power"),
    ("photovoltaic_power", "solar_input_power"),
    ("dc_12v_1_power", "car_output_power"),
    ("ac_output_power_switch", "ac_switch"),
    ("dc_output_power_switch", "dc_switch"),
    ("display_mode", "screen_brightness"),
    ("display_switch", "screen_on"),
    ("light_mode", "light_mode"),
    ("temp_unit_fahrenheit", "temperature_unit"),
    ("device_sn", "serial_number"),
    ("sw_version", "firmware"),
    ("hw_version", "hardware_version"),
    ("charging_status", "battery_state"),
    ("battery_soh", "battery_health"),
    ("remaining_time_hours", "remaining_time"),
    ("error_code", "error_code"),
    ("expansion_packs", "expansion_count"),
    ("ac_frequency", "ac_frequency"),
    ("ac_input_limit", "ac_charge_power"),
    ("device_timeout_minutes", "device_timeout"),
    ("display_timeout_seconds", "screen_timeout"),
];

const COMMAND_IDS: &[(&str, &str)] = &[
    ("ac_output_switch", "ac_switch"),
    ("dc_output_switch", "dc_switch"),
    ("light_mode", "light_mode"),
    ("display_switch", "screen_switch"),
    ("display_mode", "screen_brightness"),
    ("temp_unit_switch", "temperature_unit"),
    ("ac_charge_limit", "ac_charge_power"),
    ("device_timeout_minutes", "device_timeout"),
    ("display_timeout_seconds", "screen_timeout"),
    ("realtime_trigger", "live_reporting"),
    ("status_request", "status"),
    ("ac_fast_charge_switch", "ultrafast"),
];

/// Serial prefixes confirmed by captures in the reference projects.
const SERIALS: &[(&str, &str)] = &[("A1763", "APCDK")];

const CONTROLS: &[&str] = &[
    "ac_switch",
    "dc_switch",
    "light_mode",
    "screen_switch",
    "ultrafast",
];
const BLOCKED: &[&str] = &[
    "reset", "ota", "upgrade", "reboot", "restore", "factory", "delete", "unbind", "password",
    "wifi", "clear",
];

fn lookup(table: &[(&str, &str)], key: &str) -> Option<String> {
    table
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| (*v).to_string())
}

fn sanitize(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

fn field_id(name: &str) -> String {
    let clean = sanitize(name);
    if let Some(id) = lookup(FIELD_IDS, &clean) {
        return id;
    }
    for (prefix, ours) in [("usbc_", "usb_c"), ("usba_", "usb_a")] {
        if let Some(rest) = clean.strip_prefix(prefix)
            && let Some((n, what)) = rest.split_once('_')
        {
            let what = if what == "status" { "state" } else { what };
            return format!("{ours}{n}_{what}");
        }
    }
    clean
}

fn humanize(name: &str) -> String {
    let words: Vec<String> = sanitize(name)
        .split('_')
        .filter(|w| !w.is_empty() && *w != "set")
        .map(|w| match w {
            "ac" | "dc" | "pv" | "usb" | "sn" | "pn" | "soc" | "soh" | "sw" | "hw" | "id"
            | "ev" | "ota" => w.to_uppercase(),
            "usbc" => "USB-C".into(),
            "usba" => "USB-A".into(),
            "temp" => "temperature".into(),
            _ => w.to_string(),
        })
        .collect();
    let text = words.join(" ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => text,
    }
}

fn unit_for(name: &str) -> Option<&'static str> {
    let n = sanitize(name);
    if n.ends_with("_power")
        || n.contains("_power_")
        || n.ends_with("_limit")
        || n.ends_with("_load")
    {
        Some("W")
    } else if n.contains("soc") || n.contains("soh") {
        Some("%")
    } else if n.contains("temperature") {
        Some("°C")
    } else if n.ends_with("_seconds") || n.ends_with("_sec") {
        Some("s")
    } else if n.ends_with("_minutes") || n.ends_with("_min") {
        Some("min")
    } else if n.ends_with("_hours") {
        Some("h")
    } else if n.contains("voltage") {
        Some("V")
    } else {
        None
    }
}

fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn type_kind(ty: Option<&str>, signed: bool, has_length: bool) -> Option<&'static str> {
    Some(match (ty?, signed) {
        ("00", _) if has_length => "text",
        ("00", _) => "prefixed_text",
        ("01", false) => "u8",
        ("01", true) => "i8",
        ("02", false) => "u16",
        ("02", true) => "i16",
        ("03", false) => "u32",
        ("03", true) => "i32",
        ("04", _) => "u8",
        ("05", _) => "f32",
        _ => return None,
    })
}

#[derive(Default)]
struct Out {
    fields: String,
    commands: String,
    state_msgs: Vec<String>,
    field_ids: HashSet<String>,
    command_ids: HashSet<String>,
    status: bool,
    keepalive: bool,
    gen2: bool,
}

impl Out {
    #[allow(clippy::too_many_arguments)]
    fn field(
        &mut self,
        key: &str,
        msg: &str,
        desc: &Map<String, Value>,
        kind: &str,
        offset: Option<u64>,
        len: Option<u64>,
        mask: Option<u64>,
    ) {
        let Some(name) = desc.get("name").and_then(Value::as_str) else {
            return;
        };
        if desc.contains_key("reversed") || name == "msg_timestamp" || name.starts_with("pattern") {
            return;
        }
        let id = field_id(name);
        let mut kind = kind.to_string();
        let mut len = len;
        let mut offset = offset;
        let version = id == "firmware" || name.contains("version") || name.starts_with("sw_");
        if version && kind != "text" && kind != "prefixed_text" {
            kind = "version_digits".into();
        }
        if let Some(count) = desc.get("values").and_then(Value::as_u64) {
            offset = offset.or(Some(0));
            match count {
                1 => {
                    len = Some(1);
                    if !version {
                        kind = if desc.get("signed").and_then(Value::as_bool) == Some(true) {
                            "i8"
                        } else {
                            "u8"
                        }
                        .into();
                    }
                }
                2 => {
                    len = Some(2);
                    if !version {
                        kind = if desc.get("signed").and_then(Value::as_bool) == Some(false) {
                            "u16"
                        } else {
                            "i16"
                        }
                        .into();
                    }
                }
                _ => kind = "hex".into(),
            }
        }
        let _ = writeln!(self.fields, "[[field]]");
        let _ = writeln!(self.fields, "key = {}", q(key));
        let _ = writeln!(self.fields, "id = {}", q(&id));
        let _ = writeln!(self.fields, "label = {}", q(&humanize(name)));
        let _ = writeln!(self.fields, "kind = {}", q(&kind));
        let _ = writeln!(self.fields, "msgs = [{}]", q(msg));
        if kind == "auto" && desc.get("signed").and_then(Value::as_bool) == Some(true) {
            let _ = writeln!(self.fields, "signed = true");
        }
        if let Some(factor) = desc.get("factor").and_then(Value::as_f64)
            && factor != 1.0
            && kind != "version_digits"
        {
            let _ = writeln!(self.fields, "scale = {factor}");
        }
        if let Some(unit) = unit_for(name) {
            let _ = writeln!(self.fields, "unit = {}", q(unit));
        }
        if let Some(o) = offset {
            let _ = writeln!(self.fields, "offset = {o}");
        }
        if let Some(l) = len {
            let _ = writeln!(self.fields, "len = {l}");
        }
        if let Some(m) = mask {
            let _ = writeln!(self.fields, "mask = {m}");
        }
        let unsure = name.contains('?');
        let _ = writeln!(
            self.fields,
            "confidence = {}",
            q(if unsure { "unknown" } else { "likely" })
        );
        if name.contains("timestamp") || unsure || id.starts_with("device_pn") {
            let _ = writeln!(self.fields, "hidden = true");
        }
        let _ = writeln!(self.fields);
        self.field_ids.insert(id);
    }

    fn status_msg(&mut self, msg: &str, desc: &Map<String, Value>) {
        self.state_msgs.push(msg.to_string());
        if msg == "0421" || msg == "0900" {
            self.gen2 = true;
        }
        for (key, value) in desc {
            if key.len() != 2 || u8::from_str_radix(key, 16).is_err() {
                continue;
            }
            let Some(d) = value.as_object() else { continue };
            match d.get("bytes") {
                Some(Value::Object(bytes)) => {
                    for (pos, entry) in bytes {
                        let Ok(offset) = pos.parse::<u64>() else {
                            continue;
                        };
                        let entries: Vec<&Map<String, Value>> = match entry {
                            Value::Object(m) => vec![m],
                            Value::Array(list) => {
                                list.iter().filter_map(Value::as_object).collect()
                            }
                            _ => continue,
                        };
                        for e in entries {
                            let signed = e.get("signed").and_then(Value::as_bool).unwrap_or(false);
                            let length = e.get("length").and_then(Value::as_u64);
                            let ty = e.get("type").and_then(Value::as_str);
                            let Some(mut kind) = type_kind(
                                ty,
                                signed || ty == Some("02") && e.get("signed").is_none(),
                                length.is_some_and(|l| l > 0),
                            ) else {
                                continue;
                            };
                            let mut len = length.filter(|l| *l > 0);
                            if ty == Some("00") && length == Some(0) {
                                kind = "text";
                                len = None;
                            }
                            let mask = e.get("mask").and_then(Value::as_u64);
                            self.field(key, msg, e, kind, Some(offset), len, mask);
                        }
                    }
                }
                Some(_) => {}
                None => self.field(key, msg, d, "auto", None, None, None),
            }
        }
    }

    fn command(
        &mut self,
        msg: &str,
        name: &str,
        desc: &Map<String, Value>,
        field_names: &BTreeMap<String, String>,
    ) {
        if BLOCKED.iter().any(|b| name.contains(b)) {
            return;
        }
        let id = lookup(COMMAND_IDS, name).unwrap_or_else(|| sanitize(name));
        if self.command_ids.contains(&id) {
            return;
        }
        let mut params: Vec<(&String, &Map<String, Value>)> = desc
            .iter()
            .filter(|(k, _)| k.len() == 2 && *k != "a1" && *k != "fe" && *k != "fd")
            .filter_map(|(k, v)| Some((k, v.as_object()?)))
            .collect();
        params.sort_by(|a, b| a.0.cmp(b.0));
        let variable: Vec<_> = params
            .iter()
            .filter(|(_, p)| !p.contains_key("value_default"))
            .collect();
        if variable.len() > 1 {
            return;
        }
        let mut fixed = Vec::new();
        for (key, p) in &params {
            if !p.contains_key("value_default") || variable.iter().any(|(k, _)| k == key) {
                continue;
            }
            let Some(kind) = param_kind(p) else { return };
            let Some(value) = p.get("value_default").and_then(Value::as_i64) else {
                return;
            };
            fixed.push(format!(
                "{{ key = {}, kind = {}, value = {value} }}",
                q(key),
                q(kind)
            ));
        }
        let mut body = String::new();
        let _ = writeln!(body, "[[command]]");
        let _ = writeln!(body, "id = {}", q(&id));
        let _ = writeln!(body, "msg = {}", q(msg));
        let _ = writeln!(body, "label = {}", q(&humanize(name)));
        let group = if CONTROLS.contains(&id.as_str()) {
            "control"
        } else if id == "temperature_unit" {
            "general"
        } else if id == "status" || id == "live_reporting" || variable.is_empty() {
            "hidden"
        } else {
            "function"
        };
        let _ = writeln!(body, "group = {}", q(group));
        match variable.first() {
            None => {
                let _ = writeln!(body, "input = \"none\"");
            }
            Some((key, p)) => {
                let Some(kind) = param_kind(p) else { return };
                let unit = p.get("name").and_then(Value::as_str).and_then(unit_for);
                let _ = writeln!(body, "param = {{ key = {}, kind = {} }}", q(key), q(kind));
                match p.get("value_options") {
                    Some(Value::Object(options)) => {
                        let on = options
                            .iter()
                            .find(|(k, _)| matches!(k.as_str(), "on" | "enabled" | "enable"));
                        let off = options
                            .iter()
                            .find(|(k, _)| matches!(k.as_str(), "off" | "disabled" | "disable"));
                        if let (Some(on), Some(off), 2) = (on, off, options.len()) {
                            let _ = writeln!(body, "input = \"switch\"");
                            let _ = writeln!(body, "on_value = {}", on.1.as_i64().unwrap_or(1));
                            let _ = writeln!(body, "off_value = {}", off.1.as_i64().unwrap_or(0));
                            let _ = writeln!(body, "on_state = {}", on.1.as_i64().unwrap_or(1));
                        } else {
                            let _ = writeln!(body, "input = \"choice\"");
                            let list: Vec<String> = options
                                .iter()
                                .filter_map(|(label, v)| {
                                    Some(format!(
                                        "{{ value = {}, label = {} }}",
                                        v.as_i64()?,
                                        q(&humanize(label))
                                    ))
                                })
                                .collect();
                            let _ = writeln!(body, "options = [{}]", list.join(", "));
                        }
                    }
                    Some(Value::Array(values)) => {
                        let _ = writeln!(body, "input = \"choice\"");
                        let list: Vec<String> = values
                            .iter()
                            .filter_map(Value::as_i64)
                            .map(|v| {
                                format!(
                                    "{{ value = {v}, label = {} }}",
                                    q(&format!(
                                        "{v}{}",
                                        unit.map(|u| format!(" {u}")).unwrap_or_default()
                                    ))
                                )
                            })
                            .collect();
                        let _ = writeln!(body, "options = [{}]", list.join(", "));
                    }
                    _ => {
                        let (Some(min), Some(max)) = (
                            p.get("value_min").and_then(Value::as_i64),
                            p.get("value_max").and_then(Value::as_i64),
                        ) else {
                            return;
                        };
                        let _ = writeln!(body, "input = \"number\"");
                        let _ = writeln!(body, "min = {min}");
                        let _ = writeln!(body, "max = {max}");
                        if let Some(step) = p.get("value_step").and_then(Value::as_i64) {
                            let _ = writeln!(body, "step = {step}");
                        }
                        if let Some(unit) = unit {
                            let _ = writeln!(body, "unit = {}", q(unit));
                        }
                    }
                }
                if !p.contains_key("state_converter")
                    && let Some(state) = p.get("state_name").and_then(Value::as_str)
                    && let Some(field) = field_names.get(state)
                    && self.field_ids.contains(field)
                {
                    let _ = writeln!(body, "state = {}", q(field));
                }
                if id == "ac_frequency" {
                    let _ = writeln!(body, "danger = \"confirm\"");
                    let _ = writeln!(
                        body,
                        "danger_text = \"Changing the AC frequency affects every appliance on the AC output. Only change it if your appliances need it.\""
                    );
                }
            }
        }
        if !fixed.is_empty() {
            let _ = writeln!(body, "fixed = [{}]", fixed.join(", "));
        }
        let _ = writeln!(body);
        if id == "status" {
            self.status = true;
        }
        if id == "live_reporting" {
            self.keepalive = true;
        }
        self.command_ids.insert(id);
        self.commands.push_str(&body);
    }
}

fn param_kind(p: &Map<String, Value>) -> Option<&'static str> {
    match p.get("type").and_then(Value::as_str)? {
        "01" => Some("u8"),
        "02" => Some("u16"),
        "03" => Some("u32"),
        _ => None,
    }
}

pub fn run(root: &Path, src: &Path) -> Result<()> {
    let tmp = std::env::temp_dir().join(format!("xilo-import-{}", std::process::id()));
    fs::create_dir_all(&tmp)?;
    let script = tmp.join("dump.py");
    let json = tmp.join("dump.json");
    fs::write(&script, DUMP)?;
    let status = Process::new("python3")
        .arg("-I")
        .arg(&script)
        .arg(src)
        .arg(&json)
        .status()
        .context("python3 is needed to read the anker-solix-api maps")?;
    if !status.success() {
        bail!("reading the anker-solix-api maps failed");
    }
    let dump: Value = serde_json::from_str(&fs::read_to_string(&json)?)?;
    let _ = fs::remove_dir_all(&tmp);
    let map = dump["map"].as_object().context("map missing")?;
    let out_dir = root.join("crates/models/src/families/imported");
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir)?;
    let mut written = 0;
    for (code, msgs) in map {
        if SKIP.contains(&code.as_str()) {
            continue;
        }
        let Some(msgs) = msgs.as_object() else {
            continue;
        };
        let mut out = Out::default();
        let mut field_names = BTreeMap::new();
        for (msg, desc) in msgs {
            let Some(d) = desc.as_object() else { continue };
            if d.contains_key("command_name")
                || d.contains_key("command_list")
                || d.get("topic").and_then(Value::as_str) == Some("req")
            {
                continue;
            }
            if u16::from_str_radix(msg, 16).is_err() {
                continue;
            }
            out.status_msg(msg, d);
        }
        collect_names(msgs, &mut field_names);
        for (msg, desc) in msgs {
            let Some(d) = desc.as_object() else { continue };
            if let Some(name) = d.get("command_name").and_then(Value::as_str) {
                out.command(msg, name, d, &field_names);
            } else if let Some(list) = d.get("command_list").and_then(Value::as_array) {
                for name in list.iter().filter_map(Value::as_str) {
                    if let Some(entry) = d.get(name).and_then(Value::as_object) {
                        out.command(msg, name, entry, &field_names);
                    }
                }
            }
        }
        if out.state_msgs.is_empty() {
            continue;
        }
        let lower = code.to_lowercase();
        let family = format!("imp_{lower}");
        let mut text = String::new();
        let _ = writeln!(
            text,
            "# Generated by `cargo xtask import solix-api` from anker-solix-api (MIT). Curate by hand."
        );
        let _ = writeln!(text, "[family]");
        let _ = writeln!(text, "id = {}", q(&family));
        let _ = writeln!(text, "protocol = \"zx\"");
        let _ = writeln!(text, "gatt = \"g1\"");
        let msgs_list: Vec<String> = out.state_msgs.iter().map(|m| q(m)).collect();
        let _ = writeln!(text, "state_msgs = [{}]", msgs_list.join(", "));
        if out.status {
            let _ = writeln!(text, "status_request = \"status\"");
            let _ = writeln!(text, "poll = {{ command = \"status\", every_s = 60 }}");
        }
        if out.keepalive {
            let _ = writeln!(
                text,
                "keepalive = {{ command = \"live_reporting\", every_s = 60 }}"
            );
        }
        let _ = writeln!(text);
        text.push_str(&out.fields);
        text.push_str(&out.commands);
        let dir = out_dir.join(&lower);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("family.toml"), text)?;
        let name = lookup(NAMES, code)
            .or_else(|| dump["names"][code.as_str()].as_str().map(str::to_string))
            .unwrap_or_else(|| code.clone());
        let prefixes = lookup(SERIALS, code)
            .map(|p| format!("serial_prefixes = [{}]\n", q(&p)))
            .unwrap_or_default();
        let model = format!(
            "[model]\ncode = {}\nname = {}\nfamily = {}\nstatus = \"unverified\"\n{prefixes}assets = {}\n",
            q(code),
            q(&name),
            q(&family),
            q(&lower)
        );
        fs::write(dir.join(format!("{lower}.toml")), model)?;
        written += 1;
        let _ = out.gen2;
    }
    println!("imported {written} models into {}", out_dir.display());
    Ok(())
}

fn collect_names(msgs: &Map<String, Value>, names: &mut BTreeMap<String, String>) {
    fn walk(v: &Value, names: &mut BTreeMap<String, String>) {
        match v {
            Value::Object(m) => {
                if let Some(n) = m.get("name").and_then(Value::as_str) {
                    names.entry(n.to_string()).or_insert_with(|| field_id(n));
                }
                for x in m.values() {
                    walk(x, names);
                }
            }
            Value::Array(list) => list.iter().for_each(|x| walk(x, names)),
            _ => {}
        }
    }
    for (_, desc) in msgs {
        if let Some(d) = desc.as_object()
            && !d.contains_key("command_name")
            && !d.contains_key("command_list")
        {
            walk(desc, names);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn maps_names() {
        assert_eq!(super::field_id("usbc_2_status"), "usb_c2_state");
        assert_eq!(super::field_id("main_battery_soc"), "battery_soc");
        assert_eq!(super::field_id("exp_1_soc?"), "exp_1_soc");
        assert_eq!(super::humanize("set_ac_output_switch"), "AC output switch");
    }
}
