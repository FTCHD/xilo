use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input as TextInput, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Placement, Sizable as _, WindowExt as _, h_flex,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_models::{CommandDef, CommandRequest, Confirmed, Danger, Group, Input, ModelSpec};

use crate::device_view::DeviceView;
use crate::widgets::{caption, card, section};

const MANUAL_URL: &str = "https://charging-tutorial.anker.com/navigation?app=anker_power&lang=en-US&country=US&hideNavBar=true&pn=";

pub fn render(
    view: &mut DeviceView,
    _window: &mut Window,
    cx: &mut Context<DeviceView>,
) -> AnyElement {
    let snap = view.device.read(cx).snap.clone();
    let connected = snap.is_ready();
    let Some(model) = snap.model.clone() else {
        return card(cx)
            .child(
                h_flex()
                    .gap(px(16.))
                    .justify_between()
                    .items_center()
                    .child(
                        caption(
                            "Xilo could not detect the model yet. Choose it to see its settings.",
                            cx,
                        )
                        .flex_1()
                        .min_w(px(0.)),
                    )
                    .child(
                        Button::new("choose-model")
                            .small()
                            .primary()
                            .label("Choose Model")
                            .on_click(
                                cx.listener(|view, _, window, cx| model_picker(view, window, cx)),
                            ),
                    ),
            )
            .into_any_element();
    };
    let mut rows = |group: Group, cx: &mut Context<DeviceView>| {
        let commands: Vec<CommandDef> = model
            .commands
            .values()
            .filter(|c| c.group == group && model.is_visible(c, &snap.fields))
            .cloned()
            .collect();
        let mut list = v_flex();
        for command in commands {
            list = list.child(command_row(
                &model,
                &command,
                &snap.fields,
                connected,
                view,
                cx,
            ));
        }
        list
    };
    let function = rows(Group::Function, cx);
    let general = rows(Group::General, cx);
    let key = view.device.read(cx).key.clone();
    let name = view.device.read(cx).name();
    let serial = snap.serial().unwrap_or("-").to_string();
    let firmware = snap.text("firmware").unwrap_or("-").to_string();
    let code = model.code.clone();

    v_flex()
        .gap(px(14.))
        .max_w(px(760.))
        .child(section("Function Settings", cx))
        .child(card(cx).p(px(4.)).child(function))
        .child(section("General Settings", cx))
        .child(
            card(cx).p(px(4.)).child(
                v_flex()
                    .child(
                        info_row("Device Name", name.clone(), true, cx)
                            .id("rename")
                            .on_click(
                                cx.listener(move |view, _, window, cx| rename(view, window, cx)),
                            ),
                    )
                    .child(general)
                    .child(info_row("SN", serial, false, cx))
                    .child(info_row("Firmware", firmware, false, cx))
                    .child(
                        info_row("User Manual", String::new(), true, cx)
                            .id("manual")
                            .on_click(move |_, _, cx| cx.open_url(&format!("{MANUAL_URL}{code}"))),
                    ),
            ),
        )
        .child(section("Connection", cx))
        .child(
            card(cx).p(px(4.)).child(
                v_flex()
                    .child(
                        info_row(
                            "Model",
                            format!("{} ({})", model.name, model.code),
                            true,
                            cx,
                        )
                        .id("model")
                        .on_click(
                            cx.listener(|view, _, window, cx| model_picker(view, window, cx)),
                        ),
                    )
                    .child(row(
                        "Release to phone",
                        Some(
                            "Disconnect for 30 minutes so the Anker app on your phone can connect.",
                        ),
                        Button::new("release")
                            .small()
                            .label("Release")
                            .on_click(
                                cx.listener(|view, _, _, cx| view.device.read(cx).release(cx)),
                            )
                            .into_any_element(),
                        cx,
                    ))
                    .child(row(
                        "Remove device",
                        Some("Xilo forgets this device. The device itself is not changed."),
                        Button::new("remove")
                            .small()
                            .danger()
                            .label("Remove")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                let key = key.clone();
                                let state = view.state.clone();
                                window.open_alert_dialog(cx, move |dialog, _, _| {
                                    let key = key.clone();
                                    let state = state.clone();
                                    dialog
                                        .title("Remove Device?")
                                        .description("The device will be removed from Xilo.")
                                        .ok_text("Remove")
                                        .ok_variant(
                                            gpui_kit::component::button::ButtonVariant::Danger,
                                        )
                                        .show_cancel(true)
                                        .on_ok(move |_, _, cx| {
                                            state.update(cx, |s, cx| s.remove_device(&key, cx));
                                            true
                                        })
                                });
                            }))
                            .into_any_element(),
                        cx,
                    )),
            ),
        )
        .into_any_element()
}

