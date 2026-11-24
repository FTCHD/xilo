use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::StoreError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub theme: ThemeChoice,
    pub keep_running_in_tray: bool,
    pub launch_at_login: bool,
    pub notifications: bool,
    pub low_battery_alert: u8,
    /// Battery level for the charged alert. 0 turns it off.
    pub charged_alert: u8,
    pub power_alerts: bool,
    pub record_history: bool,
    pub history_days: u32,
    pub developer_mode: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            keep_running_in_tray: true,
            launch_at_login: false,
            notifications: true,
            low_battery_alert: 20,
            charged_alert: 100,
            power_alerts: true,
            record_history: true,
            history_days: 90,
            developer_mode: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UsbWake {
    pub enabled: bool,
    pub idle_minutes: u32,
    pub min_battery: u8,
}

impl Default for UsbWake {
    fn default() -> Self {
        Self {
            enabled: false,
            idle_minutes: 100,
            min_battery: 10,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reserve {
    pub enabled: bool,
    pub level: u8,
    /// Commands to switch off at the reserve level.
    pub outputs: Vec<String>,
}

impl Default for Reserve {
    fn default() -> Self {
        Self {
            enabled: false,
            level: 20,
            outputs: vec!["ac_switch".into(), "dc_switch".into()],
        }
    }
}

pub const EVERY_DAY: u8 = 0x7f;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    pub command: String,
    pub value: i64,
    pub hour: u8,
    pub minute: u8,
    /// Monday is bit 0, Sunday is bit 6.
    #[serde(default = "every_day")]
    pub days: u8,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn every_day() -> u8 {
    EVERY_DAY
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Automations {
    pub usb_wake: UsbWake,
    pub reserve: Reserve,
    pub schedules: Vec<Schedule>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub key: String,
    pub address: String,
    pub name: Option<String>,
    pub model: Option<String>,
    #[serde(default = "yes")]
    pub auto_connect: bool,
    #[serde(default)]
    pub automations: Automations,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub preferences: Preferences,
    #[serde(rename = "device")]
    pub devices: Vec<DeviceConfig>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, StoreError> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|e| StoreError::Parse(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), StoreError> {
        let text = toml::to_string_pretty(self).map_err(|e| StoreError::Parse(e.to_string()))?;
        write_atomic(path, text.as_bytes())
    }

    pub fn device(&self, key: &str) -> Option<&DeviceConfig> {
        self.devices.iter().find(|d| d.key == key)
    }

    pub fn device_mut(&mut self, key: &str) -> Option<&mut DeviceConfig> {
        self.devices.iter_mut().find(|d| d.key == key)
    }

    pub fn upsert(&mut self, device: DeviceConfig) {
        match self.device_mut(&device.key) {
            Some(existing) => *existing = device,
            None => self.devices.push(device),
        }
    }

    pub fn remove(&mut self, key: &str) {
        self.devices.retain(|d| d.key != key);
    }
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp: PathBuf = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_defaults() {
        let dir = std::env::temp_dir().join(format!("xilo-config-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config::load(&path).expect("missing file is default");
        assert_eq!(config, Config::default());
        config.upsert(DeviceConfig {
            key: "ALFSBK1G33300077".into(),
            address: "e981".into(),
            name: Some("Desk".into()),
            model: Some("A1723".into()),
            auto_connect: true,
            automations: Automations {
                schedules: vec![Schedule {
                    command: "ac_switch".into(),
                    value: 0,
                    hour: 23,
                    minute: 30,
                    days: 0x1f,
                    enabled: true,
                }],
                ..Default::default()
            },
        });
        config.preferences.low_battery_alert = 15;
        config.save(&path).expect("save");
        assert_eq!(Config::load(&path).expect("load"), config);
        let partial: Config =
            toml::from_str("[preferences]\ndeveloper_mode = true\n").expect("partial");
        assert!(partial.preferences.developer_mode);
        assert!(partial.preferences.keep_running_in_tray);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod compat {
    #[test]
    fn reads_files_with_removed_settings() {
        let text = "[[device]]\nkey = \"k\"\naddress = \"a\"\nallow_unverified = true\n";
        let config: super::Config = toml::from_str(text).expect("old file still loads");
        assert_eq!(config.devices.len(), 1);
    }
}
