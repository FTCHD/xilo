use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogFooter, DialogHeader, DialogTitle};
use std::time::Duration;

use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_models::CommandRequest;

use crate::device_view::DeviceView;
use crate::families::view_model::{self, Dashboard};
use crate::widgets::{
    self, asset, battery_gauge, big_value, caption, glow, icon_badge, input_color, live_card,
    output_color, pill, power_button, section, sparkline,
};

const SCALE: f32 = 1.4;

pub fn render(
    view: &mut DeviceView,
    _window: &mut Window,
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
    let remaining = snap.float("remaining_time");

    v_flex()
        .gap(px(18.))
        .when_some(d.error.clone(), |el, (_, text)| {
            el.child(error_banner(text, cx))
        })
        .child(
            h_flex()
                .gap(px(18.))
                .items_start()
                .child(hero(&d, remaining, cx))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(440.))
                        .gap(px(14.))
                        .child(
                            h_flex()
                                .gap(px(14.))
                                .child(total("Total Input", d.total_in, true, cx))
                                .child(total("Total Output", d.total_out, false, cx)),
                        )
                        .child(
                            h_flex()
                                .gap(px(6.))
                                .items_center()
                                .child(section("Ports", cx))
                                .child(
                                    div()
                                        .id("port-legend")
                                        .pt(px(4.))
                                        .cursor_pointer()
                                        .child(
                                            Icon::new(IconName::Info)
                                                .size(px(13.))
                                                .text_color(cx.theme().muted_foreground),
                                        )
                                        .on_click(cx.listener(|_, _, window, cx| {
                                            port_legend(window, cx);
                                        })),
                                ),
                        )
                        .child(ports(&d, cx))
                        .child(section("Outputs", cx))
                        .child(
                            h_flex()
                                .gap(px(14.))
                                .items_start()
                                .child(output_card(
                                    Output {
                                        id: "car",
                                        label: "Car Charger",
                                        icon: IconName::Car,
                                        power: snap.int("car_output_power"),
                                        on: car_on,
                                        timer_secs: car_timer,
                                        curve: &car_curve,
                                        switch_cmd: "dc_switch",
                                        timer_cmd: "dc_timer",
                                        busy: view.device_busy("dc_switch", cx),
                                        connected: d.connected,
                                    },
                                    cx,
                                ))
                                .child(output_card(
                                    Output {
                                        id: "ac",
                                        label: "AC Output",
                                        icon: IconName::PlugZap,
                                        power: snap.int("ac_output_power"),
                                        on: ac_on,
                                        timer_secs: ac_timer,
                                        curve: &ac_curve,
                                        switch_cmd: "ac_switch",
                                        timer_cmd: "ac_timer",
                                        busy: view.device_busy("ac_switch", cx),
                                        connected: d.connected,
                                    },
                                    cx,
                                )),
                        )
                        .child(section("Controls", cx))
                        .child(
                            h_flex()
                                .gap(px(14.))
                                .items_start()
                                .child(brightness(light, d.connected, cx))
                                .child(screen(screen_on, d.connected, cx)),
                        ),
                ),
        )
        .into_any_element()
}

fn error_banner(text: String, cx: &App) -> Div {
    let color = cx.theme().danger;
    live_card(true, color, cx).p(px(14.)).child(
        h_flex()
            .gap(px(12.))
            .items_center()
            .child(icon_badge(IconName::TriangleAlert, color, px(36.)))
            .child(div().flex_1().text_size(px(14.)).child(text)),
    )
}

