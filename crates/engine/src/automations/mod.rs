mod reserve;
mod schedule;
mod usb_wake;

use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use xilo_models::CommandRequest;

pub use reserve::{Reserve, ReserveConfig};
pub use schedule::{Action, ScheduleRule, Scheduler, Step};
pub use usb_wake::{Pulse, UsbWake, UsbWakeConfig, usb_watts};

use crate::{DeviceHandle, Outcome};

const RESTORE_DELAY: Duration = Duration::from_secs(3);
const TICK: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AutomationConfig {
    pub usb_wake: UsbWakeConfig,
    pub reserve: ReserveConfig,
    pub schedules: Vec<ScheduleRule>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationEvent {
    pub device: String,
    pub text: String,
    pub failed: bool,
}

async fn send(handle: &DeviceHandle, command: &str, value: i64) -> Result<(), String> {
    match handle.send(CommandRequest::new(command, Some(value))).await {
        Ok(Outcome::Acked) => Ok(()),
        Ok(Outcome::Rejected(_)) => Err("the device refused it".into()),
        Ok(Outcome::NoAck) => Err("the device did not answer".into()),
        Err(err) => Err(err.to_string()),
    }
}

/// Runs the automations of one device until the handle closes.
pub fn spawn(
    handle: DeviceHandle,
    mut config: watch::Receiver<AutomationConfig>,
    events: mpsc::UnboundedSender<AutomationEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let initial = config.borrow().clone();
        let mut wake = UsbWake::new(initial.usb_wake);
        let mut reserve = Reserve::new(initial.reserve);
        let mut scheduler = Scheduler::new(initial.schedules);
        let mut snaps = handle.watch();
        let mut tick = tokio::time::interval(TICK);
        let report = |text: String, failed: bool| {
            let _ = events.send(AutomationEvent {
                device: handle.key().to_string(),
                text,
                failed,
            });
        };
        loop {
            tokio::select! {
                changed = snaps.changed() => if changed.is_err() { break },
                changed = config.changed() => {
                    if changed.is_err() { break }
                    let next = config.borrow().clone();
                    wake.config = next.usb_wake;
                    reserve.set_config(next.reserve);
                    scheduler.set_rules(next.schedules);
                }
                _ = tick.tick() => {}
            }
            let snap = snaps.borrow().clone();
            let model = snap.model.as_deref();

            let ready = snap.is_ready() && model.is_some();
            for step in scheduler.tick(local_now(), ready) {
                match step {
                    Step::Run(rule) => {
                        let action = Action::new(model, &rule.command, rule.value);
                        if reserve.blocks(&rule.command, rule.value, &snap) {
                            report(
                                format!(
                                    "Schedule skipped: the battery is at the reserve level, so {} stays off.",
                                    action.label
                                ),
                                true,
                            );
                            continue;
                        }
                        match send(&handle, &rule.command, rule.value).await {
                            Ok(()) => report(format!("Schedule {}.", action.sentence()), false),
                            Err(why) => report(
                                format!("Schedule could not {}: {why}.", action.sentence()),
                                true,
                            ),
                        }
                    }
                    Step::Missed(rule) => {
                        let action = Action::new(model, &rule.command, rule.value);
                        report(
                            format!(
                                "Schedule missed: the device was out of reach, so Xilo could not {}.",
                                action.sentence()
                            ),
                            true,
                        );
                    }
                }
            }

            let off = reserve.observe(&snap);
            if !off.is_empty() {
                let soc = snap.int("battery_soc").unwrap_or_default();
                let mut done = Vec::new();
                for command in off {
                    let value = model
                        .and_then(|m| m.command(&command))
                        .map_or(0, |c| c.off_value);
                    let label = Action::new(model, &command, value).label;
                    match send(&handle, &command, value).await {
                        Ok(()) => done.push(label),
                        Err(why) => report(
                            format!("Battery reserve could not turn {label} off: {why}."),
                            true,
                        ),
                    }
                }
                if !done.is_empty() {
                    report(
                        format!(
                            "Battery reserve: the battery is at {soc}%, so Xilo turned {} off.",
                            done.join(" and ")
                        ),
                        false,
                    );
                }
            }

            if let Some(pulse) = wake.observe(Instant::now(), &snap) {
                match send(&handle, "dc_switch", 1).await {
                    Ok(()) => {
                        if pulse.restore_off {
                            tokio::time::sleep(RESTORE_DELAY).await;
                            let _ = send(&handle, "dc_switch", 0).await;
                        }
                        report("USB wake: turned the ports back on.".into(), false);
                    }
                    Err(why) => report(format!("USB wake did not work: {why}."), true),
                }
            }
        }
    })
}

fn local_now() -> jiff::civil::DateTime {
    jiff::Timestamp::now()
        .to_zoned_datetime(jiff::tz::TimeZone::system())
        .datetime()
}
