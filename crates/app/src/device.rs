use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::*;
use xilo_engine::{DeviceHandle, Outcome, SendError, Snapshot};
use xilo_models::CommandRequest;

use crate::runtime::Runtime;

const CURVE_POINTS: usize = 12;

pub struct DeviceModel {
    pub key: String,
    pub handle: DeviceHandle,
    pub snap: Arc<Snapshot>,
    pub ac_curve: VecDeque<f64>,
    pub car_curve: VecDeque<f64>,
    pub busy: Vec<String>,
    _watch: Task<()>,
    _sampler: Task<()>,
}

pub enum DeviceEvent {
    Changed,
}

impl EventEmitter<DeviceEvent> for DeviceModel {}

impl DeviceModel {
    pub fn new(handle: DeviceHandle, cx: &mut Context<Self>) -> Self {
        let mut rx = handle.watch();
        let watch = cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                let snap = rx.borrow_and_update().clone();
                let alive = this.update(cx, |model, cx| {
                    model.snap = snap;
                    cx.emit(DeviceEvent::Changed);
                    cx.notify();
                });
                if alive.is_err() || rx.changed().await.is_err() {
                    break;
                }
            }
        });
        let sampler = cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let alive = this.update(cx, |model, cx| {
                    let ac = model.snap.float("ac_output_power").unwrap_or(0.0);
                    let car = model.snap.float("car_output_power").unwrap_or(0.0);
                    push(&mut model.ac_curve, ac);
                    push(&mut model.car_curve, car);
                    if model.snap.is_ready() {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            key: handle.key().to_string(),
            snap: handle.snapshot(),
            handle,
            ac_curve: VecDeque::new(),
            car_curve: VecDeque::new(),
            busy: Vec::new(),
            _watch: watch,
            _sampler: sampler,
        }
    }

    pub fn name(&self) -> String {
        self.snap.display_name()
    }

    /// Sends a command and reports failures as a notification in `window`.
    pub fn send(&mut self, request: CommandRequest, window: &mut Window, cx: &mut Context<Self>) {
        let label = request.command.clone();
        self.busy.push(label.clone());
        cx.notify();
        let handle = self.handle.clone();
        let rx = Runtime::run(cx, async move { handle.send(request).await });
        let window = window.window_handle();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err(SendError::Closed));
            let _ = this.update(cx, |model, cx| {
                model.busy.retain(|b| b != &label);
                cx.notify();
            });
            let message = match result {
                Ok(Outcome::Acked) => None,
                Ok(Outcome::Rejected(status)) => {
                    Some(format!("The device refused: {}.", status.description()))
                }
                Ok(Outcome::NoAck) => {
                    Some("The device did not answer. This command may not be supported.".into())
                }
                Err(err) => Some(err.to_string()),
            };
            if let Some(message) = message {
                let _ = cx.update_window(window, |_, window, cx| {
                    window.push_notification(Notification::warning(message), cx);
                });
            }
        })
        .detach();
    }

    pub fn is_busy(&self, command: &str) -> bool {
        self.busy.iter().any(|b| b == command)
    }

    pub fn connect(&self, cx: &App) {
        let handle = self.handle.clone();
        Runtime::spawn(cx, async move { handle.connect().await });
    }

    pub fn release(&self, cx: &App) {
        let handle = self.handle.clone();
        Runtime::spawn(cx, async move {
            handle.release(Duration::from_secs(30 * 60)).await
        });
    }
}

fn push(curve: &mut VecDeque<f64>, value: f64) {
    curve.push_back(value);
    while curve.len() > CURVE_POINTS {
        curve.pop_front();
    }
}
