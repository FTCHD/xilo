use gpui_kit::assets::IconName;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_models::{CommandRequest, Group, Input};

use crate::device_view::DeviceView;
use crate::families::view_model::temperature_text;
use crate::widgets::{asset, caption, card, section, watts};

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

    let summary = h_flex()
        .gap(px(12.))
        .child(
            card(cx).w(px(220.)).child(
                h_flex()
                    .gap(px(12.))
                    .items_center()
                    .child(asset(
                        &format!("{}/product", model.assets),
                        px(64.),
                        IconName::BatteryCharging,
                        cx,
                    ))
                    .child(
                        v_flex()
                            .child(
                                div()
                                    .text_size(px(34.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(
                                        soc.map(|s| format!("{s}%"))
                                            .unwrap_or_else(|| "--%".into()),
                                    ),
                            )
                            .child(caption(
                                snap.int("temperature")
                                    .map(|t| temperature_text(t, fahrenheit))
                                    .unwrap_or_default(),
                                cx,
                            )),
                    ),
            ),
        )
        .child(
            card(cx).flex_1().child(
                v_flex()
                    .gap(px(6.))
                    .child(caption("Total Input", cx))
                    .child(watts(snap.int("total_input_power"), cx)),
            ),
        )
        .child(
            card(cx).flex_1().child(
                v_flex()
                    .gap(px(6.))
                    .child(caption("Total Output", cx))
                    .child(watts(snap.int("total_output_power"), cx)),
            ),
        );

    let has_controls = !controls.is_empty();
    let control_cards = h_flex()
        .gap(px(12.))
        .flex_wrap()
        .children(controls.into_iter().map(|c| {
            let on = model.current(&c, &snap.fields) == Some(c.on_value);
            let id = c.id.clone();
            let (on_value, off_value) = (c.on_value, c.off_value);
            card(cx).min_w(px(200.)).child(
                h_flex()
                    .gap(px(12.))
                    .justify_between()
                    .items_center()
                    .child(div().text_size(px(14.)).child(c.label.clone()))
                    .child(
                        Switch::new(SharedString::from(format!("ctl-{id}")))
                            .checked(on)
                            .on_click(cx.listener(move |view, checked: &bool, window, cx| {
                                if connected {
                                    let value = if *checked { on_value } else { off_value };
                                    view.send(
                                        CommandRequest::new(id.clone(), Some(value)),
                                        window,
                                        cx,
                                    );
                                }
                            })),
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
    let values = card(cx).child(v_flex().children(rows.into_iter().map(|(field, text)| {
        let badge = match field.confidence {
            xilo_models::schema::Confidence::Verified => None,
            xilo_models::schema::Confidence::Likely => Some("likely"),
            xilo_models::schema::Confidence::Unknown => Some("unknown"),
        };
        h_flex()
            .py(px(7.))
            .border_b_1()
            .border_color(cx.theme().border)
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
    })));

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
