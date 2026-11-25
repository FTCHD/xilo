use gpui_kit::assets::IconName;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_store::{Preferences, ThemeChoice};

use crate::state::AppState;
use crate::widgets::{caption, card, divided, icon_badge, section, segment, segmented};

fn row(icon: IconName, label: &str, hint: &str, control: AnyElement, cx: &App) -> AnyElement {
    h_flex()
        .px(px(14.))
        .py(px(12.))
        .gap(px(14.))
        .justify_between()
        .items_center()
        .child(icon_badge(icon, cx.theme().primary, px(32.)))
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
    cx: &App,
) -> AnyElement {
    segmented(cx)
        .children(items.iter().enumerate().map(|(i, (value, label))| {
            let state = state.clone();
            let value = *value;
            segment(
                SharedString::from(format!("{id}-{i}")),
                *label,
                value == current,
                cx,
            )
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
                    IconName::SunMoon,
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
                        cx,
                    ),
                    cx,
                ),
                row(
                    IconName::PanelTop,
                    &format!("Keep running in the {}", crate::tray::PLACE),
                    "Closing the window keeps Xilo connected and running its automations.",
                    switch("tray", p.keep_running_in_tray, state, |p, v| {
                        p.keep_running_in_tray = v
                    }),
                    cx,
                ),
                row(
                    IconName::LogIn,
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
                    IconName::Bell,
                    "Notifications",
                    "Show errors, low battery, full battery and automation actions.",
                    switch("notify", p.notifications, state, |p, v| p.notifications = v),
                    cx,
                ),
                row(
                    IconName::BatteryLow,
                    "Low battery alert",
                    "",
                    choices(
                        "low",
                        &[(0u8, "Off"), (10, "10%"), (20, "20%"), (30, "30%")],
                        p.low_battery_alert,
                        state,
                        |p, v| p.low_battery_alert = v,
                        cx,
                    ),
                    cx,
                ),
                row(
                    IconName::BatteryFull,
                    "Charged alert",
                    "Many people stop charging at 80% to keep the battery healthy.",
                    choices(
                        "charged",
                        &[(0u8, "Off"), (80, "80%"), (90, "90%"), (100, "100%")],
                        p.charged_alert,
                        state,
                        |p, v| p.charged_alert = v,
                        cx,
                    ),
                    cx,
                ),
                row(
                    IconName::Unplug,
                    "AC power alerts",
                    "When a device loses AC power and runs on its battery, and when the power comes back.",
                    switch("power", p.power_alerts, state, |p, v| p.power_alerts = v),
                    cx,
                ),
            ],
            cx,
        )))
        .child(section("History", cx))
        .child(card(cx).p(px(4.)).child(divided(
            [
                row(
                    IconName::ChartLine,
                    "Record history",
                    "Battery and power every 10 seconds, stored only on this computer.",
                    switch("record", p.record_history, state, |p, v| {
                        p.record_history = v
                    }),
                    cx,
                ),
                row(
                    IconName::CalendarDays,
                    "Keep history for",
                    "",
                    choices(
                        "days",
                        &[(30u32, "30 days"), (90, "90 days"), (365, "1 year")],
                        p.history_days,
                        state,
                        |p, v| p.history_days = v,
                        cx,
                    ),
                    cx,
                ),
            ],
            cx,
        )))
        .child(section("Advanced", cx))
        .child(card(cx).p(px(4.)).child(row(
            IconName::SquareTerminal,
            "Developer mode",
            "Shows the Inspector tab with raw frames, captures and raw commands.",
            switch("dev", p.developer_mode, state, |p, v| p.developer_mode = v),
            cx,
        )))
        .into_any_element()
}