fn hero(d: &Dashboard, remaining_hours: Option<f64>, cx: &App) -> Div {
    let w = 240.0 * SCALE;
    let h = 238.0 * SCALE;
    let battery_w = 100.0 * SCALE;
    let dark = cx.theme().is_dark();
    let hot = d.hero.contains("_red");
    let accent = if hot {
        cx.theme().danger
    } else {
        cx.theme().primary
    };
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
                        .text_color(white().alpha(0.6))
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
                                .text_size(px(44.))
                                .font_weight(FontWeight::BOLD)
                                .child(big),
                        )
                        .when(d.healthy_temp, |el| {
                            el.child(
                                div()
                                    .text_size(px(20.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("%"),
                            )
                        }),
                )
                .when(d.healthy_temp, |el| {
                    el.child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .opacity(0.9)
                            .child(d.temperature.clone().unwrap_or_default()),
                    )
                })
            }
        });
    let glow_size = px(230.);
    let halo = div()
        .absolute()
        .top(px(h / 2.0 - 115.0))
        .left(px((w - 230.0) / 2.0))
        .size(glow_size)
        .rounded_full()
        .bg(accent.alpha(if dark { 0.05 } else { 0.04 }))
        .shadow(vec![BoxShadow {
            color: accent.alpha(if dark { 0.42 } else { 0.25 }),
            offset: point(px(0.), px(0.)),
            blur_radius: px(150.),
            spread_radius: px(-10.),
            inset: false,
        }]);
    let halo: AnyElement = if d.connected && d.charging {
        halo.with_animation(
            "hero-glow",
            Animation::new(Duration::from_millis(2400))
                .repeat()
                .with_easing(pulsating_between(0.45, 1.0)),
            |el, delta| el.opacity(delta),
        )
        .into_any_element()
    } else {
        halo.when(!d.connected, |el| el.opacity(0.3))
            .into_any_element()
    };
    let stage = div()
        .relative()
        .w(px(w))
        .h(px(h))
        .child(halo)
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
        .child(center);

    let (top, bottom) = if dark {
        (rgb(0x123241), rgb(0x0b141b))
    } else {
        (rgb(0xdff5fb), rgb(0xffffff))
    };
    let time = remaining_hours
        .filter(|_| d.connected)
        .map(|hrs| view_model::duration_text((hrs * 60.0).round() as i64));
    let chip = |icon: IconName, text: String, color: Hsla| {
        h_flex()
            .gap(px(6.))
            .items_center()
            .px(px(12.))
            .py(px(6.))
            .rounded_full()
            .bg(color.alpha(0.14))
            .border_1()
            .border_color(color.alpha(0.25))
            .child(Icon::new(icon).size(px(14.)).text_color(color))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color)
                    .child(text),
            )
    };
    v_flex()
        .w(px(w + 40.))
        .flex_none()
        .rounded(px(28.))
        .overflow_hidden()
        .bg(linear_gradient(
            180.,
            linear_color_stop(top, 0.),
            linear_color_stop(bottom, 1.),
        ))
        .border_1()
        .border_color(accent.alpha(if dark { 0.18 } else { 0.25 }))
        .shadow(glow(accent, if dark { 0.5 } else { 0.25 }))
        .p(px(20.))
        .gap(px(14.))
        .items_center()
        .child(stage)
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground.alpha(0.8))
                .child(d.status.clone()),
        )
        .when(d.connected && !d.loading, |el| {
            el.child(
                h_flex()
                    .gap(px(8.))
                    .flex_wrap()
                    .justify_center()
                    .child(chip(
                        IconName::ArrowDownToLine,
                        format!("{} W", d.total_in.unwrap_or(0)),
                        input_color(cx),
                    ))
                    .child(chip(
                        IconName::ArrowUpFromLine,
                        format!("{} W", d.total_out.unwrap_or(0)),
                        output_color(cx),
                    ))
                    .when_some(time, |el, t| {
                        el.child(chip(IconName::Clock, t, cx.theme().foreground.alpha(0.75)))
                    }),
            )
        })
}

fn total(label: &'static str, value: Option<i64>, input: bool, cx: &App) -> Div {
    let color = if input {
        input_color(cx)
    } else {
        output_color(cx)
    };
    let icon = if input {
        IconName::ArrowDownToLine
    } else {
        IconName::ArrowUpFromLine
    };
    let active = value.unwrap_or(0) > 0;
    live_card(active, color, cx).flex_1().child(
        h_flex()
            .gap(px(14.))
            .items_center()
            .child(icon_badge(icon, color, px(44.)))
            .child(
                v_flex()
                    .gap(px(2.))
                    .child(caption(label, cx))
                    .child(big_value(
                        value.filter(|v| *v > 0),
                        "W",
                        30.,
                        active.then_some(color),
                        cx,
                    )),
            ),
    )
}

fn ports(d: &Dashboard, cx: &App) -> Div {
    let tiles: Vec<Div> = d
        .ports
        .iter()
        .map(|p| {
            let active = p.icon.is_on() || p.watts.unwrap_or(0) > 0;
            widgets::port_tile(
                p.label,
                p.icon.asset(),
                p.watts,
                active,
                p.icon.is_input(),
                cx,
            )
        })
        .collect();
    let mut rows = v_flex().gap(px(12.));
    let mut iter = tiles.into_iter();
    loop {
        let row: Vec<Div> = iter.by_ref().take(3).collect();
        if row.is_empty() {
            break;
        }
        rows = rows.child(h_flex().gap(px(12.)).children(row));
    }
    rows
}

struct Output<'a> {
    id: &'static str,
    label: &'static str,
    icon: IconName,
    power: Option<i64>,
    on: bool,
    timer_secs: i64,
    curve: &'a std::collections::VecDeque<f64>,
    switch_cmd: &'static str,
    timer_cmd: &'static str,
    busy: bool,
    connected: bool,
}

