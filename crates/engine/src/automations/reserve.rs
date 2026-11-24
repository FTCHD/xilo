use xilo_models::ModelSpec;

use crate::Snapshot;

/// Points above the reserve level the battery must reach before the reserve acts again.
const REARM: i64 = 5;

#[derive(Clone, Debug, PartialEq)]
pub struct ReserveConfig {
    pub enabled: bool,
    pub level: i64,
    pub outputs: Vec<String>,
}

impl Default for ReserveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            level: 20,
            outputs: vec!["ac_switch".into(), "dc_switch".into()],
        }
    }
}

/// Switches outputs off when the battery drops to the reserve level while it discharges.
/// It acts once for each drop, so outputs that the user turns on again stay on.
#[derive(Debug, Default)]
pub struct Reserve {
    config: ReserveConfig,
    tripped: bool,
}

fn charging(snap: &Snapshot) -> bool {
    snap.int("battery_state") == Some(2)
}

fn is_on(model: &ModelSpec, command: &str, snap: &Snapshot) -> bool {
    model
        .command(command)
        .is_some_and(|c| model.current(c, &snap.fields) == Some(c.on_value))
}

impl Reserve {
    pub fn new(config: ReserveConfig) -> Self {
        Self {
            config,
            tripped: false,
        }
    }

    pub fn set_config(&mut self, config: ReserveConfig) {
        if config != self.config {
            self.config = config;
            self.tripped = false;
        }
    }

    /// The outputs to switch off now.
    pub fn observe(&mut self, snap: &Snapshot) -> Vec<String> {
        if !self.config.enabled || !snap.is_ready() || snap.updated.is_none() {
            return Vec::new();
        }
        let (Some(soc), Some(model)) = (snap.int("battery_soc"), snap.model.as_deref()) else {
            return Vec::new();
        };
        if self.tripped {
            if soc > self.config.level + REARM {
                self.tripped = false;
            }
            return Vec::new();
        }
        if soc > self.config.level || charging(snap) {
            return Vec::new();
        }
        self.tripped = true;
        self.config
            .outputs
            .iter()
            .filter(|id| is_on(model, id, snap))
            .cloned()
            .collect()
    }

    /// True when a scheduled command would turn on an output that the reserve keeps off.
    pub fn blocks(&self, command: &str, value: i64, snap: &Snapshot) -> bool {
        let below = snap
            .int("battery_soc")
            .is_some_and(|soc| soc <= self.config.level);
        let turns_on = snap
            .model
            .as_deref()
            .and_then(|m| m.command(command))
            .is_some_and(|c| c.on_value == value);
        self.config.enabled
            && below
            && turns_on
            && !charging(snap)
            && self.config.outputs.iter().any(|o| o == command)
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use xilo_models::{Registry, Value};

    use super::*;
    use crate::{DeviceAddr, LinkState};

    fn snap(soc: i64, ac: bool, dc: bool) -> Snapshot {
        let mut s = Snapshot::new("k".into(), DeviceAddr::new("a"));
        s.link = LinkState::Ready;
        s.updated = Some(SystemTime::now());
        s.model = Registry::builtin().get("A1723").cloned();
        s.fields.insert("battery_soc".into(), Value::Int(soc));
        s.fields.insert("ac_switch".into(), Value::Bool(ac));
        s.fields.insert("dc_switch".into(), Value::Bool(dc));
        s
    }

    fn enabled(level: i64) -> Reserve {
        Reserve::new(ReserveConfig {
            enabled: true,
            level,
            ..Default::default()
        })
    }

    #[test]
    fn turns_on_outputs_off_once_per_drop() {
        let mut r = enabled(20);
        assert!(r.observe(&snap(40, true, true)).is_empty());
        assert_eq!(r.observe(&snap(20, true, false)), vec!["ac_switch"]);
        assert!(
            r.observe(&snap(18, true, true)).is_empty(),
            "the user turned them on again"
        );
        assert!(
            r.observe(&snap(24, true, true)).is_empty(),
            "not rearmed yet"
        );
        assert!(r.observe(&snap(26, true, true)).is_empty());
        assert_eq!(
            r.observe(&snap(20, true, true)),
            vec!["ac_switch", "dc_switch"]
        );
    }

    #[test]
    fn off_or_disconnected_does_nothing() {
        let mut off = Reserve::new(ReserveConfig::default());
        assert!(off.observe(&snap(5, true, true)).is_empty());
        let mut r = enabled(20);
        let mut lost = snap(5, true, true);
        lost.link = LinkState::Disconnected;
        assert!(r.observe(&lost).is_empty());
        assert_eq!(r.observe(&snap(5, true, false)), vec!["ac_switch"]);
    }

    #[test]
    fn waits_while_charging() {
        let mut r = enabled(20);
        let mut charging = snap(15, true, false);
        charging
            .fields
            .insert("battery_state".into(), Value::Int(2));
        assert!(r.observe(&charging).is_empty());
        assert!(!r.blocks("ac_switch", 1, &charging));
        assert_eq!(r.observe(&snap(15, true, false)), vec!["ac_switch"]);
    }

    #[test]
    fn new_level_acts_again() {
        let mut r = enabled(20);
        assert_eq!(r.observe(&snap(15, true, false)).len(), 1);
        r.set_config(ReserveConfig {
            enabled: true,
            level: 30,
            ..Default::default()
        });
        assert_eq!(r.observe(&snap(15, true, false)).len(), 1);
    }

    #[test]
    fn blocks_schedules_that_turn_outputs_on() {
        let r = enabled(20);
        assert!(r.blocks("ac_switch", 1, &snap(15, false, false)));
        assert!(!r.blocks("ac_switch", 0, &snap(15, true, false)));
        assert!(!r.blocks("ac_switch", 1, &snap(50, false, false)));
        assert!(!r.blocks("screen_switch", 1, &snap(15, false, false)));
    }
}
