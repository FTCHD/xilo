use std::time::Duration;

use tokio::time::Instant;

use crate::{LinkState, Snapshot};

const OFFLINE_AFTER: Duration = Duration::from_secs(10 * 60);
/// How long a change in AC power must last before it is reported.
const POWER_HOLD: Duration = Duration::from_secs(20);
const HYSTERESIS: i64 = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlertKind {
    Error(i64),
    ErrorCleared,
    LowBattery(i64),
    Charged(i64),
    PowerLost,
    PowerBack,
    Offline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlertConfig {
    /// 0 turns the alert off.
    pub low_battery: i64,
    /// 0 turns the alert off.
    pub charged: i64,
    pub power: bool,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            low_battery: 20,
            charged: 100,
            power: true,
        }
    }
}

/// True when the device gets AC power, if the model reports its input source.
pub fn ac_present(snap: &Snapshot) -> Option<bool> {
    snap.int("input_source").map(|s| matches!(s, 2 | 3))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub kind: AlertKind,
    pub title: String,
    pub text: String,
}

/// Turns snapshots into alerts. Each alert fires once until its cause clears.
#[derive(Debug)]
pub struct Alerts {
    pub config: AlertConfig,
    error: Option<i64>,
    low: bool,
    charged: bool,
    last_soc: Option<i64>,
    ac: Option<bool>,
    ac_change: Option<(bool, Instant)>,
    seen_ready: bool,
    lost_at: Option<Instant>,
    offline: bool,
}

impl Alerts {
    pub fn new(config: AlertConfig) -> Self {
        Self {
            config,
            error: None,
            low: false,
            charged: false,
            last_soc: None,
            ac: None,
            ac_change: None,
            seen_ready: false,
            lost_at: None,
            offline: false,
        }
    }