fn output_card(o: Output<'_>, cx: &mut Context<DeviceView>) -> Div {
    let color = output_color(cx);
    let Output {
        id,
        label,
        on,
        switch_cmd,
        timer_cmd,
        busy,
        connected,
        ..
    } = o;
    let timer_text = if o.timer_secs > 0 {
        let (h, rest) = (o.timer_secs / 3600, o.timer_secs % 3600);
        format!("{h}:{:02}:{:02}", rest / 60, rest % 60)
    } else {
        "Timer".to_string()
    };
    let timer_color = if o.timer_secs > 0 {
        color
    } else {
        cx.theme().muted_foreground
    };
    live_card(on && connected, color, cx)
        .flex_1()
        .opacity(if connected { 1.0 } else { 0.55 })
        .child(
            v_flex()
                .gap(px(12.))
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            h_flex()
                                .gap(px(10.))
                                .items_center()
                                .child(icon_badge(
                                    o.icon,
                                    if on {
                                        color
                                    } else {
                                        cx.theme().muted_foreground
                                    },
                                    px(36.),
                                ))
                                .child(
                                    div()
                                        .text_size(px(15.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(label),
                                ),
                        )
                        .child(
                            power_button(
                                SharedString::from(format!("{id}-toggle")),
                                on,
                                connected && !busy,
                                cx,
                            )
                            .when(connected && !busy, |el| {
                                el.on_click(cx.listener(move |view, _, window, cx| {
                                    view.send(
                                        CommandRequest::new(switch_cmd, Some(i64::from(!on))),
                                        window,
                                        cx,
                                    );
                                }))
                            }),
                        ),
                )
                .child(big_value(
                    o.power.filter(|v| on && *v > 0),
                    "W",
                    30.,
                    on.then_some(color),
                    cx,
                ))
                .child(sparkline(
                    o.curve,
                    if on {
                        color
                    } else {
                        cx.theme().muted_foreground
                    },
                    px(54.),
                ))
                .child(
                    h_flex()
                        .id(SharedString::from(format!("{id}-timer")))
                        .gap(px(6.))
                        .items_center()
                        .self_start()
                        .px(px(10.))
                        .py(px(5.))
                        .rounded_full()
                        .bg(timer_color.alpha(0.12))
                        .child(
                            Icon::new(IconName::Timer)
                                .size(px(14.))
                                .text_color(timer_color),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(timer_color)
                                .child(timer_text),
                        )
                        .when(on && connected, |el| {
                            el.cursor_pointer()
                                .hover(|s| s.opacity(0.8))
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.countdown_sheet(label, timer_cmd, window, cx);
                                }))
                        })
                        .when(!(on && connected), |el| el.opacity(0.5)),
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
    let color = cx.theme().primary;
    let label = match light {
        4 => "SOS",
        l => labels.get(l as usize).copied().unwrap_or("Off"),
    };
    live_card(light > 0 && connected, color, cx)
        .flex_1()
        .opacity(if connected { 1.0 } else { 0.55 })
        .child(
            v_flex()
                .gap(px(14.))
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            h_flex()
                                .gap(px(10.))
                                .items_center()
                                .child(icon_badge(
                                    IconName::Lightbulb,
                                    if light > 0 {
                                        color
                                    } else {
                                        cx.theme().muted_foreground
                                    },
                                    px(36.),
                                ))
                                .child(
                                    div()
                                        .text_size(px(15.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Light"),
                                ),
                        )
                        .child(pill(
                            label,
                            if light > 0 {
                                color
                            } else {
                                cx.theme().muted_foreground
                            },
                        )),
                )
                .child(
                    h_flex()
                        .p(px(4.))
                        .gap(px(4.))
                        .rounded(px(14.))
                        .bg(cx.theme().muted)
                        .children((0..4).map(|i| {
                            let selected = i == current;
                            let key = format!(
                                "shared/{}_{}",
                                keys[i as usize],
                                if selected { "sel" } else { "nor" }
                            );
                            v_flex()
                                .id(SharedString::from(format!("light-{i}")))
                                .flex_1()
                                .items_center()
                                .gap(px(2.))
                                .py(px(8.))
                                .rounded(px(10.))
                                .when(selected, |el| {
                                    el.bg(cx.theme().group_box).shadow(glow(color, 0.35))
                                })
                                .when(connected && !selected, |el| {
                                    el.hover(|s| s.bg(cx.theme().group_box.alpha(0.5)))
                                })
                                .when(connected, |el| {
                                    el.cursor_pointer().on_click(cx.listener(
                                        move |view, _, window, cx| {
                                            view.send(
                                                CommandRequest::new("light_mode", Some(i)),
                                                window,
                                                cx,
                                            );
                                        },
                                    ))
                                })
                                .child(asset(&key, px(24.), IconName::Lightbulb, cx))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(if selected {
                                            color
                                        } else {
                                            cx.theme().muted_foreground
                                        })
                                        .child(labels[i as usize]),
                                )
                        })),
                ),
        )
}

fn screen(on: bool, connected: bool, cx: &mut Context<DeviceView>) -> Div {
    let color = cx.theme().primary;
    live_card(on && connected, color, cx)
        .w(px(200.))
        .flex_none()
        .opacity(if connected { 1.0 } else { 0.55 })
        .child(
            v_flex()
                .gap(px(14.))
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(icon_badge(
                            IconName::Monitor,
                            if on {
                                color
                            } else {
                                cx.theme().muted_foreground
                            },
                            px(36.),
                        ))
                        .child(power_button("screen-toggle", on, connected, cx).when(
                            connected,
                            |el| {
                                el.on_click(cx.listener(move |view, _, window, cx| {
                                    view.send(
                                        CommandRequest::new("screen_switch", Some(i64::from(!on))),
                                        window,
                                        cx,
                                    );
                                }))
                            },
                        )),
                )
                .child(
                    v_flex()
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Screen"),
                        )
                        .child(caption(if on { "On" } else { "Off" }, cx)),
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
