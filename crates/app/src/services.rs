use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use xilo_engine::DeviceHandle;
use xilo_engine::alerts::{AlertConfig, Alerts};
use xilo_engine::automations::{
    self, AutomationConfig, ReserveConfig, ScheduleRule, UsbWakeConfig,
};
use xilo_store::history::RECORDED;
use xilo_store::{Automations, History, Preferences};

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

pub fn automation_config(cfg: &Automations) -> AutomationConfig {
    let wake = &cfg.usb_wake;
    AutomationConfig {
        usb_wake: UsbWakeConfig {
            enabled: wake.enabled,
            idle: Duration::from_secs(u64::from(wake.idle_minutes.clamp(10, 115)) * 60),
            min_battery: i64::from(wake.min_battery),
        },
        reserve: ReserveConfig {
            enabled: cfg.reserve.enabled,
            level: i64::from(cfg.reserve.level),
            outputs: cfg.reserve.outputs.clone(),
        },
        schedules: cfg
            .schedules
            .iter()
            .filter_map(|s| {
                Some(ScheduleRule {
                    command: s.command.clone(),
                    value: s.value,
                    time: jiff::civil::Time::new(s.hour as i8, s.minute as i8, 0, 0).ok()?,
                    days: s.days,
                    enabled: s.enabled,
                })
            })
            .collect(),
    }
}

pub fn alert_config(prefs: &Preferences) -> AlertConfig {
    AlertConfig {
        low_battery: i64::from(prefs.low_battery_alert),
        charged: i64::from(prefs.charged_alert),
        power: prefs.power_alerts,
    }
}

/// Background work for one device. Stops when dropped.
pub struct Services {
    pub automations: watch::Sender<AutomationConfig>,
    pub alerts: watch::Sender<AlertConfig>,
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
    pub automations: AutomationConfig,
    pub alerts: AlertConfig,
    pub record: bool,
    pub history: Option<Arc<Mutex<History>>>,
    pub events: mpsc::UnboundedSender<AppEvent>,
}

pub fn start(s: Start<'_>) -> Services {
    let _guard = s.runtime.enter();
    let (automations_tx, automations_rx) = watch::channel(s.automations);
    let (alerts_tx, alerts_rx) = watch::channel(s.alerts);
    let (record_tx, record_rx) = watch::channel(s.record);

    let (auto_tx, mut auto_rx) = mpsc::unbounded_channel();
    let automations = automations::spawn(s.handle.clone(), automations_rx, auto_tx);
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

    let alerts = tokio::spawn(alert_loop(s.handle.clone(), alerts_rx, s.events.clone()));
    let mut tasks = vec![automations, forward, alerts];
    if let Some(history) = s.history {
        tasks.push(tokio::spawn(sample_loop(s.handle, record_rx, history)));
    }
    Services {
        automations: automations_tx,
        alerts: alerts_tx,
        record: record_tx,
        tasks,
    }
}

async fn alert_loop(
    handle: DeviceHandle,
    mut config: watch::Receiver<AlertConfig>,
    events: mpsc::UnboundedSender<AppEvent>,
) {
    let mut alerts = Alerts::new(*config.borrow());
    let mut snaps = handle.watch();
    let mut tick = tokio::time::interval(Duration::from_secs(10));
    loop {
        tokio::select! {
            changed = snaps.changed() => if changed.is_err() { break },
            changed = config.changed() => {
                if changed.is_err() { break }
                alerts.config = *config.borrow();
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
