use std::sync::{Arc, Mutex};
use std::time::Duration;

use xilo_models::{CommandError, CommandRequest, Registry, Value};
use xilo_proto::{MsgId, TimestampMode};

use crate::automations::{self, AutomationConfig, ReserveConfig, UsbWakeConfig};
use crate::sim::{Drift, SimConnector, SimDevice, UsbIdle};
use crate::{
    DeviceAddr, DeviceHandle, DeviceSetup, Engine, EngineConfig, LinkState, Outcome, SendError,
    Snapshot,
};

fn engine(code: &str, serial: &str) -> (Arc<Engine>, DeviceHandle, Arc<Mutex<SimDevice>>) {
    let registry = Arc::new(Registry::builtin().clone());
    let sim = SimConnector::new();
    let spec = registry.get(code).expect("model").clone();
    let device = sim.add("sim-1", SimDevice::new(spec, serial));
    let engine = Engine::new(
        Arc::new(sim.clone()),
        Arc::new(sim),
        registry,
        EngineConfig {
            client_id: crate::identity::new_client_id(),
            timestamps: TimestampMode::Utc,
            timezone: "UTC0".into(),
            country: "US".into(),
        },
    );
    let handle = engine.add(DeviceSetup {
        key: "dev".into(),
        addr: DeviceAddr::new("sim-1"),
        name: None,
        model: None,
        auto_connect: true,
    });
    (engine, handle, device)
}

async fn wait_for(handle: &DeviceHandle, what: impl Fn(&Snapshot) -> bool) -> Arc<Snapshot> {
    let mut rx = handle.watch();
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let snap = rx.borrow_and_update().clone();
            if what(&snap) {
                return snap;
            }
            if rx.changed().await.is_err() {
                return snap;
            }
        }
    })
    .await
    .expect("condition reached")
}

fn received(device: &Arc<Mutex<SimDevice>>, id: u16) -> usize {
    device
        .lock()
        .expect("lock")
        .received
        .iter()
        .filter(|m| **m == MsgId(id))
        .count()
}

#[tokio::test(start_paused = true)]
async fn connects_detects_and_reads_state() {
    let (_engine, handle, _device) = engine("A1723", "ALFSBK1G33300077");
    let snap = wait_for(&handle, |s| s.is_ready() && s.int("battery_soc").is_some()).await;
    assert_eq!(snap.model.as_ref().map(|m| m.code.as_str()), Some("A1723"));
    assert_eq!(snap.int("battery_soc"), Some(80));
    assert_eq!(snap.text("serial_number"), Some("ALFSBK1G33300077"));
    assert_eq!(snap.info.as_ref().map(|i| i.mtu), Some(253));
}

#[tokio::test(start_paused = true)]
async fn sends_commands_and_validates() {
    let (_engine, handle, device) = engine("A1723", "ALFSBK1G33300077");
    wait_for(&handle, |s| s.is_ready() && s.int("battery_soc").is_some()).await;

    let outcome = handle.send(CommandRequest::new("dc_switch", Some(1))).await;
    assert_eq!(outcome, Ok(Outcome::Acked));
    assert!(handle.snapshot().flag("dc_switch"));
    assert_eq!(
        device.lock().expect("lock").state.get("dc_switch"),
        Some(&Value::Bool(true))
    );

    let err = handle
        .send(CommandRequest::new("ac_charge_power", Some(250)))
        .await;
    assert!(matches!(
        err,
        Err(SendError::Command(CommandError::InvalidValue { .. }))
    ));

    let err = handle
        .send(CommandRequest::new("factory_reset", None))
        .await;
    assert_eq!(
        err,
        Err(SendError::Command(CommandError::NeedsConfirmation))
    );
}

#[tokio::test(start_paused = true)]
async fn missing_acks_mark_commands_unsupported() {
    let (_engine, handle, device) = engine("A1723", "ALFSBK1G33300077");
    device.lock().expect("lock").ignore.insert(0x0052);
    wait_for(&handle, |s| s.is_ready() && s.int("battery_soc").is_some()).await;
    for _ in 0..2 {
        let outcome = handle
            .send(CommandRequest::new("screen_switch", Some(0)))
            .await;
        assert_eq!(outcome, Ok(Outcome::NoAck));
    }
    assert!(handle.snapshot().unsupported.contains("screen_switch"));
    assert_eq!(received(&device, 0x0052), 4, "each send is retried once");
}

#[tokio::test(start_paused = true)]
async fn keeps_the_session_alive() {
    let (_engine, handle, device) = engine("A1723", "ALFSBK1G33300077");
    wait_for(&handle, |s| s.is_ready()).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(received(&device, 0x0057), 1);
    assert_eq!(received(&device, 0x0040), 1);
    tokio::time::sleep(Duration::from_secs(61)).await;
    assert_eq!(received(&device, 0x0057), 2);
    assert_eq!(received(&device, 0x0040), 2);
}

