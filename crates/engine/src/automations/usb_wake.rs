use std::time::Duration;

use tokio::time::Instant;

use crate::Snapshot;

const MIN_GAP: Duration = Duration::from_secs(10 * 60);
const IDLE_WATTS: f64 = 1.0;

#[derive(Clone, Debug, PartialEq)]
pub struct UsbWakeConfig {
    pub enabled: bool,
    pub idle: Duration,
    pub min_battery: i64,
}

impl Default for UsbWakeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            idle: Duration::from_secs(100 * 60),
            min_battery: 10,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pulse {
    /// DC was off before the pulse, so turn it off again.
    pub restore_off: bool,
}

/// Keeps USB ports alive on devices that cut them after about two hours
/// of low load. Sends DC on before the cutoff, and once after connecting.
#[derive(Debug, Default)]
pub struct UsbWake {
    pub config: UsbWakeConfig,
    idle_since: Option<Instant>,
    last_pulse: Option<Instant>,
    pulsed_this_link: bool,
}

pub fn usb_watts(snap: &Snapshot) -> f64 {
    snap.fields
        .iter()
        .filter(|(k, _)| k.starts_with("usb_") && k.ends_with("_power"))
        .filter_map(|(_, v)| v.as_f64())
        .sum()
}

impl UsbWake {
    pub fn new(config: UsbWakeConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    pub fn observe(&mut self, now: Instant, snap: &Snapshot) -> Option<Pulse> {
        if !snap.is_ready() || snap.updated.is_none() {
            self.pulsed_this_link = false;
            return None;
        }
        if !self.config.enabled {
            return None;
        }
        if usb_watts(snap) >= IDLE_WATTS {
            self.idle_since = None;
            self.pulsed_this_link = true;
            return None;
        }
        let since = *self.idle_since.get_or_insert(now);
        let due = !self.pulsed_this_link || now.duration_since(since) >= self.config.idle;
        let rested = self
            .last_pulse
            .is_none_or(|t| now.duration_since(t) >= MIN_GAP);
        let charged = snap
            .int("battery_soc")
            .is_none_or(|s| s >= self.config.min_battery);
        if !(due && rested && charged) {
            return None;
        }
        self.last_pulse = Some(now);
        self.idle_since = Some(now);
        self.pulsed_this_link = true;
        Some(Pulse {
            restore_off: !snap.flag("dc_switch"),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use xilo_models::Value;

    use super::*;
    use crate::{DeviceAddr, LinkState};

    fn snap(usb: f64, dc: bool) -> Snapshot {
        let mut s = Snapshot::new("k".into(), DeviceAddr::new("a"));
        s.link = LinkState::Ready;
        s.updated = Some(SystemTime::now());
        s.fields.insert("usb_c1_power".into(), Value::Float(usb));
        s.fields.insert("dc_switch".into(), Value::Bool(dc));
        s.fields.insert("battery_soc".into(), Value::Int(80));
        s
    }

    #[test]
    fn pulses_after_connect_then_before_cutoff() {
        let t0 = Instant::now();
        let mut w = UsbWake::new(UsbWakeConfig {
            enabled: true,
            ..Default::default()
        });
        assert_eq!(
            w.observe(t0, &snap(0.0, false)),
            Some(Pulse { restore_off: true })
        );
        assert_eq!(
            w.observe(t0 + Duration::from_secs(60), &snap(0.0, true)),
            None
        );
        let due = t0 + Duration::from_secs(100 * 60);
        assert_eq!(
            w.observe(due, &snap(0.0, true)),
            Some(Pulse { restore_off: false })
        );
    }

    #[test]
    fn load_resets_the_idle_timer() {
        let t0 = Instant::now();
        let mut w = UsbWake::new(UsbWakeConfig {
            enabled: true,
            ..Default::default()
        });
        assert_eq!(w.observe(t0, &snap(15.0, false)), None, "ports in use");
        assert_eq!(
            w.observe(t0 + Duration::from_secs(99 * 60), &snap(0.2, false)),
            None
        );
        assert!(
            w.observe(t0 + Duration::from_secs(199 * 60), &snap(0.2, false))
                .is_some()
        );
    }

    #[test]
    fn respects_switch_battery_and_rate_limit() {
        let t0 = Instant::now();
        let mut off = UsbWake::new(UsbWakeConfig::default());
        assert_eq!(off.observe(t0, &snap(0.0, false)), None);

        let mut w = UsbWake::new(UsbWakeConfig {
            enabled: true,
            idle: Duration::from_secs(60),
            min_battery: 10,
        });
        let mut low = snap(0.0, false);
        low.fields.insert("battery_soc".into(), Value::Int(5));
        assert_eq!(w.observe(t0, &low), None);
        assert!(w.observe(t0, &snap(0.0, false)).is_some());
        assert_eq!(
            w.observe(t0 + Duration::from_secs(120), &snap(0.0, false)),
            None,
            "10 minute gap"
        );
        assert!(
            w.observe(t0 + Duration::from_secs(601), &snap(0.0, false))
                .is_some()
        );
    }
}
