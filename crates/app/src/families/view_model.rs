use xilo_engine::{LinkState, Snapshot};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Battery {
    Normal,
    LowTemp,
    HighTemp,
    Empty,
}

impl Battery {
    pub fn key(self) -> &'static str {
        match self {
            Battery::Normal => "c300/battery_normal",
            Battery::LowTemp => "c300/battery_low_temp",
            Battery::HighTemp => "c300/battery_high_temp",
            Battery::Empty => "c300/battery_low_battery",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PortIcon {
    UsbC(i64),
    UsbA(bool),
    Solar(bool),
    AcInput(bool),
}

impl PortIcon {
    pub fn asset(self) -> &'static str {
        match self {
            PortIcon::UsbC(1) => "ports/usb_c_output",
            PortIcon::UsbC(2) => "ports/usb_c_input",
            PortIcon::UsbC(_) => "ports/usb_c_off",
            PortIcon::UsbA(true) => "shared/usb_a_on",
            PortIcon::UsbA(false) => "shared/usb_a_off",
            PortIcon::Solar(true) => "ports/dc_input_on",
            PortIcon::Solar(false) => "ports/dc_input_off",
            PortIcon::AcInput(true) => "ports/ac_input_on",
            PortIcon::AcInput(false) => "ports/ac_input_off",
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Port {
    pub label: &'static str,
    pub icon: PortIcon,
    pub watts: Option<i64>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Dashboard {
    pub connected: bool,
    pub loading: bool,
    pub hero: String,
    pub battery: Battery,
    pub level: f32,
    pub soc: Option<i64>,
    pub temperature: Option<String>,
    pub healthy_temp: bool,
    pub charging: bool,
    pub overload: bool,
    pub status: String,
    pub total_in: Option<i64>,
    pub total_out: Option<i64>,
    pub ports: Vec<Port>,
    pub error: Option<(i64, String)>,
    pub gauge: bool,
}

const TEMP_ERRORS: [i64; 4] = [27, 28, 32, 33];

pub fn temperature_text(celsius: i64, fahrenheit: bool) -> String {
    if fahrenheit {
        format!("{:.0}°F", celsius as f64 * 1.8 + 32.0)
    } else {
        format!("{celsius}°C")
    }
}

/// Formats minutes like the Anker app: "1Days 2Hr 30mins".
pub fn duration_text(minutes: i64) -> String {
    let minutes = minutes.max(6);
    let (days, rest) = (minutes / 1440, minutes % 1440);
    let (hours, mins) = (rest / 60, rest % 60);
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}Days"));
    }
    if hours > 0 {
        parts.push(format!("{hours}Hr"));
    }
    if mins > 0 {
        parts.push(format!("{mins}mins"));
    }
    parts.join(" ")
}

pub fn build(snap: &Snapshot, assets: &str, blink_on: bool) -> Dashboard {
    let connected = snap.is_ready();
    let loading = connected && snap.updated.is_none();
    let error = snap.int("error_code").filter(|c| *c != 0);
    let light = snap.int("light_mode").unwrap_or(0);
    let lit = if light == 4 { blink_on } else { light > 0 };
    let hot = error.is_some_and(|e| e == 27 || e == 32);
    let hero = if !connected {
        format!("{assets}/hero_disconnect")
    } else {
        let colour = if hot { "red" } else { "blue" };
        let state = if lit { "on" } else { "off" };
        format!("{assets}/hero_{colour}_{state}")
    };
    let soc = snap.int("battery_soc");
    let overload = snap.flag("overload");
    let charging = snap.int("battery_state") == Some(2);
    let battery = match error {
        Some(27 | 32) => Battery::HighTemp,
        _ if overload => Battery::HighTemp,
        Some(28 | 33) => Battery::LowTemp,
        _ if charging => Battery::Normal,
        _ if soc.is_some_and(|s| s <= 5) => Battery::Empty,
        _ => Battery::Normal,
    };
    let fahrenheit = snap.int("temperature_unit") == Some(1);
    let temperature = snap
        .int("temperature")
        .map(|c| temperature_text(c, fahrenheit));
    let total_in = snap.int("total_input_power");
    let total_out = snap.int("total_output_power");
    let status = status_line(snap, total_in, total_out);
    let watts = |id: &str| snap.int(id).filter(|_| connected);
    let source = snap.int("input_source").unwrap_or(0);
    let ports = vec![
        Port {
            label: "C1",
            icon: PortIcon::UsbC(snap.int("usb_c1_state").unwrap_or(0)),
            watts: watts("usb_c1_power"),
        },
        Port {
            label: "C2",
            icon: PortIcon::UsbC(snap.int("usb_c2_state").unwrap_or(0)),
            watts: watts("usb_c2_power"),
        },
        Port {
            label: "C3",
            icon: PortIcon::UsbC(snap.int("usb_c3_state").unwrap_or(0)),
            watts: watts("usb_c3_power"),
        },
        Port {
            label: "USB-A",
            icon: PortIcon::UsbA(snap.int("usb_a_state").unwrap_or(0) != 0),
            watts: watts("usb_a_power"),
        },
        Port {
            label: "SOLAR IN",
            icon: PortIcon::Solar(source == 1 || source == 3),
            watts: watts("solar_input_power"),
        },
        Port {
            label: "AC INPUT",
            icon: PortIcon::AcInput(source == 2 || source == 3),
            watts: watts("ac_input_power"),
        },
    ];
    let error = error.map(|code| {
        let text = snap
            .model
            .as_ref()
            .and_then(|m| m.error_text(code as u32))
            .map(|e| e.text.clone())
            .unwrap_or_else(|| format!("The device reported error code {code}."));
        (code, text)
    });
    Dashboard {
        connected,
        loading,
        hero,
        battery,
        level: soc.map_or(0.0, |s| (s.clamp(0, 100) as f32) / 100.0),
        soc,
        temperature,
        healthy_temp: !error.as_ref().is_some_and(|(c, _)| TEMP_ERRORS.contains(c)),
        charging,
        overload,
        status,
        total_in,
        total_out,
        ports,
        error,
        gauge: true,
    }
}

/// The C1000 uses its own hero images and port layout.
pub fn build_c1000(snap: &Snapshot, assets: &str, blink_on: bool) -> Dashboard {
    let mut d = build(snap, assets, blink_on);
    d.gauge = false;
    if d.connected {
        let light = snap.int("light_mode").unwrap_or(0);
        let lit = if light == 4 { blink_on } else { light > 0 };
        let hot = d.error.as_ref().is_some_and(|(c, _)| *c == 27 || *c == 32);
        let extra = if snap.int("expansion_count").unwrap_or(0) > 0 {
            "_extrabattery"
        } else {
            ""
        };
        d.hero = format!(
            "{assets}/hero_{}{extra}_{}",
            if hot { "red" } else { "blue" },
            if lit { "lighton" } else { "lightoff" }
        );
    }
    let watts = |id: &str| snap.int(id).filter(|_| d.connected);
    let source = snap.int("input_source").unwrap_or(0);
    d.ports = vec![
        Port {
            label: "C1",
            icon: PortIcon::UsbC(snap.int("usb_c1_state").unwrap_or(0)),
            watts: watts("usb_c1_power"),
        },
        Port {
            label: "C2",
            icon: PortIcon::UsbC(snap.int("usb_c2_state").unwrap_or(0)),
            watts: watts("usb_c2_power"),
        },
        Port {
            label: "USB-A1",
            icon: PortIcon::UsbA(snap.int("usb_a1_state").unwrap_or(0) != 0),
            watts: watts("usb_a1_power"),
        },
        Port {
            label: "USB-A2",
            icon: PortIcon::UsbA(snap.int("usb_a2_state").unwrap_or(0) != 0),
            watts: watts("usb_a2_power"),
        },
        Port {
            label: "SOLAR IN",
            icon: PortIcon::Solar(source == 1 || source == 3),
            watts: watts("solar_input_power"),
        },
        Port {
            label: "AC INPUT",
            icon: PortIcon::AcInput(source == 2 || source == 3),
            watts: watts("ac_input_power"),
        },
    ];
    d
}

fn status_line(snap: &Snapshot, total_in: Option<i64>, total_out: Option<i64>) -> String {
    match snap.link {
        LinkState::Connecting | LinkState::Handshaking => return "Connecting...".into(),
        LinkState::Released => return "Released to the phone app".into(),
        LinkState::Disconnected => return "Disconnected".into(),
        LinkState::Ready => {}
    }
    if snap.updated.is_none() {
        return "Connecting...".into();
    }
    let Some(hours) = snap.float("remaining_time") else {
        return String::new();
    };
    let time = duration_text((hours * 60.0).round() as i64);
    let (input, output) = (total_in.unwrap_or(0), total_out.unwrap_or(0));
    if output > input || (output == 0 && input == 0 && snap.flag("ac_switch")) {
        format!("Remaining Usage Time: {time}")
    } else if output < input {
        format!("Remaining Charging Time: {time}")
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use xilo_engine::{DeviceAddr, LinkState, Snapshot};
    use xilo_models::{Registry, Value};

    use super::*;

    fn snap(values: &[(&str, Value)]) -> Snapshot {
        let mut s = Snapshot::new("k".into(), DeviceAddr::new("a"));
        s.model = Registry::builtin().get("A1723").cloned();
        s.link = LinkState::Ready;
        s.updated = Some(SystemTime::now());
        for (k, v) in values {
            s.fields.insert((*k).to_string(), v.clone());
        }
        s
    }

    #[test]
    fn durations_match_the_app() {
        assert_eq!(duration_text(6000), "4Days 4Hr");
        assert_eq!(duration_text(90), "1Hr 30mins");
        assert_eq!(duration_text(2), "6mins");
    }

    #[test]
    fn hero_follows_light_and_errors() {
        let d = build(&snap(&[("light_mode", Value::Int(2))]), "a1723", false);
        assert_eq!(d.hero, "a1723/hero_blue_on");
        let d = build(&snap(&[("error_code", Value::Int(27))]), "a1723", false);
        assert_eq!(d.hero, "a1723/hero_red_off");
        assert_eq!(d.battery, Battery::HighTemp);
        assert!(!d.healthy_temp);
        let d = build(&snap(&[("light_mode", Value::Int(4))]), "a1723", true);
        assert_eq!(d.hero, "a1723/hero_blue_on", "SOS blinks");
        let mut off = snap(&[]);
        off.link = LinkState::Disconnected;
        assert_eq!(build(&off, "a1723", false).hero, "a1723/hero_disconnect");
    }

    #[test]
    fn status_line_and_ports() {
        let d = build(
            &snap(&[
                ("remaining_time", Value::Float(100.0)),
                ("total_input_power", Value::Int(52)),
                ("total_output_power", Value::Int(41)),
                ("usb_c3_state", Value::Int(1)),
                ("usb_c3_power", Value::Int(17)),
                ("input_source", Value::Int(2)),
                ("battery_soc", Value::Int(99)),
            ]),
            "a1723",
            false,
        );
        assert_eq!(d.status, "Remaining Charging Time: 4Days 4Hr");
        assert_eq!(d.ports[2].icon, PortIcon::UsbC(1));
        assert_eq!(d.ports[2].watts, Some(17));
        assert_eq!(d.ports[5].icon, PortIcon::AcInput(true));
        assert!((d.level - 0.99).abs() < f32::EPSILON);
    }
}
