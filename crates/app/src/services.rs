use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use xilo_engine::DeviceHandle;
use xilo_engine::alerts::Alerts;
use xilo_engine::automations::{self, UsbWakeConfig};
use xilo_store::history::RECORDED;
use xilo_store::{History, UsbWake};

const SAMPLE_EVERY: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Alert,
    Automation,
}

#[derive(Clone, Debug)]
pub struct AppEvent {
    pub device: String,
    pub kind: EventKind,
    pub title: String,
    pub text: String,
    pub failed: bool,
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn wake_config(cfg: &UsbWake) -> UsbWakeConfig {
    UsbWakeConfig {
        enabled: cfg.enabled,
        idle: Duration::from_secs(u64::from(cfg.idle_minutes.clamp(10, 115)) * 60),
        min_battery: i64::from(cfg.min_battery),
    }
}

/// Background work for one device. Stops when dropped.
pub struct Services {
    pub wake: watch::Sender<UsbWakeConfig>,
    pub low_battery: watch::Sender<i64>,
    pub record: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for Services {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

pub struct Start<'a> {
    pub runtime: &'a tokio::runtime::Runtime,
    pub handle: DeviceHandle,
    pub wake: UsbWakeConfig,
    pub low_battery: i64,
    pub record: bool,
    pub history: Option<Arc<Mutex<History>>>,
    pub events: mpsc::UnboundedSender<AppEvent>,
}

pub fn start(s: Start<'_>) -> Services {
    let _guard = s.runtime.enter();
    let (wake_tx, wake_rx) = watch::channel(s.wake);
    let (low_tx, low_rx) = watch::channel(s.low_battery);
    let (record_tx, record_rx) = watch::channel(s.record);

    let (auto_tx, mut auto_rx) = mpsc::unbounded_channel();
    let wake = automations::spawn_usb_wake(s.handle.clone(), wake_rx, auto_tx);
    let events = s.events.clone();
    let name_handle = s.handle.clone();
    let forward = tokio::spawn(async move {
        while let Some(e) = auto_rx.recv().await {
            let _ = events.send(AppEvent {
                title: name_handle.snapshot().display_name(),
                device: e.device,
                kind: EventKind::Automation,
                text: e.text,
                failed: e.failed,
            });
        }
    });

    let alerts = tokio::spawn(alert_loop(s.handle.clone(), low_rx, s.events.clone()));
    let mut tasks = vec![wake, forward, alerts];
    if let Some(history) = s.history {
        tasks.push(tokio::spawn(sample_loop(s.handle, record_rx, history)));
    }
    Services {
        wake: wake_tx,
        low_battery: low_tx,
        record: record_tx,
        tasks,
    }
}

async fn alert_loop(
    handle: DeviceHandle,
    mut low: watch::Receiver<i64>,
    events: mpsc::UnboundedSender<AppEvent>,
) {
    let mut alerts = Alerts::new(*low.borrow());
    let mut snaps = handle.watch();
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            changed = snaps.changed() => if changed.is_err() { break },
            changed = low.changed() => {
                if changed.is_err() { break }
                alerts.low_battery = *low.borrow();
            }
            _ = tick.tick() => {}
        }
        let snap = snaps.borrow().clone();
        for alert in alerts.observe(tokio::time::Instant::now(), &snap) {
            let _ = events.send(AppEvent {
                device: handle.key().to_string(),
                kind: EventKind::Alert,
                title: alert.title,
                text: alert.text,
                failed: false,
            });
        }
    }
}

async fn sample_loop(
    handle: DeviceHandle,
    record: watch::Receiver<bool>,
    history: Arc<Mutex<History>>,
) {
    let mut tick = tokio::time::interval(SAMPLE_EVERY);
    loop {
        tick.tick().await;
        if !*record.borrow() {
            continue;
        }
        let snap = handle.snapshot();
        let fresh = snap
            .updated
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age < Duration::from_secs(90));
        if !snap.is_ready() || !fresh {
            continue;
        }
        let values: Vec<(&str, f64)> = RECORDED
            .iter()
            .filter_map(|f| snap.float(f).map(|v| (*f, v)))
            .collect();
        if values.is_empty() {
            continue;
        }
        let key = handle.key().to_string();
        let history = history.clone();
        let ts = unix_now();
        let owned: Vec<(String, f64)> = values
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let _ = tokio::task::spawn_blocking(move || {
            let refs: Vec<(&str, f64)> = owned.iter().map(|(k, v)| (k.as_str(), *v)).collect();
            if let Ok(mut h) = history.lock()
                && let Err(err) = h.record(&key, ts, &refs)
            {
                tracing::warn!("history write failed: {err}");
            }
        })
        .await;
    }
}
