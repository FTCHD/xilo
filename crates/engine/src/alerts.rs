use std::time::Duration;

use tokio::time::Instant;

use crate::{LinkState, Snapshot};

const OFFLINE_AFTER: Duration = Duration::from_secs(10 * 60);
const HYSTERESIS: i64 = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlertKind {
    Error(i64),
    ErrorCleared,
    LowBattery(i64),
    Full,
    Offline,
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
    pub low_battery: i64,
    error: Option<i64>,
    low: bool,
    full: bool,
    seen_ready: bool,
    lost_at: Option<Instant>,
    offline: bool,
}

impl Alerts {
    pub fn new(low_battery: i64) -> Self {
        Self {
            low_battery,
            error: None,
            low: false,
            full: false,
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

        if let Some(soc) = snap.int("battery_soc") {
            let charging = snap.int("battery_state") == Some(2);
            if !self.low && !charging && soc <= self.low_battery && self.low_battery > 0 {
                self.low = true;
                out.push(Alert {
                    kind: AlertKind::LowBattery(soc),
                    title: name.clone(),
                    text: format!("Battery is at {soc}%."),
                });
            } else if self.low && soc > self.low_battery + HYSTERESIS {
                self.low = false;
            }
            if !self.full && soc >= 100 && charging {
                self.full = true;
                out.push(Alert {
                    kind: AlertKind::Full,
                    title: name,
                    text: "Battery is full.".into(),
                });
            } else if self.full && soc < 100 - HYSTERESIS {
                self.full = false;
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
        let mut a = Alerts::new(20);
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

        assert_eq!(a.observe(t, &snap(100, 0, 2))[0].kind, AlertKind::Full);
        assert!(a.observe(t, &snap(100, 0, 2)).is_empty());
    }

    #[test]
    fn offline_after_ten_minutes() {
        let t = Instant::now();
        let mut a = Alerts::new(20);
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
