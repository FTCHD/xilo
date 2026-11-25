use std::collections::VecDeque;

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::assets;

/// Colour for power flowing into a device.
pub fn input_color(cx: &App) -> Hsla {
    cx.theme().success
}

/// Colour for power flowing out of a device. Deeper on light backgrounds,
/// where the brand cyan is hard to read.
pub fn output_color(cx: &App) -> Hsla {
    if cx.theme().is_dark() {
        cx.theme().primary
    } else {
        rgb(0x0093b0).into()
    }
}

/// `base` with `color` laid over it at strength `t`.
pub fn tint(base: Hsla, color: Hsla, t: f32) -> Hsla {
    base.blend(color.alpha(t))
}

fn lift(cx: &App) -> Vec<BoxShadow> {
    let dark = cx.theme().is_dark();
    vec![BoxShadow {
        color: hsla(0., 0., 0., if dark { 0.35 } else { 0.06 }),
        offset: point(px(0.), px(6.)),
        blur_radius: px(18.),
        spread_radius: px(-6.),
        inset: false,
    }]
}

/// A raised surface: a faint top highlight, a hairline border and a soft shadow.
pub fn card(cx: &App) -> Div {
    let base = cx.theme().group_box;
    let top = tint(
        base,
        hsla(0., 0., 1., 1.),
        if cx.theme().is_dark() { 0.035 } else { 0.0 },
    );
    div()
        .bg(linear_gradient(
            180.,
            linear_color_stop(top, 0.),
            linear_color_stop(base, 1.),
        ))
        .border_1()
        .border_color(
            cx.theme()
                .border
                .alpha(if cx.theme().is_dark() { 0.07 } else { 0.06 }),
        )
        .rounded(px(20.))
        .shadow(lift(cx))
        .p(px(18.))
}

/// A card that lights up in `color` while `active`.
pub fn live_card(active: bool, color: Hsla, cx: &App) -> Div {
    let base = cx.theme().group_box;
    if !active {
        return card(cx);
    }
    card(cx)
        .bg(linear_gradient(
            160.,
            linear_color_stop(tint(base, color, 0.16), 0.),
            linear_color_stop(tint(base, color, 0.03), 1.),
        ))
        .border_color(color.alpha(0.45))
}

pub fn caption(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_size(px(13.))
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// A small heading above a group of cards.
pub fn section(text: impl Into<SharedString>, cx: &App) -> Div {
    let text: SharedString = text.into();
    div()
        .pt(px(4.))
        .text_size(px(12.))
        .font_weight(FontWeight::BOLD)
        .text_color(cx.theme().muted_foreground.alpha(0.8))
        .child(text.to_uppercase())
}

/// A rounded label tinted in `color`.
pub fn pill(text: impl Into<SharedString>, color: Hsla) -> Div {
    h_flex()
        .gap(px(6.))
        .items_center()
        .px(px(10.))
        .py(px(3.))
        .rounded_full()
        .bg(color.alpha(0.14))
        .text_color(color)
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.into())
}

/// An icon in a tinted rounded square.
pub fn icon_badge(icon: IconName, color: Hsla, size: Pixels) -> Div {
    div()
        .size(size)
        .flex_none()
        .rounded(size * 0.32)
        .bg(color.alpha(0.16))
        .flex()
        .items_center()
        .justify_center()
        .child(Icon::new(icon).size(size * 0.5).text_color(color))
}

/// A large number with a small unit, or a dash when unknown.
pub fn big_value(value: Option<i64>, unit: &str, size: f32, color: Option<Hsla>, cx: &App) -> Div {
    match value {
        Some(v) => h_flex()
            .items_baseline()
            .gap(px(3.))
            .child(
                div()
                    .text_size(px(size))
                    .font_weight(FontWeight::BOLD)
                    .when_some(color, |el, c| el.text_color(c))
                    .child(v.to_string()),
            )
            .child(
                div()
                    .text_size(px(size * 0.45))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child(unit.to_string()),
            ),
        None => div()
            .text_size(px(size))
            .font_weight(FontWeight::BOLD)
            .text_color(cx.theme().muted_foreground.alpha(0.5))
            .child("--"),
    }
}

/// Colour for a battery level: red when low, amber when getting low.
pub fn battery_color(soc: i64, cx: &App) -> Hsla {
    match soc {
        ..=15 => cx.theme().danger,
        16..=30 => cx.theme().warning,
        _ => cx.theme().success,
    }
}

