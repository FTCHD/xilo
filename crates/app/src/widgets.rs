use std::collections::VecDeque;

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::*;

use crate::assets;

pub fn card(cx: &App) -> Div {
    div().bg(cx.theme().group_box).rounded(px(20.)).p(px(18.))
}

pub fn caption(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_size(px(13.))
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

pub fn section(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_size(px(15.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// A value with a small unit, or "--" when unknown.
pub fn watts(value: Option<i64>, cx: &App) -> Div {
    match value {
        Some(v) if v > 0 => h_flex()
            .items_baseline()
            .gap(px(3.))
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(FontWeight::BOLD)
                    .child(v.to_string()),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(cx.theme().muted_foreground)
                    .child("W"),
            ),
        _ => div()
            .text_size(px(20.))
            .font_weight(FontWeight::BOLD)
            .text_color(cx.theme().muted_foreground)
            .child("--"),
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

/// A small line chart of recent values.
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
            let mut path = PathBuilder::stroke(px(2.));
            for (i, v) in points.iter().enumerate() {
                let x = bounds.origin.x + px(step * i as f32);
                let y = bounds.origin.y + px(2.0 + h - (*v / max) as f32 * h);
                if i == 0 {
                    path.move_to(point(x, y));
                } else {
                    path.line_to(point(x, y));
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        },
    )
    .w_full()
    .h(height)
    .into_any_element()
}

pub fn port_tile(label: &str, icon_key: &str, value: Option<i64>, cx: &App) -> Div {
    card(cx).flex_1().min_w(px(110.)).child(
        v_flex()
            .gap(px(8.))
            .child(
                div()
                    .size(px(40.))
                    .rounded_full()
                    .bg(rgb(0xf3f7fb))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(asset(icon_key, px(28.), IconName::Plug, cx)),
            )
            .child(caption(label.to_string(), cx))
            .child(watts(value, cx)),
    )
}

pub fn status_dot(color: Hsla) -> Div {
    div().size(px(8.)).rounded_full().bg(color)
}
