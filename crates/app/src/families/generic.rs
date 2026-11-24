use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_models::{CommandRequest, Group, Input};

use crate::device_view::DeviceView;
use crate::families::view_model::temperature_text;
use crate::widgets::{
    asset, battery_color, big_value, caption, card, divided, icon_badge, input_color, level_bar,
    live_card, output_color, power_button, section,
};

pub fn render(
    view: &mut DeviceView,
    _window: &mut Window,
    cx: &mut Context<DeviceView>,
) -> AnyElement {
    let snap = view.device.read(cx).snap.clone();
    let developer = view.state.read(cx).config.preferences.developer_mode;
    let Some(model) = snap.model.clone() else {
        return card(cx)
            .child(caption(
                "Xilo is waiting for the device to answer so it can detect the model.",
                cx,
            ))
            .into_any_element();
    };
    let connected = snap.is_ready();
    let soc = snap.int("battery_soc");
    let fahrenheit = snap.int("temperature_unit") == Some(1);
    let controls: Vec<_> = model
        .commands
        .values()
        .filter(|c| c.group == Group::Control && c.input == Input::Switch)
        .filter(|c| model.is_visible(c, &snap.fields))
        .cloned()
        .collect();

    let temperature = snap
        .int("temperature")
        .map(|t| temperature_text(t, fahrenheit));
    let battery = card(cx).w(px(280.)).flex_none().child(
        h_flex()
            .gap(px(14.))
            .items_center()
            .child(
                div()
                    .size(px(64.))
                    .flex_none()
                    .rounded(px(18.))
                    .bg(linear_gradient(
                        160.,
                        linear_color_stop(cx.theme().primary.alpha(0.22), 0.),
                        linear_color_stop(cx.theme().primary.alpha(0.04), 1.),
                    ))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(asset(
                        &format!("{}/product", model.assets),
                        px(52.),
                        IconName::BatteryCharging,
                        cx,
                    )),
            )
            .child(
                v_flex()
                    .flex_1()
                    .gap(px(6.))
                    .child(big_value(soc, "%", 32., None, cx))
                    .when_some(soc, |el, s| {
                        el.child(level_bar(
                            s as f32 / 100.0,
                            battery_color(s, cx),
                            px(6.),
                            cx,
                        ))
                    })
                    .when_some(temperature, |el, t| {
                        el.child(
                            h_flex()
                                .gap(px(4.))
                                .items_center()
                                .child(
                                    Icon::new(IconName::Thermometer)
                                        .size(px(13.))
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .child(caption(t, cx)),
                        )
                    }),
            ),
    );
    let total = |label: &'static str, value: Option<i64>, input: bool, cx: &App| {
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
        live_card(active, color, cx)
            .flex_1()
            .flex()
            .items_center()
            .child(
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
    };
    let summary = h_flex()
        .gap(px(14.))
        .items_stretch()
        .child(battery)
        .child(total(
            "Total Input",
            snap.int("total_input_power"),
            true,
            cx,
        ))
        .child(total(
            "Total Output",
            snap.int("total_output_power"),
            false,
            cx,
        ));

    let has_controls = !controls.is_empty();
    let control_cards = div()
        .grid()
        .grid_cols(3)
        .gap(px(14.))
        .children(controls.into_iter().map(|c| {
            let on = model.current(&c, &snap.fields) == Some(c.on_value);
            let id = c.id.clone();
            let value = if on { c.off_value } else { c.on_value };
            let color = output_color(cx);
            live_card(on && connected, color, cx)
                .min_w(px(0.))
                .p(px(16.))
                .child(
                    h_flex()
                        .gap(px(12.))
                        .items_center()
                        .child(icon_badge(
                            control_icon(&c.id),
                            if on {
                                color
                            } else {
                                cx.theme().muted_foreground
                            },
                            px(36.),
                        ))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w(px(0.))
                                .child(
                                    div()
                                        .text_size(px(15.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(c.label.clone()),
                                )
                                .child(caption(if on { "On" } else { "Off" }, cx)),
                        )
                        .child(
                            power_button(
                                SharedString::from(format!("ctl-{id}")),
                                on,
                                connected,
                                cx,
                            )
                            .when(connected, |el| {
                                el.on_click(cx.listener(move |view, _, window, cx| {
                                    view.send(
                                        CommandRequest::new(id.clone(), Some(value)),
                                        window,
                                        cx,
                                    );
                                }))
                            }),
                        ),
                )
        }));

    let mut seen = std::collections::HashSet::new();
    let rows: Vec<_> = model
        .fields
        .iter()
        .filter(|f| seen.insert(f.id.clone()))
        .filter(|f| developer || !f.hidden)
        .filter_map(|f| {
            snap.fields
                .get(&f.id)
                .map(|v| (f.clone(), model.format(&f.id, v)))
        })
        .collect();
    let values = card(cx).p(px(4.)).child(divided(
        rows.into_iter().map(|(field, text)| {
            let badge = match field.confidence {
                xilo_models::schema::Confidence::Verified
                | xilo_models::schema::Confidence::Likely => None,
                xilo_models::schema::Confidence::Unknown => Some("unknown"),
            };
            h_flex()
                .px(px(14.))
                .py(px(10.))
                .justify_between()
                .child(
                    h_flex()
                        .gap(px(8.))
                        .child(div().text_size(px(14.)).child(field.label.clone()))
                        .when_some(badge, |el, b| {
                            el.child(
                                div()
                                    .px(px(6.))
                                    .rounded(px(6.))
                                    .bg(cx.theme().muted)
                                    .text_size(px(11.))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(b),
                            )
                        }),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(text),
                )
                .into_any_element()
        }),
        cx,
    ));

    v_flex()
        .gap(px(16.))
        .child(summary)
        .when(has_controls, |el| {
            el.child(section("Controls", cx)).child(control_cards)
        })
        .child(section("All Values", cx))
        .child(values)
        .into_any_element()
}

fn control_icon(id: &str) -> IconName {
    match id {
        "ac_switch" => IconName::PlugZap,
        "dc_switch" => IconName::Car,
        "screen_switch" => IconName::Monitor,
        "light_mode" => IconName::Lightbulb,
        "ultrafast" => IconName::Zap,
        id if id.starts_with("usb") => IconName::Usb,
        _ => IconName::Power,
    }
}
