use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gpui_kit::*;
use tokio::sync::mpsc;
use xilo_engine::{DeviceAddr, DeviceSetup};
use xilo_store::{Config, DeviceConfig, History, Preferences, UsbWake};

use crate::device::DeviceModel;
use crate::runtime::Runtime;
use crate::services::{self, AppEvent, Services};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Dashboard,
    Settings,
    History,
    Automations,
    Inspector,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Route {
    Device(String, Tab),
    AddDevice,
    Preferences,
    About,
}

pub struct AppState {
    pub config: Config,
    pub devices: Vec<(String, Entity<DeviceModel>)>,
    pub route: Route,
    pub history: Option<Arc<Mutex<History>>>,
    services: HashMap<String, Services>,
    events: mpsc::UnboundedSender<AppEvent>,
}

impl AppState {
    pub fn new(
        config: Config,
        history: Option<History>,
        events: mpsc::UnboundedSender<AppEvent>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut state = Self {
            route: match config.devices.first() {
                Some(d) => Route::Device(d.key.clone(), Tab::Dashboard),
                None => Route::AddDevice,
            },
            config,
            devices: Vec::new(),
            history: history.map(|h| Arc::new(Mutex::new(h))),
            services: HashMap::new(),
            events,
        };
        for device in state.config.devices.clone() {
            state.start(&device, cx);
        }
        state
    }

    fn start(&mut self, device: &DeviceConfig, cx: &mut Context<Self>) -> Entity<DeviceModel> {
        let setup = DeviceSetup {
            key: device.key.clone(),
            addr: DeviceAddr::new(device.address.clone()),
            name: device.name.clone(),
            model: device.model.clone(),
            auto_connect: device.auto_connect,
        };
        let handle = Runtime::enter(cx, |engine| engine.add(setup));
        let prefs = &self.config.preferences;
        let started = services::start(services::Start {
            runtime: &Runtime::get(cx).tokio,
            handle: handle.clone(),
            wake: services::wake_config(&device.automations.usb_wake),
            low_battery: i64::from(prefs.low_battery_alert),
            record: prefs.record_history,
            history: self.history.clone(),
            events: self.events.clone(),
        });
        self.services.insert(device.key.clone(), started);
        let entity = cx.new(|cx| DeviceModel::new(handle, cx));
        cx.subscribe(&entity, |state: &mut Self, device, _event, cx| {
            state.remember_model(&device.read(cx).key.clone(), cx);
        })
        .detach();
        self.devices.push((device.key.clone(), entity.clone()));
        entity
    }

    pub fn add_device(&mut self, address: String, name: Option<String>, cx: &mut Context<Self>) {
        if self.config.device(&address).is_some() {
            self.route = Route::Device(address, Tab::Dashboard);
            cx.notify();
            return;
        }
        let device = DeviceConfig {
            key: address.clone(),
            address: address.clone(),
            name,
            model: None,
            auto_connect: true,
            automations: Default::default(),
        };
        self.start(&device, cx);
        self.config.upsert(device);
        self.save();
        self.route = Route::Device(address, Tab::Dashboard);
        cx.notify();
    }

    pub fn remove_device(&mut self, key: &str, cx: &mut Context<Self>) {
        self.devices.retain(|(k, _)| k != key);
        self.services.remove(key);
        if let Some(history) = &self.history
            && let Ok(h) = history.lock()
        {
            let _ = h.forget(key);
        }
        self.config.remove(key);
        self.save();
        let key = key.to_string();
        let engine = Runtime::get(cx).engine.clone();
        Runtime::spawn(cx, async move { engine.remove(&key).await });
        self.route = match self.config.devices.first() {
            Some(d) => Route::Device(d.key.clone(), Tab::Dashboard),
            None => Route::AddDevice,
        };
        cx.notify();
    }

    fn remember_model(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(device) = self.device(key) else {
            return;
        };
        let code = device.read(cx).snap.model.as_ref().map(|m| m.code.clone());
        if let Some(cfg) = self.config.device_mut(key)
            && code.is_some()
            && cfg.model != code
        {
            cfg.model = code;
            self.save();
        }
    }

    pub fn device(&self, key: &str) -> Option<Entity<DeviceModel>> {
        self.devices
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, d)| d.clone())
    }

    pub fn entities(&self) -> impl Iterator<Item = &Entity<DeviceModel>> {
        self.devices.iter().map(|(_, d)| d)
    }

    pub fn device_config_mut(&mut self, key: &str) -> Option<&mut DeviceConfig> {
        self.config.device_mut(key)
    }

    pub fn set_usb_wake(&mut self, key: &str, wake: UsbWake) {
        if let Some(s) = self.services.get(key) {
            s.wake.send_replace(services::wake_config(&wake));
        }
        if let Some(d) = self.config.device_mut(key) {
            d.automations.usb_wake = wake;
        }
        self.save();
    }

    pub fn set_preferences(&mut self, prefs: Preferences, cx: &mut Context<Self>) {
        for s in self.services.values() {
            s.low_battery
                .send_replace(i64::from(prefs.low_battery_alert));
            s.record.send_replace(prefs.record_history);
        }
        cx.set_quit_mode(if prefs.keep_running_in_tray {
            QuitMode::Explicit
        } else {
            QuitMode::LastWindowClosed
        });
        if prefs.launch_at_login != self.config.preferences.launch_at_login
            && let Err(err) = crate::login::set(prefs.launch_at_login)
        {
            tracing::warn!("could not change open at login: {err}");
        }
        self.config.preferences = prefs;
        self.save();
        cx.notify();
    }

    pub fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        self.route = route;
        cx.notify();
    }

    pub fn save(&self) {
        if let Some(paths) = PATHS.get()
            && let Err(err) = self.config.save(&paths.config_file())
        {
            tracing::error!("could not save settings: {err}");
        }
    }
}

pub static PATHS: std::sync::OnceLock<xilo_store::Paths> = std::sync::OnceLock::new();
