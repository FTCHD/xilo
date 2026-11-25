#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used)]

mod add_device;
mod assets;
mod automations;
mod device;
mod device_view;
mod families;
mod history_view;
mod inspector;
mod instance;
mod login;
mod preferences;
mod runtime;
mod services;
mod settings;
mod shell;
mod state;
mod theme;
mod tray;
mod widgets;

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::TitleBar;
use gpui_kit::*;
use tokio::sync::mpsc;
use xilo_engine::sim::{SimConnector, SimDevice};
use xilo_engine::{Connector, Engine, EngineConfig, Scanner};
use xilo_models::{Registry, Value};
use xilo_proto::TimestampMode;
use xilo_store::{Config, History, Paths};

use crate::runtime::Runtime;
use crate::state::{AppState, PATHS};

actions!(xilo, [Quit, ShowWindow, CloseWindow]);

fn init_logging(paths: &Paths) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::prelude::*;
    std::fs::create_dir_all(&paths.logs).ok()?;
    let file = tracing_appender::rolling::daily(&paths.logs, "xilo.log");
    let (writer, guard) = tracing_appender::non_blocking(file);
    let filter = std::env::var("XILO_LOG").unwrap_or_else(|_| "info,btleplug=warn".into());
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(filter))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();
    Some(guard)
}

fn simulator(registry: &Registry) -> SimConnector {
    let sim = SimConnector::new();
    for (addr, code, serial) in [
        ("sim-c300x", "A1723", "ALFSBK1G00000001"),
        ("sim-c1000", "A1761", "APC9FQ0E00000001"),
        ("sim-c1000g2", "A1763", "APCDK00000000001"),
        ("sim-sb2", "A17C1", "APCGQ00000000001"),
    ] {
        if let Some(spec) = registry.get(code) {
            let mut device = SimDevice::new(spec.clone(), serial);
            if addr == "sim-c300x" {
                for (id, value) in [
                    ("battery_state", Value::Int(2)),
                    ("battery_soc", Value::Int(72)),
                    ("ac_switch", Value::Bool(true)),
                    ("ac_output_power", Value::Int(24)),
                    ("ac_input_power", Value::Int(118)),
                    ("input_source", Value::Int(2)),
                    ("total_input_power", Value::Int(118)),
                    ("total_output_power", Value::Int(41)),
                    ("usb_c3_state", Value::Int(1)),
                    ("usb_c3_power", Value::Int(17)),
                    ("remaining_time", Value::Float(1.3)),
                    ("light_mode", Value::Int(1)),
                ] {
                    device.set(id, value);
                }
            }
            sim.add(addr, device);
        }
    }
    sim
}

fn local_offset() -> i32 {
    jiff::Timestamp::now()
        .to_zoned_datetime(jiff::tz::TimeZone::system())
        .offset()
        .seconds()
}

fn build_runtime(paths: &Paths, registry: Arc<Registry>) -> anyhow::Result<Runtime> {
    let tokio = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("xilo-io")
        .enable_all()
        .build()?;
    let simulated = std::env::var_os("XILO_SIM").is_some();
    let mut bluetooth_error = None;
    let (connector, scanner): (Arc<dyn Connector>, Arc<dyn Scanner>) = if simulated {
        let sim = simulator(&registry);
        (Arc::new(sim.clone()), Arc::new(sim))
    } else {
        match tokio.block_on(xilo_ble::BleTransport::new()) {
            Ok(ble) => (ble.clone(), ble),
            Err(err) => {
                tracing::error!("bluetooth unavailable: {err}");
                bluetooth_error = Some(format!(
                    "{err}. Turn on Bluetooth and allow Xilo to use it, then restart Xilo."
                ));
                let none = SimConnector::new();
                (Arc::new(none.clone()), Arc::new(none))
            }
        }
    };
    let client_id = xilo_store::client_id(paths, xilo_engine::identity::new_client_id)?;
    let engine = Engine::new(
        connector,
        scanner,
        registry,
        EngineConfig {
            client_id,
            timestamps: TimestampMode::Utc,
            timezone: xilo_engine::identity::posix_timezone(local_offset()),
            country: "US".into(),
        },
    );
    Ok(Runtime {
        tokio,
        engine,
        bluetooth_error,
        simulated,
    })
}

