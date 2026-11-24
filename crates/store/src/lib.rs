#![deny(clippy::unwrap_used)]

pub mod capture;
pub mod config;
pub mod history;

use std::path::PathBuf;

pub use config::{
    Automations, Config, DeviceConfig, EVERY_DAY, Preferences, Reserve, Schedule, ThemeChoice,
    UsbWake,
};
pub use history::History;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not read data: {0}")]
    Parse(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
}

/// Where Xilo keeps its files.
#[derive(Clone, Debug)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub logs: PathBuf,
}

impl Paths {
    pub fn detect() -> Self {
        if let Some(home) = std::env::var_os("XILO_HOME") {
            let base = PathBuf::from(home);
            return Self {
                config: base.clone(),
                data: base.clone(),
                logs: base.join("logs"),
            };
        }
        match directories::ProjectDirs::from("dev", "xilo", "Xilo") {
            Some(dirs) => Self {
                config: dirs.config_dir().to_path_buf(),
                data: dirs.data_dir().to_path_buf(),
                logs: dirs.data_dir().join("logs"),
            },
            None => {
                let base = std::env::temp_dir().join("xilo");
                Self {
                    config: base.clone(),
                    data: base.clone(),
                    logs: base.join("logs"),
                }
            }
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    pub fn identity_file(&self) -> PathBuf {
        self.config.join("identity.toml")
    }

    pub fn history_file(&self) -> PathBuf {
        self.data.join("history.sqlite")
    }

    pub fn captures(&self) -> PathBuf {
        self.data.join("captures")
    }

    pub fn models(&self) -> PathBuf {
        self.config.join("models")
    }

    pub fn audit_file(&self) -> PathBuf {
        self.data.join("audit.log")
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Identity {
    client_id: String,
}

/// The client id this installation uses with devices. Created once.
pub fn client_id(paths: &Paths, create: impl FnOnce() -> String) -> Result<String, StoreError> {
    let path = paths.identity_file();
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(id) = toml::from_str::<Identity>(&text)
        && id.client_id.len() == 36
    {
        return Ok(id.client_id);
    }
    let id = Identity {
        client_id: create(),
    };
    let text = toml::to_string(&id).map_err(|e| StoreError::Parse(e.to_string()))?;
    config::write_atomic(&path, text.as_bytes())?;
    Ok(id.client_id)
}

/// Appends a line to the audit log of dangerous actions.
pub fn audit(paths: &Paths, line: &str) -> Result<(), StoreError> {
    use std::io::Write;
    std::fs::create_dir_all(&paths.data)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.audit_file())?;
    writeln!(file, "{line}")?;
    Ok(())
}

/// The user's Downloads folder, or the home folder.
pub fn download_dir() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|d| {
            d.download_dir()
                .map(|p| p.to_path_buf())
                .or(Some(d.home_dir().to_path_buf()))
        })
        .unwrap_or_else(std::env::temp_dir)
}