/// An extracted Anker image, or a fallback icon when vendor assets are missing.
pub fn asset(key: &str, size: Pixels, fallback: IconName, cx: &App) -> AnyElement {
    match assets::vendor(key) {
        Some(path) => img(path)
            .size(size)
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        None => div()
            .size(size)
            .flex()
            .items_center()
            .justify_center()
            .child(
                Icon::new(fallback)
                    .size(size * 0.6)
                    .text_color(cx.theme().muted_foreground),
            )
            .into_any_element(),
    }
}

/// A battery that fills to the charge level, for when the Anker images are missing.
pub fn battery_avatar(
    level: Option<f32>,
    label: Option<String>,
    size: f32,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let body_w = size * 0.56;
    let body_h = size * 0.84;
    let border = (size * 0.035).max(2.);
    let inset = border + (size * 0.03).max(2.);
    let fill = level.map(|l| l.clamp(0., 1.));
    let color = fill.map_or(theme.muted_foreground, |l| {
        battery_color((l * 100.).round() as i64, cx)
    });
    v_flex()
        .w(px(size))
        .h(px(size))
        .flex_none()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(size * 0.22))
                .h(px(size * 0.06))
                .rounded_t(px(size * 0.03))
                .bg(theme.foreground.alpha(0.35)),
        )
        .child(
            div()
                .relative()
                .w(px(body_w))
                .h(px(body_h))
                .rounded(px(size * 0.12))
                .border(px(border))
                .border_color(theme.foreground.alpha(0.35))
                .when_some(fill.filter(|l| *l > 0.), |el, l| {
                    let inner = body_h - inset * 2.;
                    el.child(
                        div()
                            .absolute()
                            .left(px(inset - border))
                            .right(px(inset - border))
                            .bottom(px(inset - border))
                            .h(px(inner * l))
                            .rounded(px(size * 0.06))
                            .bg(linear_gradient(
                                180.,
                                linear_color_stop(color, 0.),
                                linear_color_stop(color.alpha(0.75), 1.),
                            )),
                    )
                })
                .when_some(label, |el, label| {
                    el.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px((size * 0.16).max(10.)))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child(label),
                    )
                }),
        )
        .into_any_element()
}

/// The Anker battery animation: the outline with the column clipped to the level.
pub fn battery_gauge(key: &str, level: f32, scale: f32) -> AnyElement {
    let (Some(base), Some(column)) = (
        assets::vendor(&format!("{key}_base")),
        assets::vendor(&format!("{key}_column")),
    ) else {
        return div().into_any_element();
    };
    let w = 100.0 * scale;
    let h = 106.0 * scale;
    let clip_top = 8.0 * scale;
    let clip_h = 97.0 * scale;
    let offset = (1.0 - level.clamp(0.0, 1.0)) * 97.0 * scale;
    div()
        .relative()
        .w(px(w))
        .h(px(h))
        .child(img(base).absolute().top_0().left_0().w(px(w)).h(px(h)))
        .child(
            div()
                .absolute()
                .left_0()
                .top(px(clip_top))
                .w(px(w))
                .h(px(clip_h))
                .overflow_hidden()
                .child(
                    img(column)
                        .absolute()
                        .left_0()
                        .top(px(offset - clip_top))
                        .w(px(w))
                        .h(px(h)),
                ),
        )
        .into_any_element()
}

/// A small chart of recent values: a line with a fading area under it.
pub fn sparkline(values: &VecDeque<f64>, color: Hsla, height: Pixels) -> AnyElement {
    let points: Vec<f64> = values.iter().copied().collect();
    canvas(
        move |_, _, _| points,
        move |bounds, points, window, _| {
            if points.len() < 2 {
                return;
            }
            let max = points.iter().copied().fold(1.0_f64, f64::max);
            let w = f32::from(bounds.size.width);
            let h = f32::from(bounds.size.height) - 4.0;
            let step = w / (points.len() - 1) as f32;
            let at = |i: usize, v: f64| {
                point(
                    bounds.origin.x + px(step * i as f32),
                    bounds.origin.y + px(2.0 + h - (v / max) as f32 * h),
                )
            };
            let bottom = bounds.origin.y + bounds.size.height;
            let mut area = PathBuilder::fill();
            area.move_to(point(bounds.origin.x, bottom));
            for (i, v) in points.iter().enumerate() {
                area.line_to(at(i, *v));
            }
            area.line_to(point(bounds.origin.x + bounds.size.width, bottom));
            area.close();
            if let Ok(path) = area.build() {
                window.paint_path(
                    path,
                    linear_gradient(
                        180.,
                        linear_color_stop(color.alpha(0.35), 0.),
                        linear_color_stop(color.alpha(0.0), 1.),
                    ),
                );
            }
            let mut line = PathBuilder::stroke(px(2.));
            for (i, v) in points.iter().enumerate() {
                if i == 0 {
                    line.move_to(at(i, *v));
                } else {
                    line.line_to(at(i, *v));
                }
            }
            if let Ok(path) = line.build() {
                window.paint_path(path, color);
            }
        },
    )
    .w_full()
    .h(height)
    .into_any_element()
}