/// Lets the user pick the model when detection fails or is wrong.
fn model_picker(view: &mut DeviceView, window: &mut Window, cx: &mut Context<DeviceView>) {
    let registry = crate::runtime::Runtime::get(cx).engine.registry().clone();
    let mut models: Vec<(String, String)> = registry
        .all()
        .map(|m| (m.code.clone(), m.name.clone()))
        .collect();
    models.sort_by(|a, b| a.1.cmp(&b.1));
    let current = view
        .device
        .read(cx)
        .snap
        .model
        .as_ref()
        .map(|m| m.code.clone());
    let device = view.device.clone();
    let state = view.state.clone();
    window.open_sheet_at(Placement::Right, cx, move |sheet, _, cx| {
        let list = v_flex()
            .gap(px(2.))
            .children(models.iter().map(|(code, name)| {
                let selected = current.as_deref() == Some(code.as_str());
                let device = device.clone();
                let state = state.clone();
                let pick = code.clone();
                h_flex()
                    .id(SharedString::from(format!("model-{code}")))
                    .px(px(12.))
                    .py(px(8.))
                    .rounded(px(10.))
                    .gap(px(8.))
                    .justify_between()
                    .cursor_pointer()
                    .hover(|s| s.bg(cx.theme().muted))
                    .child(
                        v_flex()
                            .child(div().text_size(px(14.)).child(name.clone()))
                            .child(caption(code.clone(), cx)),
                    )
                    .when(selected, |el| {
                        el.child(Icon::new(IconName::Check).text_color(cx.theme().primary))
                    })
                    .on_click(move |_, window, cx| {
                        let handle = device.read(cx).handle.clone();
                        let key = device.read(cx).key.clone();
                        let code = pick.clone();
                        let chosen = Some(code.clone());
                        crate::runtime::Runtime::spawn(cx, async move {
                            handle.set_model(chosen).await
                        });
                        state.update(cx, |s, cx| {
                            if let Some(d) = s.device_config_mut(&key) {
                                d.model = Some(code);
                            }
                            s.save();
                            cx.notify();
                        });
                        window.close_sheet(cx);
                    })
            }));
        sheet.title("Choose Model").size(px(360.)).child(
            div()
                .id("model-list")
                .size_full()
                .overflow_y_scroll()
                .child(list),
        )
    });
}

fn rename(view: &mut DeviceView, window: &mut Window, cx: &mut Context<DeviceView>) {
    let current = view.device.read(cx).name();
    let input = cx.new(|cx| InputState::new(window, cx).default_value(current));
    let device = view.device.clone();
    let state = view.state.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let input = input.clone();
        let device = device.clone();
        let state = state.clone();
        dialog
            .title("Device Name")
            .w(px(380.))
            .child(TextInput::new(&input))
            .footer(
                h_flex()
                    .gap(px(8.))
                    .justify_end()
                    .child(
                        Button::new("cancel")
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(Button::new("save").primary().label("Save").on_click(
                        move |_, window, cx| {
                            let name = input.read(cx).value().trim().to_string();
                            if name.is_empty() || name.chars().count() > 50 {
                                return;
                            }
                            let handle = device.read(cx).handle.clone();
                            let key = device.read(cx).key.clone();
                            let value = Some(name.clone());
                            crate::runtime::Runtime::spawn(cx, async move {
                                handle.set_name(value).await
                            });
                            state.update(cx, |s, cx| {
                                if let Some(d) = s.device_config_mut(&key) {
                                    d.name = Some(name.clone());
                                }
                                s.save();
                                cx.notify();
                            });
                            window.close_dialog(cx);
                        },
                    )),
            )
    });
}

fn row(label: &str, hint: Option<&str>, control: AnyElement, cx: &App) -> Stateful<Div> {
    h_flex()
        .id(SharedString::from(format!("row-{label}")))
        .px(px(14.))
        .py(px(12.))
        .gap(px(16.))
        .justify_between()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(2.))
                .child(div().text_size(px(15.)).child(label.to_string()))
                .when_some(hint, |el, h| el.child(caption(h.to_string(), cx))),
        )
        .child(control)
}

