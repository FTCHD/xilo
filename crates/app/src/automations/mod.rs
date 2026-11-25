mod reserve;
mod schedules;
mod usb_wake;

use gpui_kit::assets::IconName;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_engine::Snapshot;
use xilo_models::{CommandDef, Danger, Group, Input, ModelSpec};
use xilo_store::Automations;

pub use schedules::{Editor, add as add_schedule};

use crate::device_view::DeviceView;
use crate::history_view::time_text;
use crate::services::unix_now;
use crate::widgets::{caption, card, divided, icon_badge, pill, section, segment, segmented};

pub fn render(
    view: &mut DeviceView,
    window: &mut Window,
    cx: &mut Context<DeviceView>,
) -> AnyElement {
    let device = view.device.read(cx);
    let key = device.key.clone();
    let snap = device.snap.clone();
    let automations = view.state.read(cx).automations(&key);
    let events = view
        .state
        .read(cx)
        .history
        .as_ref()
        .and_then(|h| {
            h.lock()
                .ok()?
                .events(&key, unix_now() - 7 * 86_400, i64::MAX)
                .ok()
        })
        .unwrap_or_default();
    let has_dc = snap
        .model
        .as_ref()
        .is_some_and(|m| m.command("dc_switch").is_some());

    let log = divided(
        events.iter().take(30).map(|e| {
            h_flex()
                .px(px(14.))
                .py(px(10.))
                .gap(px(12.))
                .child(caption(time_text(e.ts), cx).w(px(130.)).flex_none())
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(14.))
                        .child(e.text.clone()),
                )
                .into_any_element()
        }),
        cx,
    );

    v_flex()
        .gap(px(14.))
        .child(schedules::render(view, &snap, &automations, window, cx))
        .child(section("Battery Reserve", cx))
        .child(reserve::render(&snap, &automations, cx))
        .when(has_dc, |el| {
            el.child(section("USB Wake", cx))
                .child(usb_wake::render(&snap, &automations, cx))
        })
        .child(section("Activity", cx))
        .child(card(cx).p(px(4.)).child(if events.is_empty() {
            caption(
                "Nothing yet. Alerts and automation actions from the last 7 days appear here.",
                cx,
            )
            .p(px(14.))
            .into_any_element()
        } else {
            log.into_any_element()
        }))
        .into_any_element()
}

fn save(view: &mut DeviceView, automations: Automations, cx: &mut Context<DeviceView>) {
    let key = view.device.read(cx).key.clone();
    view.state.update(cx, |s, cx| {
        s.set_automations(&key, automations);
        cx.notify();
    });
    cx.notify();
}

/// The dashboard switches and choices that automations can use.
fn controls(model: &ModelSpec, snap: &Snapshot) -> Vec<CommandDef> {
    model
        .commands
        .values()
        .filter(|c| c.group == Group::Control && c.danger == Danger::None)
        .filter(|c| c.input == Input::Switch || (c.input == Input::Choice && !c.options.is_empty()))
        .filter(|c| model.is_visible(c, &snap.fields))
        .cloned()
        .collect()
}

/// The top row of an automation card: icon, title, status, text and switch.
struct Header {
    id: &'static str,
    icon: IconName,
    color: Hsla,
    title: &'static str,
    status: (String, Hsla),
    text: &'static str,
    enabled: bool,
}

fn header(
    h: Header,
    base: &Automations,
    toggle: fn(&mut Automations),
    cx: &mut Context<DeviceView>,
) -> AnyElement {
    let mut next = base.clone();
    toggle(&mut next);
    h_flex()
        .px(px(14.))
        .py(px(14.))
        .gap(px(16.))
        .items_center()
        .child(icon_badge(
            h.icon,
            if h.enabled {
                h.color
            } else {
                cx.theme().muted_foreground
            },
            px(44.),
        ))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(6.))
                .child(
                    h_flex()
                        .gap(px(10.))
                        .items_center()
                        .child(
                            div()
                                .text_size(px(16.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(h.title),
                        )
                        .child(pill(h.status.0, h.status.1)),
                )
                .child(caption(h.text, cx)),
        )
        .child(
            Switch::new(h.id)
                .checked(h.enabled)
                .on_click(cx.listener(move |view, _: &bool, _, cx| save(view, next.clone(), cx))),
        )
        .into_any_element()
}

fn choice_row<T: Copy + PartialEq + 'static>(
    label: &'static str,
    id: &'static str,
    items: Vec<(T, String)>,
    current: T,
    base: &Automations,
    apply: fn(&mut Automations, T),
    cx: &mut Context<DeviceView>,
) -> AnyElement {
    h_flex()
        .px(px(14.))
        .py(px(12.))
        .gap(px(12.))
        .justify_between()
        .items_center()
        .child(div().text_size(px(15.)).child(label))
        .child(
            segmented(cx).children(items.into_iter().enumerate().map(|(i, (value, text))| {
                let mut next = base.clone();
                apply(&mut next, value);
                segment(
                    SharedString::from(format!("{id}-{i}")),
                    text,
                    value == current,
                    cx,
                )
                .on_click(cx.listener(move |view, _, _, cx| save(view, next.clone(), cx)))
            })),
        )
        .into_any_element()
}
