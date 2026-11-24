use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogFooter, DialogHeader, DialogTitle};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_models::CommandRequest;

use crate::device_view::DeviceView;
use crate::families::view_model::{self, Dashboard};
use crate::widgets::{self, asset, battery_gauge, caption, card, section, sparkline, watts};

const SCALE: f32 = 1.4;

pub fn render(
    view: &mut DeviceView,
    window: &mut Window,
    cx: &mut Context<DeviceView>,
) -> AnyElement {
    let device = view.device.read(cx);
    let snap = device.snap.clone();
    let assets = snap
        .model
        .as_ref()
        .map(|m| m.assets.clone())
        .unwrap_or_else(|| "a1723".into());
    let c1000 = snap.model.as_ref().is_some_and(|m| m.family.id == "c1000");
    let d = if c1000 {
        view_model::build_c1000(&snap, &assets, view.blink_on)
    } else {
        view_model::build(&snap, &assets, view.blink_on)
    };
    let ac_curve = device.ac_curve.clone();
    let car_curve = device.car_curve.clone();
    let ac_on = snap.flag("ac_switch");
    let car_on = snap.flag("dc_switch");
    let light = snap.int("light_mode").unwrap_or(0);
    let screen_on = snap.flag("screen_on");
    let ac_timer = snap.int("ac_timer_remaining").unwrap_or(0);
    let car_timer = snap.int("dc_timer_remaining").unwrap_or(0);
    let _ = window;

    v_flex()
        .gap(px(16.))
        .when_some(d.error.clone(), |el, (_, text)| {
            el.child(error_banner(text, cx))
        })
        .child(
            h_flex()
                .gap(px(20.))
                .items_start()
                .child(hero(&d, cx))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(420.))
                        .gap(px(16.))
                        .child(
                            h_flex()
                                .gap(px(12.))
                                .child(total("Total Input", d.total_in, cx))
                                .child(total("Total Output", d.total_out, cx)),
                        )
                        .child(
                            h_flex()
                                .gap(px(6.))
                                .items_center()
                                .child(section("Port Status", cx))
                                .child(
                                    div()
                                        .id("port-legend")
                                        .cursor_pointer()
                                        .child(asset("shared/tips", px(18.), IconName::Info, cx))
                                        .on_click(cx.listener(|_, _, window, cx| {
                                            port_legend(window, cx);
                                        })),
                                ),
                        )
                        .child(ports(&d, cx))
                        .child(
                            h_flex()
                                .gap(px(12.))
                                .items_start()
                                .child(output_card(
                                    "car",
                                    "Car Charger Output",
                                    snap.int("car_output_power"),
                                    car_on,
                                    car_timer,
                                    &car_curve,
                                    "dc_switch",
                                    "dc_timer",
                                    view.device_busy("dc_switch", cx),
                                    d.connected,
                                    cx,
                                ))
                                .child(output_card(
                                    "ac",
                                    "AC Output",
                                    snap.int("ac_output_power"),
                                    ac_on,
                                    ac_timer,
                                    &ac_curve,
                                    "ac_switch",
                                    "ac_timer",
                                    view.device_busy("ac_switch", cx),
                                    d.connected,
                                    cx,
                                )),
                        )
                        .child(section("Control", cx))
                        .child(
                            h_flex()
                                .gap(px(12.))
                                .items_start()
                                .child(brightness(light, d.connected, cx))
                                .child(screen(screen_on, d.connected, cx)),
                        ),
                ),
        )
        .into_any_element()
}

fn error_banner(text: String, cx: &App) -> Div {
    card(cx).border_1().border_color(cx.theme().danger).child(
        h_flex()
            .gap(px(10.))
            .items_center()
            .child(asset("shared/error", px(22.), IconName::TriangleAlert, cx))
            .child(div().text_size(px(14.)).child(text)),
    )
}

