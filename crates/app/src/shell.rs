use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Icon, TitleBar, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tokio::sync::mpsc;
use xilo_engine::LinkState;

use crate::add_device::AddDevice;
use crate::device_view::DeviceView;
use crate::preferences;
use crate::runtime::Runtime;
use crate::services::{AppEvent, EventKind, unix_now};
use crate::state::{AppState, Route, Tab};
use crate::theme;
use crate::widgets::{asset, battery_avatar, battery_color, caption, section, status_dot, tint};

pub const SIDEBAR_WIDTH: Pixels = px(260.);
const GITHUB: &str = "https://github.com/ftchd/xilo";

pub struct Root {
    state: Entity<AppState>,
    views: HashMap<String, Entity<DeviceView>>,
    add: Option<Entity<AddDevice>>,
    _subscriptions: Vec<Subscription>,
}

impl Root {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            let choice = this.state.read(cx).config.preferences.theme;
            theme::apply(choice, window, cx);
        });
        let observe = cx.observe_in(&state, window, |this, state, window, cx| {
            let choice = state.read(cx).config.preferences.theme;
            theme::apply(choice, window, cx);
            let keys: Vec<String> = state
                .read(cx)
                .devices
                .iter()
                .map(|(k, _)| k.clone())
                .collect();
            this.views.retain(|k, _| keys.contains(k));
            if state.read(cx).route != Route::AddDevice {
                this.add = None;
            }
            cx.notify();
        });
        Self {
            state,
            views: HashMap::new(),
            add: None,
            _subscriptions: vec![appearance, observe],
        }
    }

    fn device_view(
        &mut self,
        key: &str,
        tab: Tab,
        cx: &mut Context<Self>,
    ) -> Option<Entity<DeviceView>> {
        if let Some(view) = self.views.get(key) {
            if view.read(cx).tab != tab {
                view.update(cx, |v, cx| {
                    v.tab = tab;
                    cx.notify();
                });
            }
            return Some(view.clone());
        }
        let device = self.state.read(cx).device(key)?;
        let state = self.state.clone();
        let view = cx.new(|cx| DeviceView::new(device, state, tab, cx));
        self.views.insert(key.to_string(), view.clone());
        Some(view)
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let route = self.state.read(cx).route.clone();
        let theme = cx.theme();
        let devices: Vec<_> = self.state.read(cx).entities().cloned().collect();
        let item = |id: SharedString, selected: bool, cx: &App| {
            h_flex()
                .id(id)
                .relative()
                .px(px(10.))
                .py(px(9.))
                .gap(px(12.))
                .rounded(px(14.))
                .items_center()
                .cursor_pointer()
                .when(selected, |el| {
                    el.bg(cx.theme().primary.alpha(0.14))
                        .border_1()
                        .border_color(cx.theme().primary.alpha(0.3))
                })
                .when(!selected, |el| {
                    el.border_1()
                        .border_color(transparent_black())
                        .hover(|s| s.bg(cx.theme().muted))
                })
        };
        let mut list = v_flex().gap(px(4.));
        for device in devices {
            let d = device.read(cx);
            let key = d.key.clone();
            let snap = &d.snap;
            let selected = matches!(&route, Route::Device(k, _) if *k == key);
            let dot = match snap.link {
                LinkState::Ready => theme.success,
                LinkState::Connecting | LinkState::Handshaking | LinkState::Released => {
                    theme.warning
                }
                LinkState::Disconnected => theme.muted_foreground,
            };
            let soc = snap.is_ready().then(|| snap.int("battery_soc")).flatten();
            let out = snap.int("total_output_power").unwrap_or(0);
            let input = snap.int("total_input_power").unwrap_or(0);
            let activity = if input > out {
                format!("Charging · {input} W")
            } else if out > 0 {
                format!("{out} W out")
            } else {
                "Idle".to_string()
            };
            let assets = snap
                .model
                .as_ref()
                .map(|m| m.assets.clone())
                .unwrap_or_default();
            let state = self.state.clone();
            let thumb = div()
                .relative()
                .size(px(40.))
                .flex_none()
                .rounded(px(12.))
                .bg(theme.muted)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    if crate::assets::vendor(&format!("{assets}/product")).is_some() {
                        asset(
                            &format!("{assets}/product"),
                            px(32.),
                            IconName::BatteryCharging,
                            cx,
                        )
                    } else {
                        battery_avatar(soc.map(|s| s as f32 / 100.), None, 26., cx)
                    },
                )
                .child(
                    div()
                        .absolute()
                        .bottom(px(-2.))
                        .right(px(-2.))
                        .size(px(12.))
                        .rounded_full()
                        .bg(theme.sidebar)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(status_dot(dot)),
                );
            list = list.child(
                item(SharedString::from(format!("nav-{key}")), selected, cx)
                    .child(thumb)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.))
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(d.name()),
                            )
                            .child(match soc {
                                Some(soc) => h_flex()
                                    .gap(px(4.))
                                    .text_size(px(12.))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(battery_color(soc, cx))
                                            .child(format!("{soc}%")),
                                    )
                                    .child(caption(format!("· {activity}"), cx).text_size(px(12.)))
                                    .into_any_element(),
                                None => caption(snap.link.label().to_string(), cx)
                                    .text_size(px(12.))
                                    .into_any_element(),
                            }),
                    )
                    .on_click(move |_, _, cx| {
                        let key = key.clone();
                        state.update(cx, |s, cx| {
                            s.navigate(Route::Device(key, Tab::Dashboard), cx)
                        });
                    }),
            );
        }
        let nav =
            |id: &'static str, label: &'static str, icon: IconName, selected: bool, cx: &App| {
                item(SharedString::from(id), selected, cx)
                    .child(
                        div()
                            .size(px(28.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(if selected {
                                cx.theme().primary.alpha(0.2)
                            } else {
                                cx.theme().muted
                            })
                            .child(Icon::new(icon).size_4().text_color(if selected {
                                cx.theme().primary
                            } else {
                                cx.theme().muted_foreground
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(label),
                    )
            };
        let go =
            |id: &'static str, label: &'static str, icon: IconName, target: Route, cx: &App| {
                let state = self.state.clone();
                nav(id, label, icon, route == target, cx).on_click(move |_, _, cx| {
                    let target = target.clone();
                    state.update(cx, |s, cx| s.navigate(target, cx));
                })
            };
        let github = nav("nav-github", "GitHub", IconName::Github, false, cx)
            .child(div().flex_1())
            .child(
                Icon::new(IconName::ArrowUpRight)
                    .size_4()
                    .text_color(cx.theme().muted_foreground),
            )
            .on_click(|_, _, cx| cx.open_url(GITHUB));
        v_flex()
            .w(SIDEBAR_WIDTH)
            .flex_none()
            .h_full()
            .p(px(12.))
            .gap(px(10.))
            .border_r_1()
            .border_color(theme.border.alpha(0.06))
            .bg(linear_gradient(
                180.,
                linear_color_stop(theme.sidebar, 0.),
                linear_color_stop(tint(theme.sidebar, theme.primary, 0.04), 1.),
            ))
            .child(section("Devices", cx).px(px(10.)).pt(px(8.)))
            .child(list)
            .child(go(
                "nav-add",
                "Add Device",
                IconName::Plus,
                Route::AddDevice,
                cx,
            ))
            .child(div().flex_1())
            .child(
                v_flex()
                    .gap(px(8.))
                    .child(go(
                        "nav-prefs",
                        "Preferences",
                        IconName::Settings,
                        Route::Preferences,
                        cx,
                    ))
                    .child(github),
            )
    }
}

/// Receives alerts and automation reports, logs them and shows them.
pub fn listen(state: Entity<AppState>, mut rx: mpsc::UnboundedReceiver<AppEvent>, cx: &mut App) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        while let Some(event) = rx.recv().await {
            cx.update(|cx| {
                let s = state.read(cx);
                if let Some(history) = &s.history
                    && let Ok(h) = history.lock()
                {
                    let kind = match event.kind {
                        EventKind::Alert => "alert",
                        EventKind::Automation => "automation",
                    };
                    let _ = h.event(&event.device, unix_now(), kind, &event.text);
                }
                if !s.config.preferences.notifications {
                    return;
                }
                let note = if event.failed {
                    Notification::warning(event.text.clone())
                } else {
                    Notification::info(event.text.clone())
                }
                .title(event.title.clone())
                .in_app_and_system();
                if let Some(window) = cx.windows().first().copied() {
                    let _ = window.update(cx, |_, window, cx| window.push_notification(note, cx));
                }
            });
        }
    })
    .detach();
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let route = self.state.read(cx).route.clone();
        let content: AnyElement = match &route {
            Route::Device(key, tab) => match self.device_view(key, *tab, cx) {
                Some(view) => view.into_any_element(),
                None => div().into_any_element(),
            },
            Route::AddDevice => {
                let state = self.state.clone();
                self.add
                    .get_or_insert_with(|| cx.new(|cx| AddDevice::new(state, cx)))
                    .clone()
                    .into_any_element()
            }
            Route::Preferences => div()
                .id("prefs-scroll")
                .size_full()
                .overflow_y_scroll()
                .child(preferences::render(&self.state, cx))
                .into_any_element(),
        };
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new().child(
                    h_flex()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Xilo"),
                        )
                        .when(Runtime::get(cx).simulated, |el| {
                            el.child(caption("Simulator", cx))
                        }),
                ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(self.sidebar(cx))
                    .child(div().flex_1().h_full().overflow_hidden().child(content)),
            )
    }
}