fn info_row(label: &str, value: String, chevron: bool, cx: &App) -> Stateful<Div> {
    row(
        label,
        None,
        h_flex()
            .gap(px(6.))
            .items_center()
            .child(caption(value, cx))
            .when(chevron, |el| {
                el.child(Icon::new(IconName::ChevronRight).size_4())
            })
            .into_any_element(),
        cx,
    )
    .cursor_pointer()
}

fn command_row(
    model: &ModelSpec,
    command: &CommandDef,
    state: &xilo_models::FieldMap,
    connected: bool,
    view: &mut DeviceView,
    cx: &mut Context<DeviceView>,
) -> Stateful<Div> {
    let snap = view.device.read(cx).snap.clone();
    let unsupported = snap.unsupported.contains(&command.id);
    let gated = model.gate(command, state);
    let current = model.current(command, state);
    let enabled = connected && !view.device_busy(&command.id, cx);
    let hint = if unsupported {
        Some("Not supported by this firmware.".to_string())
    } else {
        gated.clone()
    };
    let control: AnyElement = match command.input {
        Input::Switch => {
            let on = current == Some(command.on_value);
            let c = command.clone();
            Switch::new(SharedString::from(format!("sw-{}", command.id)))
                .checked(on)
                .on_click(cx.listener(move |view, checked: &bool, window, cx| {
                    if !enabled {
                        return;
                    }
                    let value = if *checked { c.on_value } else { c.off_value };
                    request(view, &c, Some(value), window, cx);
                }))
                .into_any_element()
        }
        Input::Choice | Input::Number => {
            let text = current
                .map(|v| option_label(command, v))
                .unwrap_or_else(|| "-".into());
            let c = command.clone();
            h_flex()
                .id(SharedString::from(format!("pick-{}", command.id)))
                .gap(px(6.))
                .px(px(8.))
                .py(px(4.))
                .rounded(px(8.))
                .items_center()
                .cursor_pointer()
                .hover(|s| s.bg(cx.theme().muted))
                .child(div().text_size(px(14.)).child(text))
                .child(
                    Icon::new(IconName::ChevronRight)
                        .size_4()
                        .text_color(cx.theme().muted_foreground),
                )
                .on_click(cx.listener(move |view, _, window, cx| {
                    if enabled {
                        if let Some(reason) = view_gate(view, &c, cx) {
                            window.push_notification(reason, cx);
                        } else {
                            picker(view, &c, window, cx);
                        }
                    }
                }))
                .into_any_element()
        }
        Input::None => {
            let c = command.clone();
            let danger = command.danger == Danger::Destructive;
            Button::new(SharedString::from(format!("act-{}", command.id)))
                .small()
                .when(danger, |b| b.danger())
                .label(if danger { "Restore" } else { "Run" })
                .on_click(cx.listener(move |view, _, window, cx| {
                    if enabled {
                        request(view, &c, None, window, cx);
                    }
                }))
                .into_any_element()
        }
    };
    let label = command.label.clone();
    let tip = command.tip.clone();
    let mut row = row(&label, hint.as_deref(), control, cx);
    if let Some(tip) = tip {
        row = row.child(
            div()
                .id(SharedString::from(format!("tip-{}", command.id)))
                .cursor_pointer()
                .child(
                    Icon::new(IconName::Info)
                        .size_4()
                        .text_color(cx.theme().muted_foreground),
                )
                .on_click(move |_, window, cx| {
                    let tip = tip.clone();
                    let label = label.clone();
                    window.open_alert_dialog(cx, move |dialog, _, _| {
                        dialog
                            .title(label.clone())
                            .description(tip.clone())
                            .ok_text("Got It")
                    });
                }),
        );
    }
    row.opacity(if connected { 1.0 } else { 0.5 })
}

fn view_gate(view: &DeviceView, command: &CommandDef, cx: &App) -> Option<String> {
    let snap = &view.device.read(cx).snap;
    snap.model.as_ref()?.gate(command, &snap.fields)
}

fn option_label(command: &CommandDef, value: i64) -> String {
    command
        .options
        .iter()
        .find(|o| o.value == value)
        .map(|o| o.label.clone())
        .unwrap_or_else(|| match &command.unit {
            Some(unit) => format!("{value}{unit}"),
            None => value.to_string(),
        })
}