fn open_history(paths: &Paths, days: u32) -> Option<History> {
    match History::open(&paths.history_file()) {
        Ok(h) => {
            let cutoff = services::unix_now() - i64::from(days) * 86_400;
            if let Err(err) = h.prune(cutoff) {
                tracing::warn!("could not prune history: {err}");
            }
            Some(h)
        }
        Err(err) => {
            tracing::error!("could not open history: {err}");
            None
        }
    }
}

pub fn open_main_window(state: Entity<AppState>, cx: &mut App) {
    if let Some(window) = cx.windows().first().copied() {
        let _ = window.update(cx, |_, window, _| window.activate_window());
        cx.activate(true);
        return;
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1200.), px(820.)),
            cx,
        ))),
        window_min_size: Some(size(px(800.), px(600.))),
        app_id: Some("dev.xilo.Xilo".into()),
        ..TitleBar::window_options()
    };
    if let Err(err) = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| shell::Root::new(state, window, cx))
    }) {
        tracing::error!("could not open the window: {err}");
    }
    cx.activate(true);
}

fn main() -> anyhow::Result<()> {
    let paths = Paths::detect();
    let _log = init_logging(&paths);
    let Some(wake) = instance::claim(&paths) else {
        tracing::info!("Xilo is already running");
        return Ok(());
    };
    let _ = PATHS.set(paths.clone());
    let config = Config::load(&paths.config_file()).unwrap_or_else(|err| {
        tracing::error!("settings could not be read, using defaults: {err}");
        Config::default()
    });
    let (registry, problems) = Registry::with_user_dir(&paths.models());
    for problem in problems {
        tracing::warn!("models folder: {problem}");
    }
    let registry = Arc::new(registry);
    let runtime = build_runtime(&paths, registry)?;
    let history = config
        .preferences
        .record_history
        .then(|| open_history(&paths, config.preferences.history_days))
        .flatten()
        .or_else(|| History::open(&paths.history_file()).ok());

    let app = gpui_kit::application().with_assets(assets::Assets);
    app.on_reopen(|cx| cx.dispatch_action(&ShowWindow));
    app.run(move |cx| {
        gpui_kit::init(cx);
        theme::init(cx, config.preferences.theme);
        cx.set_global(runtime);

        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let tray_mode = config.preferences.keep_running_in_tray;
        let state = cx.new(|cx| AppState::new(config, history, events_tx, cx));
        shell::listen(state.clone(), events_rx, cx);
        instance::listen(wake, state.clone(), cx);
        tray::init(state.clone(), cx);

        cx.on_action(|_: &Quit, cx| cx.quit());
        let show = state.clone();
        cx.on_action(move |_: &ShowWindow, cx| open_main_window(show.clone(), cx));
        cx.on_action(|_: &CloseWindow, cx| {
            if let Some(window) = cx.active_window() {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            }
        });
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-w", CloseWindow, None),
        ]);
        cx.set_menus(vec![Menu {
            name: "Xilo".into(),
            items: vec![
                MenuItem::action("Show Xilo", ShowWindow),
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWindow),
                MenuItem::action("Quit Xilo", Quit),
            ],
            disabled: false,
        }]);
        cx.set_quit_mode(if tray_mode {
            QuitMode::Explicit
        } else {
            QuitMode::LastWindowClosed
        });
        cx.on_app_quit(|cx| {
            let runtime = cx.global::<Runtime>();
            let engine = runtime.engine.clone();
            let done = Runtime::run(cx, async move {
                let _ = tokio::time::timeout(Duration::from_secs(2), engine.shutdown()).await;
            });
            async move {
                let _ = done.await;
            }
        })
        .detach();

        open_main_window(state, cx);
    });
    Ok(())
}