    pub fn observe(&mut self, now: Instant, snap: &Snapshot) -> Vec<Alert> {
        let name = snap.display_name();
        let mut out = Vec::new();
        if snap.link == LinkState::Ready {
            self.seen_ready = true;
            self.lost_at = None;
            self.offline = false;
        } else if self.seen_ready && snap.link != LinkState::Released {
            let since = *self.lost_at.get_or_insert(now);
            if !self.offline && now.duration_since(since) >= OFFLINE_AFTER {
                self.offline = true;
                out.push(Alert {
                    kind: AlertKind::Offline,
                    title: name.clone(),
                    text: "The device has been out of reach for 10 minutes.".into(),
                });
            }
            return out;
        }
        if snap.updated.is_none() {
            return out;
        }

        let error = snap.int("error_code").filter(|c| *c != 0);
        if error != self.error {
            match error {
                Some(code) => {
                    let text = snap
                        .model
                        .as_ref()
                        .and_then(|m| m.error_text(code as u32))
                        .map(|e| e.text.clone())
                        .unwrap_or_else(|| format!("Error code {code}."));
                    out.push(Alert {
                        kind: AlertKind::Error(code),
                        title: format!("{name}: error {code}"),
                        text,
                    });
                }
                None => out.push(Alert {
                    kind: AlertKind::ErrorCleared,
                    title: name.clone(),
                    text: "The error is gone.".into(),
                }),
            }
            self.error = error;
        }

        let soc = snap.int("battery_soc");
        if let Some(soc) = soc {
            let low = self.config.low_battery;
            let charging = snap.int("battery_state") == Some(2);
            if !self.low && !charging && soc <= low && low > 0 {
                self.low = true;
                out.push(Alert {
                    kind: AlertKind::LowBattery(soc),
                    title: name.clone(),
                    text: format!("Battery is at {soc}%."),
                });
            } else if self.low && soc > low + HYSTERESIS {
                self.low = false;
            }

            let target = self.config.charged;
            let rose = self.last_soc.is_some_and(|last| last < target);
            if !self.charged && target > 0 && soc >= target && rose {
                self.charged = true;
                out.push(Alert {
                    kind: AlertKind::Charged(soc),
                    title: name.clone(),
                    text: if target >= 100 {
                        "Battery is full.".into()
                    } else {
                        format!("Battery is charged to {soc}%.")
                    },
                });
            } else if self.charged && soc < target - HYSTERESIS {
                self.charged = false;
            }
            self.last_soc = Some(soc);
        }

        if let Some(ac) = ac_present(snap) {
            match self.ac {
                None => self.ac = Some(ac),
                Some(known) if known == ac => self.ac_change = None,
                Some(_) => {
                    let since = match self.ac_change {
                        Some((value, since)) if value == ac => since,
                        _ => now,
                    };
                    self.ac_change = Some((ac, since));
                    if now.duration_since(since) >= POWER_HOLD {
                        self.ac = Some(ac);
                        self.ac_change = None;
                        if self.config.power {
                            out.push(if ac {
                                Alert {
                                    kind: AlertKind::PowerBack,
                                    title: name,
                                    text: "AC power is back.".into(),
                                }
                            } else {
                                Alert {
                                    kind: AlertKind::PowerLost,
                                    title: name,
                                    text: match soc {
                                        Some(soc) => format!(
                                            "AC power was lost. The device now runs on its battery, which is at {soc}%."
                                        ),
                                        None => "AC power was lost. The device now runs on its battery.".into(),
                                    },
                                }
                            });
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use xilo_models::Value;

    use super::*;
    use crate::DeviceAddr;

    fn powered(soc: i64, source: i64) -> Snapshot {
        let mut s = snap(soc, 0, 1);
        s.fields.insert("input_source".into(), Value::Int(source));
        s
    }

    fn snap(soc: i64, error: i64, state: i64) -> Snapshot {
        let mut s = Snapshot::new("k".into(), DeviceAddr::new("a"));
        s.link = LinkState::Ready;
        s.updated = Some(SystemTime::now());
        s.fields.insert("battery_soc".into(), Value::Int(soc));
        s.fields.insert("error_code".into(), Value::Int(error));
        s.fields.insert("battery_state".into(), Value::Int(state));
        s
    }

    #[test]
    fn deduplicates_with_hysteresis() {
        let t = Instant::now();
        let mut a = Alerts::new(AlertConfig::default());
        assert!(a.observe(t, &snap(50, 0, 1)).is_empty());
        assert_eq!(
            a.observe(t, &snap(20, 0, 1))[0].kind,
            AlertKind::LowBattery(20)
        );
        assert!(a.observe(t, &snap(19, 0, 1)).is_empty());
        assert!(a.observe(t, &snap(22, 0, 1)).is_empty());
        assert!(a.observe(t, &snap(30, 0, 1)).is_empty());
        assert_eq!(
            a.observe(t, &snap(20, 0, 1)).len(),
            1,
            "fires again after recovering"
        );

        assert_eq!(a.observe(t, &snap(50, 27, 1))[0].kind, AlertKind::Error(27));
        assert!(a.observe(t, &snap(50, 27, 1)).is_empty());
        assert_eq!(
            a.observe(t, &snap(50, 0, 1))[0].kind,
            AlertKind::ErrorCleared
        );

        assert!(a.observe(t, &snap(99, 0, 2)).is_empty());
        assert_eq!(
            a.observe(t, &snap(100, 0, 2))[0].kind,
            AlertKind::Charged(100)
        );
        assert!(a.observe(t, &snap(100, 0, 1)).is_empty());
    }

    #[test]
    fn charged_alert_at_a_chosen_level() {
        let t = Instant::now();
        let mut a = Alerts::new(AlertConfig {
            charged: 80,
            ..Default::default()
        });
        assert!(a.observe(t, &snap(85, 0, 2)).is_empty(), "already above");
        assert!(a.observe(t, &snap(60, 0, 2)).is_empty());
        let fired = a.observe(t, &snap(81, 0, 2));
        assert_eq!(fired[0].kind, AlertKind::Charged(81));
        assert_eq!(fired[0].text, "Battery is charged to 81%.");
        assert!(a.observe(t, &snap(90, 0, 2)).is_empty());
        assert!(
            a.observe(t, &snap(78, 0, 1)).is_empty(),
            "within hysteresis"
        );
        assert!(a.observe(t, &snap(80, 0, 2)).is_empty());

        a.config.charged = 0;
        a.observe(t, &snap(50, 0, 1));
        assert!(a.observe(t, &snap(100, 0, 2)).is_empty(), "turned off");
    }

    #[test]
    fn reports_ac_power_after_it_holds() {
        let t = Instant::now();
        let s = Duration::from_secs;
        let mut a = Alerts::new(AlertConfig::default());
        assert!(a.observe(t, &powered(70, 2)).is_empty(), "first reading");
        assert!(a.observe(t + s(1), &powered(70, 0)).is_empty());
        assert!(
            a.observe(t + s(5), &powered(70, 2)).is_empty(),
            "short drop"
        );
        assert!(a.observe(t + s(30), &powered(70, 2)).is_empty());

        assert!(a.observe(t + s(40), &powered(70, 0)).is_empty());
        let lost = a.observe(t + s(60), &powered(69, 0));
        assert_eq!(lost[0].kind, AlertKind::PowerLost);
        assert!(lost[0].text.contains("69%"));
        assert!(a.observe(t + s(90), &powered(68, 0)).is_empty());

        a.observe(t + s(100), &powered(68, 3));
        assert_eq!(
            a.observe(t + s(120), &powered(68, 3))[0].kind,
            AlertKind::PowerBack
        );

        a.config.power = false;
        a.observe(t + s(200), &powered(68, 1));
        assert!(a.observe(t + s(300), &powered(68, 1)).is_empty());
    }

    #[test]
    fn offline_after_ten_minutes() {
        let t = Instant::now();
        let mut a = Alerts::new(AlertConfig::default());
        a.observe(t, &snap(50, 0, 1));
        let mut lost = snap(50, 0, 1);
        lost.link = LinkState::Disconnected;
        assert!(a.observe(t, &lost).is_empty());
        assert_eq!(
            a.observe(t + OFFLINE_AFTER, &lost)[0].kind,
            AlertKind::Offline
        );
        assert!(a.observe(t + OFFLINE_AFTER * 2, &lost).is_empty());
    }
}
