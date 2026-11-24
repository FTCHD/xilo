use std::sync::{Arc, Mutex};
use std::time::Duration;

use indexmap::IndexMap;
use tokio::sync::watch;
use xilo_models::Registry;
use xilo_proto::TimestampMode;
use xilo_proto::gatt;

use crate::actor::{self, DeviceHandle, DeviceSetup, Shared};
use crate::transport::{Advert, Connector, Scanner, TransportError};

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub client_id: String,
    pub timestamps: TimestampMode,
    /// POSIX time zone for devices with a clock.
    pub timezone: String,
    pub country: String,
}

/// A device found while scanning.
#[derive(Clone, Debug)]
pub struct Found {
    pub advert: Advert,
    pub known: bool,
}

pub struct Engine {
    shared: Arc<Shared>,
    scanner: Arc<dyn Scanner>,
    devices: Mutex<IndexMap<String, DeviceHandle>>,
    list: watch::Sender<Vec<String>>,
}

impl Engine {
    pub fn new(
        connector: Arc<dyn Connector>,
        scanner: Arc<dyn Scanner>,
        registry: Arc<Registry>,
        config: EngineConfig,
    ) -> Arc<Self> {
        Arc::new(Self {
            shared: Arc::new(Shared {
                connector,
                registry,
                client_id: config.client_id,
                timestamps: config.timestamps,
                timezone: config.timezone,
                country: config.country,
            }),
            scanner,
            devices: Mutex::new(IndexMap::new()),
            list: watch::channel(Vec::new()).0,
        })
    }

    pub fn registry(&self) -> &Arc<Registry> {
        &self.shared.registry
    }

    /// Starts a device actor. Must be called inside a tokio runtime.
    pub fn add(&self, setup: DeviceSetup) -> DeviceHandle {
        let mut devices = self.lock();
        if let Some(existing) = devices.get(&setup.key) {
            return existing.clone();
        }
        let handle = actor::spawn(setup, self.shared.clone());
        devices.insert(handle.key().to_string(), handle.clone());
        self.list.send_replace(devices.keys().cloned().collect());
        handle
    }

    pub async fn remove(&self, key: &str) {
        let handle = {
            let mut devices = self.lock();
            let handle = devices.shift_remove(key);
            self.list.send_replace(devices.keys().cloned().collect());
            handle
        };
        if let Some(handle) = handle {
            handle.shutdown().await;
        }
    }

    pub fn device(&self, key: &str) -> Option<DeviceHandle> {
        self.lock().get(key).cloned()
    }

    pub fn devices(&self) -> Vec<DeviceHandle> {
        self.lock().values().cloned().collect()
    }

    pub fn watch_list(&self) -> watch::Receiver<Vec<String>> {
        self.list.subscribe()
    }

    pub async fn scan(&self, window: Duration) -> Result<Vec<Found>, TransportError> {
        let adverts = self.scanner.scan(window).await?;
        let known: Vec<String> = self
            .devices()
            .iter()
            .map(|d| d.snapshot().addr.id.clone())
            .collect();
        let solix = gatt::advertised_services();
        Ok(adverts
            .into_iter()
            .filter(|a| {
                a.services
                    .iter()
                    .any(|s| solix.iter().any(|x| x.eq_ignore_ascii_case(s)))
                    || a.name.as_deref().is_some_and(|n| n.contains("SOLIX"))
            })
            .map(|advert| Found {
                known: advert.addr.as_ref().is_some_and(|a| known.contains(&a.id)),
                advert,
            })
            .collect())
    }

    pub async fn shutdown(&self) {
        for device in self.devices() {
            device.shutdown().await;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, IndexMap<String, DeviceHandle>> {
        self.devices
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
