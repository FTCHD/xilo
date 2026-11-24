use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_store::{Preferences, ThemeChoice};

use crate::state::AppState;
use crate::widgets::{caption, card, divided, section};

const LICENSES: &str = include_str!("../../../THIRD_PARTY_NOTICES.md");

fn row(label: &str, hint: &str, control: AnyElement, cx: &App) -> AnyElement {
    h_flex()
        .px(px(14.))
        .py(px(12.))
        .gap(px(16.))
        .justify_between()
        .items_center()
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(2.))
                .child(div().text_size(px(15.)).child(label.to_string()))
                .when(!hint.is_empty(), |el| {
                    el.child(caption(hint.to_string(), cx))
                }),
        )
        .child(control)
        .into_any_element()
}

fn update(state: &Entity<AppState>, cx: &mut App, change: impl FnOnce(&mut Preferences)) {
    state.update(cx, |s, cx| {
        let mut prefs = s.config.preferences.clone();
        change(&mut prefs);
        s.set_preferences(prefs, cx);
    });
    cx.refresh_windows();
}

fn switch(
    id: &'static str,
    on: bool,
    state: &Entity<AppState>,
    change: fn(&mut Preferences, bool),
) -> AnyElement {
    let state = state.clone();
    Switch::new(id)
        .checked(on)
        .on_click(move |checked: &bool, _, cx| {
            let checked = *checked;
            update(&state, cx, |p| change(p, checked));
        })
        .into_any_element()
}

fn choices<T: Copy + PartialEq + 'static>(
    id: &'static str,
    items: &[(T, &'static str)],
    current: T,
    state: &Entity<AppState>,
    change: fn(&mut Preferences, T),
) -> AnyElement {
    h_flex()
        .gap(px(6.))
        .children(items.iter().enumerate().map(|(i, (value, label))| {
            let state = state.clone();
            let value = *value;
            Button::new(SharedString::from(format!("{id}-{i}")))
                .small()
                .when(value == current, |b| b.primary())
                .when(value != current, |b| b.ghost())
                .label(*label)
                .on_click(move |_, _, cx| update(&state, cx, |p| change(p, value)))
        }))
        .into_any_element()
}

pub fn render(state: &Entity<AppState>, cx: &App) -> AnyElement {
    let p = state.read(cx).config.preferences.clone();
    v_flex()
        .size_full()
        .p(px(24.))
        .gap(px(14.))
        .child(
            div()
                .text_size(px(22.))
                .font_weight(FontWeight::BOLD)
                .child("Preferences"),
        )
        .child(section("General", cx))
        .child(card(cx).p(px(4.)).child(divided(
            [
                row(
                    "Appearance",
                    "",
                    choices(
                        "theme",
                        &[
                            (ThemeChoice::System, "System"),
                            (ThemeChoice::Light, "Light"),
                            (ThemeChoice::Dark, "Dark"),
                        ],
                        p.theme,
                        state,
                        |p, v| p.theme = v,
                    ),
                    cx,
                ),
                row(
                    "Keep running in the menu bar",
                    "Closing the window keeps Xilo connected and running its automations.",
                    switch("tray", p.keep_running_in_tray, state, |p, v| {
                        p.keep_running_in_tray = v
                    }),
                    cx,
                ),
                row(
                    "Open at login",
                    "",
                    switch("login", p.launch_at_login, state, |p, v| {
                        p.launch_at_login = v
                    }),
                    cx,
                ),
            ],
            cx,
        )))
        .child(section("Alerts", cx))
        .child(card(cx).p(px(4.)).child(divided(
            [
                row(
                    "Notifications",
                    "Show errors, low battery, full battery and automation actions.",
                    switch("notify", p.notifications, state, |p, v| p.notifications = v),
                    cx,
                ),
                row(
                    "Low battery alert",
                    "",
                    choices(
                        "low",
                        &[(0u8, "Off"), (10, "10%"), (20, "20%"), (30, "30%")],
                        p.low_battery_alert,
                        state,
                        |p, v| p.low_battery_alert = v,
                    ),
                    cx,
                ),
            ],
            cx,
        )))
        .child(section("History", cx))
        .child(card(cx).p(px(4.)).child(divided(
            [
                row(
                    "Record history",
                    "Battery and power every 10 seconds, stored only on this Mac.",
                    switch("record", p.record_history, state, |p, v| {
                        p.record_history = v
                    }),
                    cx,
                ),
                row(
                    "Keep history for",
                    "",
                    choices(
                        "days",
                        &[(30u32, "30 days"), (90, "90 days"), (365, "1 year")],
                        p.history_days,
                        state,
                        |p, v| p.history_days = v,
                    ),
                    cx,
                ),
            ],
            cx,
        )))
        .child(section("Advanced", cx))
        .child(card(cx).p(px(4.)).child(row(
            "Developer mode",
            "Shows the Inspector tab with raw frames, captures and raw commands.",
            switch("dev", p.developer_mode, state, |p, v| p.developer_mode = v),
            cx,
        )))
        .into_any_element()
}

pub fn about(cx: &App) -> AnyElement {
    v_flex()
        .id("about")
        .size_full()
        .overflow_y_scroll()
        .p(px(24.))
        .gap(px(14.))
        .child(div().text_size(px(22.)).font_weight(FontWeight::BOLD).child("About Xilo"))
        .child(card(cx).child(
            v_flex()
                .gap(px(6.))
                .child(div().text_size(px(15.)).child(format!("Version {}", env!("CARGO_PKG_VERSION"))))
                .child(caption(
                    "Xilo controls Anker SOLIX devices over Bluetooth. It works without an account and without the internet. \
                     Xilo is not made by or affiliated with Anker.",
                    cx,
                )),
        ))
        .child(section("Third-Party Notices", cx))
        .child(
            card(cx).child(
                div()
                    .text_size(px(12.))
                    .font_family(cx.theme().mono_font_family.clone())
                    .whitespace_normal()
                    .child(LICENSES),
            ),
        )
        .into_any_element()
}