#[tokio::test(start_paused = true)]
async fn controls_every_model() {
    let (_engine, handle, _device) = engine("A1761", "APC9FE0E27300275");
    let snap = wait_for(&handle, |s| s.is_ready() && s.model.is_some()).await;
    assert_eq!(snap.model.as_ref().map(|m| m.code.as_str()), Some("A1761"));
    let outcome = handle.send(CommandRequest::new("dc_switch", Some(1))).await;
    assert_eq!(outcome, Ok(Outcome::Acked));
}

#[tokio::test(start_paused = true)]
async fn release_and_reconnect() {
    let (_engine, handle, _device) = engine("A1723", "ALFSBK1G33300077");
    wait_for(&handle, |s| s.is_ready()).await;
    handle.release(Duration::from_secs(600)).await;
    wait_for(&handle, |s| s.link == LinkState::Released).await;
    let err = handle.send(CommandRequest::new("dc_switch", Some(1))).await;
    assert_eq!(err, Err(SendError::NotConnected));
    handle.connect().await;
    wait_for(&handle, |s| s.is_ready()).await;
}

#[tokio::test(start_paused = true)]
async fn dc_on_revives_dead_usb_ports() {
    let (_engine, handle, device) = engine("A1723", "ALFSBK1G33300077");
    device.lock().expect("lock").usb_idle = Some(UsbIdle {
        after: Duration::from_secs(7200),
    });
    wait_for(&handle, |s| s.is_ready()).await;
    tokio::time::sleep(Duration::from_secs(7300)).await;
    assert!(!device.lock().expect("lock").draw_usb(30), "ports are dead");
    handle
        .send(CommandRequest::new("dc_switch", Some(1)))
        .await
        .expect("dc on");
    assert!(device.lock().expect("lock").draw_usb(30), "ports revived");
}

#[tokio::test(start_paused = true)]
async fn connects_with_legacy_aes() {
    let (_engine, handle, device) = engine("A1723", "ALFSBK1G33300077");
    device.lock().expect("lock").legacy = true;
    let snap = wait_for(&handle, |s| s.is_ready() && s.int("battery_soc").is_some()).await;
    assert_eq!(snap.int("battery_soc"), Some(80));
    assert_eq!(snap.info.as_ref().map(|i| i.capability), Some(0x04));
    let outcome = handle
        .send(CommandRequest::new("light_mode", Some(2)))
        .await
        .expect("sent");
    assert_eq!(outcome, Outcome::Acked);
}

#[tokio::test(start_paused = true)]
async fn usb_wake_leaves_dc_off_after_the_reserve() {
    let (_engine, handle, device) = engine("A1723", "ALFSBK1G33300077");
    {
        let mut d = device.lock().expect("lock");
        d.set("battery_soc", Value::Int(15));
        d.set("dc_switch", Value::Bool(true));
    }
    wait_for(&handle, |s| s.is_ready() && s.updated.is_some()).await;
    let (_config, rx) = tokio::sync::watch::channel(AutomationConfig {
        usb_wake: UsbWakeConfig {
            enabled: true,
            ..Default::default()
        },
        reserve: ReserveConfig {
            enabled: true,
            level: 20,
            outputs: vec!["dc_switch".into()],
        },
        schedules: Vec::new(),
    });
    let (events, mut log) = tokio::sync::mpsc::unbounded_channel();
    let _task = automations::spawn(handle.clone(), rx, events);
    tokio::time::sleep(Duration::from_secs(60)).await;

    let mut texts = Vec::new();
    while let Ok(event) = log.try_recv() {
        texts.push(event.text);
    }
    assert!(
        texts.iter().any(|t| t.starts_with("Battery reserve")),
        "{texts:?}"
    );
    assert_eq!(
        device.lock().expect("lock").state.get("dc_switch"),
        Some(&Value::Bool(false)),
        "{texts:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn simulated_readings_follow_their_switch() {
    let registry = Registry::builtin();
    let mut device = SimDevice::new(
        registry.get("A1723").expect("model").clone(),
        "ALFSBK1G33300077",
    );
    device.set("ac_switch", Value::Bool(true));
    device.drift = vec![Drift::new("ac_output_power", 26, 6.).when("ac_switch")];
    let sim = SimConnector::new();
    sim.add("sim-1", device);
    let engine = Engine::new(
        Arc::new(sim.clone()),
        Arc::new(sim),
        Arc::new(registry.clone()),
        EngineConfig {
            client_id: crate::identity::new_client_id(),
            timestamps: TimestampMode::Utc,
            timezone: "UTC0".into(),
            country: "US".into(),
        },
    );
    let handle = engine.add(DeviceSetup {
        key: "dev".into(),
        addr: DeviceAddr::new("sim-1"),
        name: None,
        model: None,
        auto_connect: true,
    });
    wait_for(&handle, |s| s.int("ac_output_power").is_some_and(|w| w > 0)).await;
    handle
        .send(CommandRequest::new("ac_switch", Some(0)))
        .await
        .expect("ac off");
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(handle.snapshot().int("ac_output_power"), Some(0));
}
