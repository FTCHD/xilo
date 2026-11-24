use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Placement, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_engine::LinkState;
use xilo_models::CommandRequest;

use crate::device::DeviceModel;
use crate::families;
use crate::state::{AppState, Route, Tab};
use crate::widgets::{asset, caption, status_dot};
use crate::{automations, history_view, inspector, settings};

pub struct DeviceView {
    pub device: Entity<DeviceModel>,
    pub state: Entity<AppState>,
    pub tab: Tab,
    pub blink_on: bool,
    pub custom_minutes: i64,
    pub inspector: Option<Entity<inspector::Inspector>>,
    pub history: Option<Entity<history_view::HistoryView>>,
    _blink: Task<()>,
    _observe: Subscription,
}

impl DeviceView {
    pub fn new(
        device: Entity<DeviceModel>,
        state: Entity<AppState>,
        tab: Tab,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe = cx.observe(&device, |_, _, cx| cx.notify());
        let blink = cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let alive = this.update(cx, |view, cx| {
                    if view.device.read(cx).snap.int("light_mode") == Some(4) {
                        view.blink_on = !view.blink_on;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            device,
            state,
            tab,
            blink_on: false,
            custom_minutes: 60,
            inspector: None,
            history: None,
            _blink: blink,
            _observe: observe,
        }
    }

    pub fn send(&mut self, request: CommandRequest, window: &mut Window, cx: &mut Context<Self>) {
        self.device.update(cx, |d, cx| d.send(request, window, cx));
    }

    pub fn device_busy(&self, command: &str, cx: &App) -> bool {
        self.device.read(cx).is_busy(command)
    }

    pub fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        let key = self.device.read(cx).key.clone();
        self.state
            .update(cx, |s, cx| s.navigate(Route::Device(key, tab), cx));
        cx.notify();
    }

    /// The Anker app's countdown sheet: Off, 1Hr, 2Hr, 3Hr or a custom time.
    pub fn countdown_sheet(
        &mut self,
        label: &'static str,
        command: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        window.open_sheet_at(Placement::Bottom, cx, move |sheet, _, cx| {
            let minutes = view.read(cx).custom_minutes;
            let option = |id: &'static str, text: String, secs: i64, view: Entity<DeviceView>| {
                Button::new(id)
                    .ghost()
                    .w_full()
                    .label(text)
                    .on_click(move |_, window, cx| {
                        view.update(cx, |v, cx| {
                            v.send(CommandRequest::new(command, Some(secs)), window, cx)
                        });
                        window.close_sheet(cx);
                    })
            };
            let step = |id: &'static str, delta: i64, icon: IconName, view: Entity<DeviceView>| {
                Button::new(id)
                    .ghost()
                    .small()
                    .icon(Icon::new(icon))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |v, cx| {
                            v.custom_minutes = (v.custom_minutes + delta).clamp(5, 23 * 60 + 55);
                            cx.notify();
                        });
                    })
            };
            sheet.title(format!("{label} Timer")).size(px(360.)).child(
                v_flex()
                    .gap(px(4.))
                    .child(option("t-off", "Off".into(), 0, view.clone()))
                    .child(option("t-1", "1Hr".into(), 3600, view.clone()))
                    .child(option("t-2", "2Hr".into(), 7200, view.clone()))
                    .child(option("t-3", "3Hr".into(), 10800, view.clone()))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .items_center()
                            .justify_center()
                            .pt(px(8.))
                            .child(step("t-h-", -60, IconName::ChevronsLeft, view.clone()))
                            .child(step("t-m-", -5, IconName::ChevronLeft, view.clone()))
                            .child(
                                div()
                                    .w(px(90.))
                                    .text_center()
                                    .text_size(px(18.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("{}:{:02}", minutes / 60, minutes % 60)),
                            )
                            .child(step("t-m+", 5, IconName::ChevronRight, view.clone()))
                            .child(step("t-h+", 60, IconName::ChevronsRight, view.clone())),
                    )
                    .child(option(
                        "t-custom",
                        "Set Custom Time".into(),
                        minutes * 60,
                        view.clone(),
                    )),
            )
        });
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let device = self.device.read(cx);
        let snap = &device.snap;
        let theme = cx.theme();
        let (dot, text) = match snap.link {
            LinkState::Ready => (theme.success, "Connected".to_string()),
            LinkState::Released => (theme.warning, "Released to the phone app".to_string()),
            LinkState::Connecting | LinkState::Handshaking => {
                (theme.warning, "Connecting...".to_string())
            }
            LinkState::Disconnected => (
                theme.danger,
                snap.last_error
                    .clone()
                    .map(|e| format!("Disconnected: {e}"))
                    .unwrap_or_else(|| "Disconnected".into()),
            ),
        };
        let model = snap
            .model
            .as_ref()
            .map(|m| format!("{} ({})", m.name, m.code))
            .unwrap_or_else(|| "Detecting model...".into());
        let assets = snap
            .model
            .as_ref()
            .map(|m| m.assets.clone())
            .unwrap_or_default();
        let disconnected = matches!(snap.link, LinkState::Disconnected | LinkState::Released);
        let tabs = [
            (Tab::Dashboard, "Dashboard"),
            (Tab::Settings, "Settings"),
            (Tab::History, "History"),
            (Tab::Automations, "Automations"),
            (Tab::Inspector, "Inspector"),
        ];
        let developer = self.state.read(cx).config.preferences.developer_mode;
        let current = self.tab;
        v_flex()
            .gap(px(14.))
            .child(
                h_flex()
                    .gap(px(14.))
                    .items_center()
                    .child(asset(
                        &format!("{assets}/product"),
                        px(56.),
                        IconName::BatteryCharging,
                        cx,
                    ))
                    .child(
                        v_flex()
                            .flex_1()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(device.name()),
                            )
                            .child(
                                h_flex()
                                    .gap(px(8.))
                                    .items_center()
                                    .child(status_dot(dot))
                                    .child(caption(text, cx))
                                    .child(caption(format!("· {model}"), cx)),
                            ),
                    )
                    .when(disconnected, |el| {
                        el.child(
                            Button::new("reconnect")
                                .small()
                                .primary()
                                .label("Connect")
                                .on_click(
                                    cx.listener(|view, _, _, cx| view.device.read(cx).connect(cx)),
                                ),
                        )
                    }),
            )
            .child(
                h_flex().gap(px(6.)).children(
                    tabs.into_iter()
                        .filter(|(tab, _)| *tab != Tab::Inspector || developer)
                        .map(|(tab, label)| {
                            let selected = tab == current;
                            div()
                                .id(SharedString::from(label))
                                .px(px(14.))
                                .py(px(6.))
                                .rounded(px(16.))
                                .text_size(px(14.))
                                .cursor_pointer()
                                .when(selected, |el| {
                                    el.bg(cx.theme().primary)
                                        .text_color(cx.theme().primary_foreground)
                                        .font_weight(FontWeight::SEMIBOLD)
                                })
                                .when(!selected, |el| el.hover(|s| s.bg(cx.theme().muted)))
                                .child(label)
                                .on_click(cx.listener(move |view, _, _, cx| view.set_tab(tab, cx)))
                        }),
                ),
            )
    }
}

impl Render for DeviceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.tab {
            Tab::Dashboard => families::dashboard(self, window, cx),
            Tab::Settings => settings::render(self, window, cx),
            Tab::History => {
                let view = self.history.get_or_insert_with(|| {
                    let device = self.device.clone();
                    let state = self.state.clone();
                    cx.new(|cx| history_view::HistoryView::new(device, state, cx))
                });
                view.clone().into_any_element()
            }
            Tab::Automations => automations::render(self, window, cx),
            Tab::Inspector => {
                let view = self.inspector.get_or_insert_with(|| {
                    let device = self.device.clone();
                    cx.new(|cx| inspector::Inspector::new(device, window, cx))
                });
                view.clone().into_any_element()
            }
        };
        v_flex()
            .id("device-scroll")
            .size_full()
            .overflow_y_scroll()
            .p(px(24.))
            .gap(px(18.))
            .child(self.header(cx))
            .child(body)
    }
}
