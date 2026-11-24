use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_engine::automations::usb_watts;
use xilo_store::UsbWake;

use crate::device_view::DeviceView;
use crate::history_view::time_text;
use crate::services::unix_now;
use crate::widgets::{caption, card, section};

const IDLE_CHOICES: [u32; 4] = [60, 80, 100, 110];
const BATTERY_CHOICES: [u8; 4] = [5, 10, 20, 30];

pub fn render(
    view: &mut DeviceView,
    _window: &mut Window,
    cx: &mut Context<DeviceView>,
) -> AnyElement {
    let device = view.device.read(cx);
    let key = device.key.clone();
    let snap = device.snap.clone();
    let has_dc = snap
        .model
        .as_ref()
        .is_some_and(|m| m.command("dc_switch").is_some());
    let wake = view
        .state
        .read(cx)
        .config
        .device(&key)
        .map(|d| d.automations.usb_wake.clone())
        .unwrap_or_default();
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

    let update = move |view: &mut DeviceView, wake: UsbWake, cx: &mut Context<DeviceView>| {
        let key = view.device.read(cx).key.clone();
        view.state.update(cx, |s, cx| {
            s.set_usb_wake(&key, wake);
            cx.notify();
        });
        cx.notify();
    };

    let choice_row = |label: &'static str,
                      values: Vec<(i64, String)>,
                      current: i64,
                      id: &'static str,
                      cx: &mut Context<DeviceView>,
                      apply: fn(&mut UsbWake, i64)| {
        let base = wake.clone();
        h_flex()
            .px(px(14.))
            .py(px(12.))
            .justify_between()
            .items_center()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().text_size(px(15.)).child(label))
            .child(
                h_flex()
                    .gap(px(6.))
                    .children(values.into_iter().map(|(value, text)| {
                        let mut next = base.clone();
                        apply(&mut next, value);
                        Button::new(SharedString::from(format!("{id}-{value}")))
                            .small()
                            .when(value == current, |b| b.primary())
                            .when(value != current, |b| b.ghost())
                            .label(text)
                            .on_click(
                                cx.listener(move |view, _, _, cx| update(view, next.clone(), cx)),
                            )
                    })),
            )
    };

    let toggle = {
        let mut next = wake.clone();
        next.enabled = !wake.enabled;
        Switch::new("usb-wake")
            .checked(wake.enabled)
            .on_click(cx.listener(move |view, _: &bool, _, cx| update(view, next.clone(), cx)))
    };

    let idle_row = choice_row(
        "Wake after idle",
        IDLE_CHOICES
            .iter()
            .map(|m| (i64::from(*m), format!("{m} min")))
            .collect(),
        i64::from(wake.idle_minutes),
        "idle",
        cx,
        |w, v| w.idle_minutes = v as u32,
    );
    let battery_row = choice_row(
        "Skip below battery",
        BATTERY_CHOICES
            .iter()
            .map(|b| (i64::from(*b), format!("{b}%")))
            .collect(),
        i64::from(wake.min_battery),
        "battery",
        cx,
        |w, v| w.min_battery = v as u8,
    );

    let status = if !wake.enabled {
        "Off".to_string()
    } else if !snap.is_ready() {
        "Waiting for the device to connect.".to_string()
    } else {
        format!("Watching. USB load now: {:.0} W.", usb_watts(&snap))
    };

    let log = v_flex().children(events.iter().take(30).map(|e| {
        h_flex()
            .py(px(6.))
            .gap(px(12.))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(caption(time_text(e.ts), cx).w(px(130.)))
            .child(div().text_size(px(14.)).child(e.text.clone()))
    }));

    v_flex()
        .gap(px(14.))
        .max_w(px(760.))
        .child(section("USB Wake", cx))
        .child(
            card(cx).p(px(4.)).child(
                v_flex()
                    .child(
                        h_flex()
                            .px(px(14.))
                            .py(px(12.))
                            .gap(px(16.))
                            .justify_between()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .gap(px(4.))
                                    .child(div().text_size(px(15.)).child("Keep USB ports on"))
                                    .child(caption(
                                        "The device turns its USB ports off after about two hours of low load. \
                                         Xilo turns the car output on briefly before that happens, which keeps the ports on. \
                                         If the car output was off, Xilo turns it off again 3 seconds later.",
                                        cx,
                                    ))
                                    .child(caption(status, cx)),
                            )
                            .child(toggle),
                    )
                    .when(wake.enabled, |el| el.child(idle_row).child(battery_row)),
            ),
        )
        .when(!has_dc, |el| {
            el.child(caption("This model has no car output command, so USB wake cannot run.", cx))
        })
        .child(section("Activity", cx))
        .child(card(cx).child(if events.is_empty() {
            caption("Nothing yet. Alerts and automation actions from the last 7 days appear here.", cx).into_any_element()
        } else {
            log.into_any_element()
        }))
        .into_any_element()
}