fn options(command: &CommandDef) -> Vec<(i64, String)> {
    if !command.options.is_empty() {
        return command
            .options
            .iter()
            .map(|o| (o.value, o.label.clone()))
            .collect();
    }
    let (min, max, step) = (
        command.min.unwrap_or(0),
        command.max.unwrap_or(0),
        command.step.unwrap_or(1).max(1),
    );
    (min..=max)
        .step_by(step as usize)
        .take(60)
        .map(|v| (v, option_label(command, v)))
        .collect()
}

fn picker(
    view: &mut DeviceView,
    command: &CommandDef,
    window: &mut Window,
    cx: &mut Context<DeviceView>,
) {
    let snap = view.device.read(cx).snap.clone();
    let current = snap
        .model
        .as_ref()
        .and_then(|m| m.current(command, &snap.fields));
    let entity = cx.entity();
    let command = command.clone();
    window.open_sheet_at(Placement::Right, cx, move |sheet, _, cx| {
        let list = v_flex()
            .gap(px(2.))
            .children(options(&command).into_iter().map(|(value, label)| {
                let entity = entity.clone();
                let command = command.clone();
                let selected = current == Some(value);
                h_flex()
                    .id(SharedString::from(format!("opt-{}-{value}", command.id)))
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(10.))
                    .justify_between()
                    .cursor_pointer()
                    .hover(|s| s.bg(cx.theme().muted))
                    .child(div().text_size(px(15.)).child(label))
                    .when(selected, |el| {
                        el.child(Icon::new(IconName::Check).text_color(cx.theme().primary))
                    })
                    .on_click(move |_, window, cx| {
                        window.close_sheet(cx);
                        entity.update(cx, |view, cx| {
                            request(view, &command, Some(value), window, cx)
                        });
                    })
            }));
        sheet
            .title(command.label.clone())
            .size(px(340.))
            .child(list)
    });
}

/// Sends a command, asking for confirmation first when it is dangerous.
fn request(
    view: &mut DeviceView,
    command: &CommandDef,
    value: Option<i64>,
    window: &mut Window,
    cx: &mut Context<DeviceView>,
) {
    let req = CommandRequest::new(command.id.clone(), value);
    match command.danger {
        Danger::None => view.send(req, window, cx),
        Danger::Confirm => {
            let entity = cx.entity();
            let text = command
                .danger_text
                .clone()
                .unwrap_or_else(|| "Are you sure?".into());
            let title = command.label.clone();
            window.open_alert_dialog(cx, move |dialog, _, _| {
                let entity = entity.clone();
                let req = req.clone();
                dialog
                    .title(title.clone())
                    .description(text.clone())
                    .show_cancel(true)
                    .ok_text("Change")
                    .on_ok(move |_, window, cx| {
                        let req = req.clone().confirmed(Confirmed::by_user());
                        entity.update(cx, |view, cx| view.send(req, window, cx));
                        true
                    })
            });
        }
        Danger::Destructive => {
            let entity = cx.entity();
            let text = command.danger_text.clone().unwrap_or_default();
            let title = command.label.clone();
            let understood = Rc::new(Cell::new(false));
            window.open_dialog(cx, move |dialog, _, cx| {
                let entity = entity.clone();
                let req = req.clone();
                let understood = understood.clone();
                let checked = understood.get();
                let toggle = understood.clone();
                dialog
                    .title(title.clone())
                    .w(px(420.))
                    .child(
                        v_flex()
                            .gap(px(12.))
                            .child(div().text_size(px(14.)).child(text.clone()))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(cx.theme().danger)
                                    .child("All settings on the device return to their defaults. This cannot be undone."),
                            )
                            .child(Checkbox::new("understand").label("I understand").checked(checked).on_click(
                                move |value, window, _| {
                                    toggle.set(*value);
                                    window.refresh();
                                },
                            )),
                    )
                    .footer(
                        h_flex()
                            .gap(px(8.))
                            .justify_end()
                            .child(Button::new("cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                            .child(
                                Button::new("confirm")
                                    .danger()
                                    .label("Restore Defaults")
                                    .disabled(!checked)
                                    .on_click(move |_, window, cx| {
                                        if !understood.get() {
                                            return;
                                        }
                                        let req = req.clone().confirmed(Confirmed::by_user());
                                        let line = format!("{} {}", now_text(), req.command);
                                        if let Some(paths) = crate::state::PATHS.get() {
                                            let _ = xilo_store::audit(paths, &line);
                                        }
                                        entity.update(cx, |view, cx| view.send(req, window, cx));
                                        window.close_dialog(cx);
                                    }),
                            ),
                    )
            });
        }
    }
}

fn now_text() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("unix {secs}")
}
