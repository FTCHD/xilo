use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input as TextInput, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_proto::{MsgId, Request, tlv};
use xilo_store::capture::{self, Header, Record};

use crate::device::DeviceModel;
use crate::runtime::Runtime;
use crate::widgets::{caption, card, section};

const MAX_ROWS: usize = 400;

struct Row {
    record: Record,
    fields: Vec<(String, bool)>,
}

pub struct Inspector {
    device: Entity<DeviceModel>,
    handle: xilo_engine::DeviceHandle,
    rows: VecDeque<Row>,
    last: HashMap<String, HashMap<u8, Vec<u8>>>,
    paused: bool,
    start: Instant,
    msg: Entity<InputState>,
    params: Entity<InputState>,
    _listen: Task<()>,
}

impl Drop for Inspector {
    fn drop(&mut self) {
        self.handle.stop_traces();
    }
}

fn item_text(item: &tlv::Item) -> String {
    let value = match (item.ty, item.uint()) {
        (Some(0x00), _) => format!("\"{}\"", item.text()),
        (_, Some(v)) if item.data.len() <= 4 => format!("{v}"),
        _ => hex::encode(&item.data),
    };
    format!("{:02x}={value}", item.key)
}

impl Inspector {
    pub fn new(device: Entity<DeviceModel>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let handle = device.read(cx).handle.clone();
        let mut rx = handle.traces();
        let stop = handle.clone();
        let listen = cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                let trace = match rx.recv().await {
                    Ok(t) => t,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                };
                if this.update(cx, |view, cx| view.push(&trace, cx)).is_err() {
                    break;
                }
            }
            stop.stop_traces();
        });
        let status = handle.clone();
        Runtime::spawn(cx, async move {
            let _ = status
                .send(xilo_models::CommandRequest::new("status", None))
                .await;
        });
        Self {
            device,
            handle,
            rows: VecDeque::new(),
            last: HashMap::new(),
            paused: false,
            start: Instant::now(),
            msg: cx.new(|cx| InputState::new(window, cx).placeholder("Message id")),
            params: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Parameters, for example a2=u8:1")
            }),
            _listen: listen,
        }
    }

    fn push(&mut self, trace: &xilo_proto::Trace, cx: &mut Context<Self>) {
        if self.paused {
            return;
        }
        let record = Record::from_trace(self.start.elapsed().as_millis() as u64, trace);
        let mut fields = Vec::new();
        if let Some(message) = record.message() {
            let cmd = record.cmd.clone().unwrap_or_default();
            let previous = self.last.entry(cmd).or_default();
            for item in &message.payload.items {
                let changed = previous.get(&item.key).is_some_and(|old| *old != item.data);
                fields.push((item_text(item), changed));
                previous.insert(item.key, item.data.clone());
            }
        }
        self.rows.push_front(Row { record, fields });
        self.rows.truncate(MAX_ROWS);
        cx.notify();
    }

    fn export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let snap = self.device.read(cx).snap.clone();
        let mut records: Vec<Record> = self.rows.iter().rev().map(|r| r.record.clone()).collect();
        let mut secrets: Vec<Vec<u8>> = Vec::new();
        if let Some(serial) = snap.serial() {
            secrets.push(serial.as_bytes().to_vec());
        }
        if let Some(mac) = snap.info.as_ref().and_then(|i| i.mac) {
            secrets.push(mac.to_vec());
        }
        for r in &mut records {
            capture::redact(r, &secrets);
        }
        let header = Header {
            xilo_capture: 1,
            model: snap.model.as_ref().map(|m| m.code.clone()),
            firmware: snap.text("firmware").map(str::to_string),
            redacted: true,
        };
        let dir = crate::state::PATHS
            .get()
            .map(|p| p.captures())
            .unwrap_or_else(xilo_store::download_dir);
        let _ = std::fs::create_dir_all(&dir);
        let name = format!(
            "{} capture.xcap",
            header.model.clone().unwrap_or_else(|| "device".into())
        );
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        let window = window.window_handle();
        cx.spawn(async move |_, cx: &mut AsyncApp| {
            let Ok(Ok(Some(path))) = rx.await else {
                return;
            };
            let result = std::fs::File::create(&path)
                .map_err(|e| e.to_string())
                .and_then(|mut f| {
                    capture::write(&mut f, &header, &records).map_err(|e| e.to_string())
                });
            let note = match result {
                Ok(()) => Notification::success(format!(
                    "Saved {} records with serial and MAC removed.",
                    records.len()
                )),
                Err(err) => Notification::error(format!("Could not save the capture: {err}")),
            };
            let _ = cx.update_window(window, |_, window, cx| window.push_notification(note, cx));
        })
        .detach();
    }

    fn send_raw(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let msg = self.msg.read(cx).value().trim().to_string();
        let params_text = self.params.read(cx).value().to_string();
        let Some(id) = MsgId::parse(&msg) else {
            window.push_notification(
                Notification::warning(format!("{msg} is not a message id.")),
                cx,
            );
            return;
        };
        let params: Result<Vec<_>, String> = params_text
            .split_whitespace()
            .map(tlv::parse_param)
            .collect();
        let params = match params {
            Ok(p) => p,
            Err(err) => {
                window.push_notification(Notification::warning(err), cx);
                return;
            }
        };
        let handle = self.device.read(cx).handle.clone();
        let confirm =
            cx.new(|cx| InputState::new(window, cx).placeholder(format!("Type {id} to send")));
        let expected = id.to_string();
        window.open_dialog(cx, move |dialog, _, cx| {
            let confirm = confirm.clone();
            let handle = handle.clone();
            let params = params.clone();
            let expected = expected.clone();
            dialog
                .title("Send a Raw Command?")
                .w(px(440.))
                .child(
                    v_flex()
                        .gap(px(10.))
                        .child(div().text_size(px(14.)).child(
                            "Raw commands skip all checks. The device accepts values outside the normal range. \
                             Unknown commands can change settings or reset the device.",
                        ))
                        .child(caption(format!("{expected} {}", params_text_of(&params)), cx))
                        .child(TextInput::new(&confirm)),
                )
                .footer(
                    h_flex()
                        .gap(px(8.))
                        .justify_end()
                        .child(Button::new("cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("send").danger().label("Send").on_click(move |_, window, cx| {
                            if confirm.read(cx).value().trim() != expected {
                                return;
                            }
                            let handle = handle.clone();
                            let request = Request {
                                id,
                                params: params.clone(),
                            };
                            let line = format!("raw {expected} {}", params_text_of(&request.params));
                            if let Some(paths) = crate::state::PATHS.get() {
                                let _ = xilo_store::audit(paths, &line);
                            }
                            let rx = Runtime::run(cx, async move { handle.send_raw(request).await });
                            let target = window.window_handle();
                            cx.spawn(async move |cx: &mut AsyncApp| {
                                let text = match rx.await {
                                    Ok(Ok(outcome)) => format!("{line}: {outcome:?}"),
                                    Ok(Err(err)) => format!("{line}: {err}"),
                                    Err(_) => format!("{line}: no result"),
                                };
                                let _ = cx.update_window(target, |_, window, cx| {
                                    window.push_notification(Notification::info(text), cx);
                                });
                            })
                            .detach();
                            window.close_dialog(cx);
                        })),
                )
        });
    }
}

