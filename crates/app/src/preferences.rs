use gpui_kit::assets::IconName;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_store::{Preferences, ThemeChoice};

use crate::state::AppState;
use crate::widgets::{
    caption, card, divided, glow, icon_badge, pill, section, segment, segmented, tint,
};

const LICENSES: &str = include_str!("../../../THIRD_PARTY_NOTICES.md");

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
                    "Keep running in the menu bar",
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
            ],
            cx,
        )))
        .child(section("History", cx))
        .child(card(cx).p(px(4.)).child(divided(
            [
                row(
                    IconName::ChartLine,
                    "Record history",
                    "Battery and power every 10 seconds, stored only on this Mac.",
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

pub fn about(cx: &App) -> AnyElement {
    let primary = cx.theme().primary;
    let credit = |name: &'static str,
                  url: &'static str,
                  holder: &'static str,
                  what: &'static str,
                  cx: &App| {
        card(cx).flex_1().child(
            v_flex()
                .gap(px(8.))
                .child(
                    h_flex()
                        .gap(px(10.))
                        .items_center()
                        .child(icon_badge(IconName::Github, cx.theme().foreground, px(32.)))
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(name),
                        )
                        .child(pill("MIT", cx.theme().success)),
                )
                .child(caption(what, cx))
                .child(caption(holder, cx).text_size(px(12.)))
                .child(
                    div()
                        .id(name)
                        .text_size(px(12.))
                        .text_color(primary)
                        .cursor_pointer()
                        .hover(|s| s.underline())
                        .child(url)
                        .on_click(move |_, _, cx| cx.open_url(url)),
                ),
        )
    };
    v_flex()
        .id("about")
        .size_full()
        .overflow_y_scroll()
        .p(px(24.))
        .gap(px(16.))
        .child(
            card(cx)
                .bg(linear_gradient(
                    135.,
                    linear_color_stop(tint(cx.theme().group_box, primary, 0.18), 0.),
                    linear_color_stop(cx.theme().group_box, 1.),
                ))
                .p(px(24.))
                .child(
                    h_flex()
                        .gap(px(20.))
                        .items_center()
                        .child(
                            div()
                                .size(px(80.))
                                .flex_none()
                                .rounded(px(22.))
                                .bg(linear_gradient(
                                    160.,
                                    linear_color_stop(rgb(0x00cceb), 0.),
                                    linear_color_stop(rgb(0x0073b3), 1.),
                                ))
                                .shadow(glow(primary, 0.7))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(Icon::new(IconName::Zap).size(px(40.)).text_color(white())),
                        )
                        .child(
                            v_flex()
                                .gap(px(6.))
                                .child(
                                    h_flex()
                                        .gap(px(10.))
                                        .items_center()
                                        .child(div().text_size(px(28.)).font_weight(FontWeight::BOLD).child("Xilo"))
                                        .child(pill(format!("Version {}", env!("CARGO_PKG_VERSION")), primary)),
                                )
                                .child(caption(
                                    "Controls Anker SOLIX devices over Bluetooth. No account, no cloud, no internet.",
                                    cx,
                                ))
                                .child(caption("Xilo is not made by or affiliated with Anker.", cx).text_size(px(12.))),
                        ),
                ),
        )
        .child(section("Built On", cx))
        .child(
            h_flex()
                .gap(px(14.))
                .items_stretch()
                .child(credit(
                    "SolixBLE",
                    "https://github.com/flip-dots/SolixBLE",
                    "© 2026 Harvey Lelliott",
                    "Protocol logic, Prime support, field maps and test data.",
                    cx,
                ))
                .child(credit(
                    "anker-solix-api",
                    "https://github.com/thomluther/anker-solix-api",
                    "© 2024 thomluther",
                    "Field and command maps for most models.",
                    cx,
                )),
        )
        .child(section("License Texts", cx))
        .child(
            card(cx).child(
                div()
                    .text_size(px(11.))
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(LICENSES),
            ),
        )
        .into_any_element()
}
