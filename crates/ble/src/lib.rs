#![deny(clippy::unwrap_used)]

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use btleplug::api::{
    Central, CentralEvent, CharPropFlags, Characteristic, Manager as _, Peripheral as _,
    ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use uuid::Uuid;
use xilo_engine::{
    Advert, Connector, DeviceAddr, Link, LinkEvent, LinkInfo, Scanner, TransportError,
};
use xilo_proto::gatt::{self, GattSet};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const FIND_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_WRITE: usize = 180;

pub struct BleTransport {
    adapter: Adapter,
    seen: Mutex<HashMap<String, Peripheral>>,
    scanning: tokio::sync::Mutex<()>,
}

impl BleTransport {
    pub async fn new() -> Result<Arc<Self>, TransportError> {
        let manager = Manager::new()
            .await
            .map_err(|e| TransportError::Unavailable(e.to_string()))?;
        let adapter = manager
            .adapters()
            .await
            .map_err(|e| TransportError::Unavailable(e.to_string()))?
            .into_iter()
            .next()
            .ok_or_else(|| TransportError::Unavailable("no Bluetooth adapter".into()))?;
        Ok(Arc::new(Self {
            adapter,
            seen: Mutex::new(HashMap::new()),
            scanning: tokio::sync::Mutex::new(()),
        }))
    }

    fn remember(&self, peripherals: &[Peripheral]) {
        let mut seen = self
            .seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for p in peripherals {
            seen.insert(p.id().to_string(), p.clone());
        }
    }

    fn known(&self, id: &str) -> Option<Peripheral> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(id)
            .cloned()
    }

    fn filter() -> ScanFilter {
        ScanFilter {
            services: gatt::advertised_services()
                .into_iter()
                .filter_map(|s| Uuid::from_str(s).ok())
                .collect(),
        }
    }

    /// Scans until `window` ends or the peripheral with `target` id appears.
    async fn run_scan(
        &self,
        window: Duration,
        target: Option<&str>,
    ) -> Result<Vec<Peripheral>, TransportError> {
        let _guard = self.scanning.lock().await;
        let mut events = self
            .adapter
            .events()
            .await
            .map_err(|e| TransportError::Unavailable(e.to_string()))?;
        self.adapter
            .start_scan(Self::filter())
            .await
            .map_err(|e| TransportError::Unavailable(e.to_string()))?;
        let deadline = tokio::time::Instant::now() + window;
        if let Some(target) = target {
            while let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.next()).await {
                if let CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id) = event
                    && id.to_string() == target
                {
                    break;
                }
            }
        } else {
            tokio::time::sleep_until(deadline).await;
        }
        let _ = self.adapter.stop_scan().await;
        let peripherals = self
            .adapter
            .peripherals()
            .await
            .map_err(|e| TransportError::Unavailable(e.to_string()))?;
        self.remember(&peripherals);
        Ok(peripherals)
    }
}

async fn advert(p: &Peripheral) -> Option<Advert> {
    let props = p.properties().await.ok().flatten()?;
    Some(Advert {
        addr: Some(DeviceAddr::new(p.id().to_string())),
        name: props.local_name.or(props.advertisement_name),
        rssi: props.rssi,
        services: props.services.iter().map(Uuid::to_string).collect(),
        service_data: props
            .service_data
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        manufacturer_data: props.manufacturer_data,
    })
}

#[async_trait]
impl Scanner for BleTransport {
    async fn scan(&self, window: Duration) -> Result<Vec<Advert>, TransportError> {
        let peripherals = self.run_scan(window, None).await?;
        let mut out = Vec::new();
        for p in &peripherals {
            if let Some(a) = advert(p).await {
                out.push(a);
            }
        }
        Ok(out)
    }
}