fn params_text_of(params: &[(u8, tlv::Value)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{k:02x}={v:?}"))
        .collect::<Vec<_>>()
        .join(" ")
}

impl Render for Inspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let paused = self.paused;
        let toolbar = h_flex()
            .gap(px(8.))
            .child(
                Button::new("pause")
                    .small()
                    .label(if paused { "Resume" } else { "Pause" })
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.paused = !view.paused;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("clear")
                    .small()
                    .ghost()
                    .label("Clear")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.rows.clear();
                        view.last.clear();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("save")
                    .small()
                    .ghost()
                    .label("Save Capture")
                    .on_click(cx.listener(|view, _, window, cx| view.export(window, cx))),
            );
        let raw = card(cx).child(
            v_flex()
                .gap(px(8.))
                .child(caption(
                    "Raw commands are written to the audit log. Factory reset is blocked here.",
                    cx,
                ))
                .child(
                    h_flex()
                        .gap(px(8.))
                        .child(div().w(px(160.)).child(TextInput::new(&self.msg).small()))
                        .child(div().flex_1().child(TextInput::new(&self.params).small()))
                        .child(
                            Button::new("raw-send")
                                .small()
                                .danger()
                                .label("Send")
                                .on_click(
                                    cx.listener(|view, _, window, cx| view.send_raw(window, cx)),
                                ),
                        ),
                ),
        );
        let theme = cx.theme();
        let mono = theme.mono_font_family.clone();
        let log = v_flex()
            .font_family(mono)
            .text_size(px(12.))
            .children(self.rows.iter().map(|row| {
                let r = &row.record;
                let arrow = if r.outgoing { "→" } else { "←" };
                let plain = r.plain.clone().unwrap_or_else(|| r.raw.clone());
                v_flex()
                    .py(px(5.))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        h_flex()
                            .gap(px(10.))
                            .child(
                                div()
                                    .w(px(70.))
                                    .text_color(theme.muted_foreground)
                                    .child(format!("{:.1}s", r.ms as f64 / 1000.0)),
                            )
                            .child(div().w(px(16.)).child(arrow))
                            .child(
                                div()
                                    .w(px(44.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(r.cmd.clone().unwrap_or_default()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_color(theme.muted_foreground)
                                    .child(plain),
                            )
                            .when_some(r.note.clone(), |el, n| {
                                el.child(div().text_color(theme.warning).child(n))
                            }),
                    )
                    .when(!row.fields.is_empty(), |el| {
                        el.child(h_flex().pl(px(140.)).gap(px(10.)).flex_wrap().children(
                            row.fields.iter().map(|(text, changed)| {
                                div()
                                    .px(px(4.))
                                    .rounded(px(4.))
                                    .when(*changed, |d| d.bg(theme.warning.opacity(0.25)))
                                    .child(text.clone())
                            }),
                        ))
                    })
            }));
        v_flex()
            .gap(px(14.))
            .child(section("Raw Command", cx))
            .child(raw)
            .child(
                h_flex()
                    .justify_between()
                    .child(section("Frames", cx))
                    .child(toolbar),
            )
            .child(card(cx).child(if self.rows.is_empty() {
                caption(
                    "Frames appear here as the device sends them. Changed values are highlighted.",
                    cx,
                )
                .into_any_element()
            } else {
                log.into_any_element()
            }))
    }
}