fn hero(d: &Dashboard, cx: &App) -> Div {
    let w = 240.0 * SCALE;
    let h = 238.0 * SCALE;
    let battery_w = 100.0 * SCALE;
    let center = v_flex()
        .absolute()
        .top(px(68.0 * SCALE))
        .left(px((w - battery_w) / 2.0))
        .w(px(battery_w))
        .h(px(106.0 * SCALE))
        .items_center()
        .justify_center()
        .text_color(white())
        .map(|el| {
            if !d.connected || d.loading {
                el.child(
                    div()
                        .text_size(px(30.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("--%"),
                )
            } else if d.overload {
                el.child(asset(
                    "shared/overload",
                    px(40.),
                    IconName::TriangleAlert,
                    cx,
                ))
            } else {
                let big = if d.healthy_temp {
                    d.soc.map(|s| format!("{s}")).unwrap_or_else(|| "--".into())
                } else {
                    d.temperature.clone().unwrap_or_default()
                };
                el.when(d.charging && d.healthy_temp, |el| {
                    el.child(asset("shared/charging", px(22.), IconName::Zap, cx))
                })
                .child(
                    h_flex()
                        .items_baseline()
                        .child(
                            div()
                                .text_size(px(40.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(big),
                        )
                        .when(d.healthy_temp, |el| {
                            el.child(div().text_size(px(20.)).child("%"))
                        }),
                )
                .when(d.healthy_temp, |el| {
                    el.child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(d.temperature.clone().unwrap_or_default()),
                    )
                })
            }
        });
    let stage = div()
        .relative()
        .w(px(w))
        .h(px(h))
        .child(asset(&d.hero, px(w), IconName::BatteryCharging, cx))
        .when(d.connected && !d.loading && d.gauge, |el| {
            el.child(
                div()
                    .absolute()
                    .top(px(68.0 * SCALE))
                    .left(px((w - battery_w) / 2.0))
                    .child(battery_gauge(d.battery.key(), d.level, SCALE)),
            )
        })
        .child(center)
        .when(
            d.connected && (d.charging || d.total_out.unwrap_or(0) > 0),
            |el| {
                let key = if d.level < 0.06 {
                    "shared/bottom_charging_red"
                } else {
                    "shared/bottom_charging"
                };
                el.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left(px((w - 112.0 * SCALE) / 2.0))
                        .child(asset(key, px(112.0 * SCALE), IconName::Zap, cx)),
                )
            },
        );
    let background = crate::assets::vendor("shared/home_bg");
    v_flex()
        .w(px(w + 36.))
        .rounded(px(24.))
        .overflow_hidden()
        .bg(rgb(0xf3f7fb))
        .relative()
        .when_some(background, |el, bg| {
            el.child(
                img(bg)
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(w + 36.))
                    .h(px(h + 120.))
                    .rounded(px(24.))
                    .object_fit(ObjectFit::Cover),
            )
        })
        .p(px(18.))
        .gap(px(10.))
        .items_center()
        .child(stage)
        .child(
            div()
                .text_size(px(14.))
                .text_color(rgb(0x000f16).opacity(0.7))
                .child(d.status.clone()),
        )
}

fn total(label: &'static str, value: Option<i64>, cx: &App) -> Div {
    card(cx).flex_1().child(
        v_flex()
            .gap(px(6.))
            .child(caption(label, cx))
            .child(watts(value, cx)),
    )
}

fn ports(d: &Dashboard, cx: &App) -> Div {
    let tiles: Vec<Div> = d
        .ports
        .iter()
        .map(|p| widgets::port_tile(p.label, p.icon.asset(), p.watts, cx))
        .collect();
    let mut rows = v_flex().gap(px(10.));
    let mut iter = tiles.into_iter();
    loop {
        let row: Vec<Div> = iter.by_ref().take(3).collect();
        if row.is_empty() {
            break;
        }
        rows = rows.child(h_flex().gap(px(10.)).children(row));
    }
    rows
}

#[allow(clippy::too_many_arguments)]
fn output_card(
    id: &'static str,
    label: &'static str,
    power: Option<i64>,
    on: bool,
    timer_secs: i64,
    curve: &std::collections::VecDeque<f64>,
    switch_cmd: &'static str,
    timer_cmd: &'static str,
    busy: bool,
    connected: bool,
    cx: &mut Context<DeviceView>,
) -> Div {
    let brand = cx.theme().primary;
    let toggle_key = if on {
        "shared/device_on"
    } else {
        "shared/device_off"
    };
    let countdown = if timer_secs > 0 {
        let (h, rest) = (timer_secs / 3600, timer_secs % 3600);
        h_flex()
            .gap(px(4.))
            .child(asset("shared/countdown_on", px(16.), IconName::Timer, cx))
            .child(
                div()
                    .text_size(px(12.))
                    .child(format!("{h}:{:02}:{:02}", rest / 60, rest % 60)),
            )
            .into_any_element()
    } else {
        asset("shared/countdown_off", px(20.), IconName::Timer, cx)
    };
    card(cx)
        .flex_1()
        .opacity(if connected { 1.0 } else { 0.5 })
        .child(
            v_flex()
                .gap(px(10.))
                .child(caption(label, cx))
                .child(watts(power.filter(|_| on), cx))
                .child(sparkline(curve, brand, px(56.)))
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(div().id(SharedString::from(format!("{id}-timer"))).when(
                            on && connected,
                            |el| {
                                el.cursor_pointer()
                                    .px(px(10.))
                                    .py(px(4.))
                                    .rounded(px(14.))
                                    .border_1()
                                    .border_color(rgb(0x1cb5e6))
                                    .child(countdown)
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.countdown_sheet(label, timer_cmd, window, cx);
                                    }))
                            },
                        ))
                        .child(
                            div()
                                .id(SharedString::from(format!("{id}-toggle")))
                                .when(connected && !busy, |el| {
                                    el.cursor_pointer().on_click(cx.listener(
                                        move |view, _, window, cx| {
                                            view.send(
                                                CommandRequest::new(
                                                    switch_cmd,
                                                    Some(i64::from(!on)),
                                                ),
                                                window,
                                                cx,
                                            );
                                        },
                                    ))
                                })
                                .opacity(if busy { 0.5 } else { 1.0 })
                                .child(asset(toggle_key, px(30.), IconName::Power, cx)),
                        ),
                ),
        )
}