/// A port with its icon, name and power. Active ports light up.
pub fn port_tile(
    label: &str,
    icon_key: &str,
    fallback: IconName,
    value: Option<i64>,
    active: bool,
    input: bool,
    cx: &App,
) -> Div {
    let color = if input {
        input_color(cx)
    } else {
        output_color(cx)
    };
    let status = match (active, input) {
        (false, _) => "Idle",
        (true, true) => "Input",
        (true, false) => "Output",
    };
    live_card(active, color, cx)
        .flex_1()
        .min_w(px(120.))
        .p(px(16.))
        .child(
            v_flex()
                .gap(px(10.))
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(if assets::vendor(icon_key).is_some() {
                            div()
                                .size(px(40.))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(rgb(0xf3f7fb))
                                .when(!active, |el| el.opacity(0.4))
                                .child(asset(icon_key, px(28.), fallback, cx))
                        } else {
                            icon_badge(
                                fallback,
                                if active {
                                    color
                                } else {
                                    cx.theme().muted_foreground
                                },
                                px(40.),
                            )
                        })
                        .child(
                            div()
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if active {
                                    color
                                } else {
                                    cx.theme().muted_foreground.alpha(0.6)
                                })
                                .child(status),
                        ),
                )
                .child(
                    v_flex()
                        .gap(px(2.))
                        .child(
                            caption(label.to_string(), cx)
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD),
                        )
                        .child(big_value(
                            value.filter(|v| *v > 0),
                            "W",
                            22.,
                            active.then_some(color),
                            cx,
                        )),
                ),
        )
}

/// A round power button, filled while on.
pub fn power_button(id: impl Into<ElementId>, on: bool, enabled: bool, cx: &App) -> Stateful<Div> {
    let color = output_color(cx);
    div()
        .id(id)
        .size(px(40.))
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .when(on, |el| el.bg(color))
        .when(!on, |el| {
            el.bg(cx.theme().muted)
                .border_1()
                .border_color(cx.theme().border)
        })
        .when(enabled, |el| el.cursor_pointer().hover(|s| s.opacity(0.85)))
        .when(!enabled, |el| el.opacity(0.5))
        .child(Icon::new(IconName::Power).size(px(18.)).text_color(if on {
            cx.theme().primary_foreground
        } else {
            cx.theme().muted_foreground
        }))
}

pub fn status_dot(color: Hsla) -> Div {
    div().size(px(8.)).rounded_full().bg(color)
}

/// Stacks rows with a thin line between them, never after the last one.
pub fn divided(rows: impl IntoIterator<Item = AnyElement>, cx: &App) -> Div {
    let mut list = v_flex();
    for (i, row) in rows.into_iter().enumerate() {
        if i > 0 {
            list = list.child(
                div()
                    .h(px(1.))
                    .mx(px(14.))
                    .bg(cx.theme().border.alpha(0.06)),
            );
        }
        list = list.child(row);
    }
    list
}

/// The track of a segmented control. Fill it with [`segment`]s.
pub fn segmented(cx: &App) -> Div {
    h_flex()
        .self_start()
        .p(px(4.))
        .gap(px(2.))
        .rounded_full()
        .bg(cx.theme().muted)
        .border_1()
        .border_color(cx.theme().border.alpha(0.06))
}

/// One option in a segmented control.
pub fn segment(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let primary = cx.theme().primary;
    div()
        .id(id)
        .px(px(14.))
        .py(px(6.))
        .rounded_full()
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .when(selected, |el| {
            el.bg(primary).text_color(cx.theme().primary_foreground)
        })
        .when(!selected, |el| {
            el.text_color(cx.theme().muted_foreground)
                .hover(|s| s.text_color(cx.theme().foreground))
        })
        .child(label.into())
}
