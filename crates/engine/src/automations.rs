use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use xilo_models::CommandRequest;

use crate::{DeviceHandle, Outcome, Snapshot};

const MIN_GAP: Duration = Duration::from_secs(10 * 60);
const RESTORE_DELAY: Duration = Duration::from_secs(3);
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

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationEvent {
    pub device: String,
    pub text: String,
    pub failed: bool,
}

/// Runs USB wake for one device until the handle closes.
pub fn spawn_usb_wake(
    handle: DeviceHandle,
    mut config: watch::Receiver<UsbWakeConfig>,
    events: mpsc::UnboundedSender<AutomationEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut wake = UsbWake::new(config.borrow().clone());
        let mut snaps = handle.watch();
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                changed = snaps.changed() => if changed.is_err() { break },
                changed = config.changed() => {
                    if changed.is_err() { break }
                    wake.config = config.borrow().clone();
                }
                _ = tick.tick() => {}
            }
            let snap = snaps.borrow().clone();
            let Some(pulse) = wake.observe(Instant::now(), &snap) else {
                continue;
            };
            let device = handle.key().to_string();
            let report = |text: String, failed: bool| {
                let _ = events.send(AutomationEvent {
                    device: device.clone(),
                    text,
                    failed,
                });
            };
            match handle.send(CommandRequest::new("dc_switch", Some(1))).await {
                Ok(Outcome::Acked) => {
                    if pulse.restore_off {
                        tokio::time::sleep(RESTORE_DELAY).await;
                        let _ = handle.send(CommandRequest::new("dc_switch", Some(0))).await;
                    }
                    report("USB wake: turned the ports back on.".into(), false);
                }
                Ok(other) => report(format!("USB wake did not work: {other:?}."), true),
                Err(err) => report(format!("USB wake did not work: {err}."), true),
            }
        }
    })
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