fn brightness(light: i64, connected: bool, cx: &mut Context<DeviceView>) -> Div {
    let labels = ["Off", "Low", "Mid", "High"];
    let keys = [
        "brightness_off",
        "brightness_low",
        "brightness_middle",
        "brightness_high",
    ];
    let current = if light == 4 { -1 } else { light };
    let active = cx.theme().primary;
    let label = match light {
        4 => "SOS",
        l => labels.get(l as usize).copied().unwrap_or("Off"),
    };
    card(cx)
        .flex_1()
        .opacity(if connected { 1.0 } else { 0.5 })
        .child(
            v_flex()
                .gap(px(12.))
                .child(
                    h_flex()
                        .justify_between()
                        .child(caption("Brightness", cx))
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if light > 0 {
                                    active
                                } else {
                                    cx.theme().muted_foreground
                                })
                                .child(label),
                        ),
                )
                .child(h_flex().gap(px(8.)).children((0..4).map(|i| {
                    let selected = i == current;
                    let key = format!(
                        "shared/{}_{}",
                        keys[i as usize],
                        if selected { "sel" } else { "nor" }
                    );
                    div()
                        .id(SharedString::from(format!("light-{i}")))
                        .flex_1()
                        .flex()
                        .justify_center()
                        .py(px(8.))
                        .rounded(px(12.))
                        .when(selected, |el| el.bg(cx.theme().accent))
                        .when(connected, |el| {
                            el.cursor_pointer()
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.send(
                                        CommandRequest::new("light_mode", Some(i)),
                                        window,
                                        cx,
                                    );
                                }))
                        })
                        .child(asset(&key, px(26.), IconName::Lightbulb, cx))
                }))),
        )
}

fn screen(on: bool, connected: bool, cx: &mut Context<DeviceView>) -> Div {
    let key = if on {
        "shared/device_on"
    } else {
        "shared/device_off"
    };
    card(cx)
        .w(px(180.))
        .opacity(if connected { 1.0 } else { 0.5 })
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(caption("Screen", cx))
                .child(
                    div()
                        .id("screen-toggle")
                        .when(connected, |el| {
                            el.cursor_pointer()
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.send(
                                        CommandRequest::new("screen_switch", Some(i64::from(!on))),
                                        window,
                                        cx,
                                    );
                                }))
                        })
                        .child(asset(key, px(30.), IconName::Monitor, cx)),
                ),
        )
}

fn port_legend(window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, |dialog, _, cx| {
        let row = |key: &str, color: &str, status: &str, cx: &App| {
            h_flex()
                .gap(px(12.))
                .items_center()
                .child(asset(key, px(32.), IconName::Usb, cx))
                .child(
                    v_flex()
                        .child(
                            div()
                                .text_size(px(14.))
                                .child(format!("Port Color: {color}")),
                        )
                        .child(caption(format!("Port Status: {status}"), cx)),
                )
        };
        dialog
            .w(px(360.))
            .child(DialogHeader::new().child(DialogTitle::new().child("Port Status")))
            .child(
                v_flex()
                    .gap(px(14.))
                    .py(px(8.))
                    .child(row("ports/usb_c_input", "Green", "Input", cx))
                    .child(row("ports/usb_c_output", "Blue", "Output", cx)),
            )
            .child(
                DialogFooter::new().child(
                    Button::new("ok")
                        .primary()
                        .small()
                        .label("OK")
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                ),
            )
    });
}