#[async_trait]
impl Connector for BleTransport {
    async fn connect(&self, addr: &DeviceAddr) -> Result<Box<dyn Link>, TransportError> {
        let peripheral = match self.known(&addr.id) {
            Some(p) => p,
            None => {
                self.run_scan(FIND_TIMEOUT, Some(&addr.id)).await?;
                self.known(&addr.id).ok_or(TransportError::NotFound)?
            }
        };
        tracing::debug!(device = %addr, "connecting");
        let connected = peripheral.is_connected().await.unwrap_or(false);
        if !connected {
            let mut attempt = tokio::time::timeout(CONNECT_TIMEOUT, peripheral.connect())
                .await
                .map_err(|_| TransportError::Timeout)?;
            if let Err(e) = &attempt
                && e.to_string().contains("no longer available")
            {
                tracing::debug!(device = %addr, "rediscovering peripheral");
                self.run_scan(FIND_TIMEOUT, Some(&addr.id)).await?;
                attempt = tokio::time::timeout(CONNECT_TIMEOUT, peripheral.connect())
                    .await
                    .map_err(|_| TransportError::Timeout)?;
            }
            attempt.map_err(|e| {
                tracing::warn!(device = %addr, "connect error: {e:?}");
                TransportError::Connect(e.to_string())
            })?;
        }
        tokio::time::timeout(CONNECT_TIMEOUT, peripheral.discover_services())
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|e| TransportError::Connect(e.to_string()))?;

        let (set, write, notify) = match find_characteristics(&peripheral) {
            Some(found) => found,
            None => {
                let _ = peripheral.disconnect().await;
                return Err(TransportError::NoService);
            }
        };
        peripheral
            .subscribe(&notify)
            .await
            .map_err(|e| TransportError::Connect(e.to_string()))?;

        let (tx, rx) = mpsc::channel(256);
        let mut notifications = peripheral
            .notifications()
            .await
            .map_err(|e| TransportError::Connect(e.to_string()))?;
        let notify_uuid = notify.uuid;
        let tx_notes = tx.clone();
        let notes_task = tokio::spawn(async move {
            while let Some(n) = notifications.next().await {
                if n.uuid == notify_uuid && tx_notes.send(LinkEvent::Notify(n.value)).await.is_err()
                {
                    break;
                }
            }
            let _ = tx_notes
                .send(LinkEvent::Disconnected("notifications ended".into()))
                .await;
        });
        let mut events = self
            .adapter
            .events()
            .await
            .map_err(|e| TransportError::Connect(e.to_string()))?;
        let id = peripheral.id();
        let events_task = tokio::spawn(async move {
            while let Some(event) = events.next().await {
                if let CentralEvent::DeviceDisconnected(gone) = event
                    && gone == id
                {
                    let _ = tx
                        .send(LinkEvent::Disconnected("device disconnected".into()))
                        .await;
                    break;
                }
            }
        });

        let mtu = usize::from(peripheral.mtu());
        let max_write = if mtu > 23 {
            (mtu - 3).min(MAX_WRITE)
        } else {
            MAX_WRITE
        };
        let write_type = if write.properties.contains(CharPropFlags::WRITE) {
            WriteType::WithResponse
        } else {
            WriteType::WithoutResponse
        };
        Ok(Box::new(BleLink {
            peripheral,
            write,
            write_type,
            rx,
            tasks: vec![notes_task, events_task],
            info: LinkInfo {
                gatt: set.name,
                max_write,
            },
        }))
    }
}

fn find_characteristics(p: &Peripheral) -> Option<(GattSet, Characteristic, Characteristic)> {
    for service in p.services() {
        let Some(set) = gatt::by_service(&service.uuid.to_string()) else {
            continue;
        };
        let find = |uuid: &str| {
            service
                .characteristics
                .iter()
                .find(|c| c.uuid.to_string().eq_ignore_ascii_case(uuid))
                .cloned()
        };
        if let (Some(w), Some(n)) = (find(set.write), find(set.notify)) {
            return Some((set, w, n));
        }
    }
    None
}

pub struct BleLink {
    peripheral: Peripheral,
    write: Characteristic,
    write_type: WriteType,
    rx: mpsc::Receiver<LinkEvent>,
    tasks: Vec<JoinHandle<()>>,
    info: LinkInfo,
}

#[async_trait]
impl Link for BleLink {
    fn info(&self) -> &LinkInfo {
        &self.info
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.peripheral
            .write(&self.write, bytes, self.write_type)
            .await
            .map_err(|e| TransportError::Write(e.to_string()))
    }

    async fn next_event(&mut self) -> Option<LinkEvent> {
        self.rx.recv().await
    }

    async fn close(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
        let _ = self.peripheral.disconnect().await;
    }
}

impl Drop for BleLink {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
