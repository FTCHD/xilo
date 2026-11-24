use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Placement, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_engine::LinkState;
use xilo_models::CommandRequest;

use crate::device::DeviceModel;
use crate::families;
use crate::families::view_model::{duration_text, temperature_text};
use crate::shell::SIDEBAR_WIDTH;
use crate::state::{AppState, Route, Tab};
use crate::widgets::{battery_color, caption};
use crate::{automations, history_view, inspector, settings};

pub struct DeviceView {
    pub device: Entity<DeviceModel>,
    pub state: Entity<AppState>,
    pub tab: Tab,
    pub blink_on: bool,
    pub custom_minutes: i64,
    pub inspector: Option<Entity<inspector::Inspector>>,
    pub history: Option<Entity<history_view::HistoryView>>,
    pub schedule_editor: Option<automations::Editor>,
    scroll: ScrollHandle,
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
            schedule_editor: None,
            scroll: ScrollHandle::new(),
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
        self.scroll.set_offset(point(px(0.), px(0.)));
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

    fn header(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let device = self.device.read(cx);
        let snap = &device.snap;
        let error = matches!(snap.link, LinkState::Disconnected)
            .then(|| snap.last_error.clone())
            .flatten();
        let ready = snap.is_ready();
        let soc = ready.then(|| snap.int("battery_soc")).flatten();
        let fahrenheit = snap.int("temperature_unit") == Some(1);
        let temperature = ready
            .then(|| snap.int("temperature"))
            .flatten()
            .map(|t| temperature_text(t, fahrenheit));
        let input = snap.int("total_input_power").unwrap_or(0);
        let output = snap.int("total_output_power").unwrap_or(0);
        let remaining = ready
            .then(|| snap.float("remaining_time"))
            .flatten()
            .filter(|_| input != output)
            .map(|hours| {
                let time = duration_text((hours * 60.0).round() as i64);
                if input > output {
                    format!("{time} to full")
                } else {
                    format!("{time} left")
                }
            });
        let details: Vec<String> = temperature.into_iter().chain(remaining).collect();
        let status = match snap.link {
            LinkState::Ready if snap.model.is_none() => Some("Detecting model".to_string()),
            LinkState::Ready if details.is_empty() => None,
            LinkState::Ready => Some(details.join(" · ")),
            other => Some(other.label().to_string()),
        };
        let disconnected = matches!(snap.link, LinkState::Disconnected | LinkState::Released);
        let name = device.name();

        let developer = self.state.read(cx).config.preferences.developer_mode;
        let tabs: Vec<(Tab, &'static str, IconName)> = [
            (Tab::Dashboard, "Dashboard", IconName::LayoutDashboard),
            (Tab::Settings, "Settings", IconName::Settings),
            (Tab::History, "History", IconName::ChartLine),
            (Tab::Automations, "Automations", IconName::Workflow),
            (Tab::Inspector, "Inspector", IconName::SquareTerminal),
        ]
        .into_iter()
        .filter(|(tab, ..)| *tab != Tab::Inspector || developer)
        .collect();

        let subtitle = match soc {
            Some(soc) if !details.is_empty() => {
                text_width(&format!("{soc}%"), 15., FontWeight::BOLD, window, cx)
                    + px(6.)
                    + text_width(
                        &format!("· {}", details.join(" · ")),
                        13.,
                        FontWeight::NORMAL,
                        window,
                        cx,
                    )
            }
            Some(soc) => text_width(&format!("{soc}%"), 15., FontWeight::BOLD, window, cx),
            None => status.as_deref().map_or(px(0.), |s| {
                text_width(s, 13., FontWeight::NORMAL, window, cx)
            }),
        };
        let title = px(AVATAR)
            + GAP
            + text_width(&name, 26., FontWeight::BOLD, window, cx).max(subtitle)
            + if disconnected { CONNECT } else { px(0.) };
        let count = tabs.len() as f32;
        let text = tabs
            .iter()
            .map(|(_, label, _)| text_width(label, 13., FontWeight::SEMIBOLD, window, cx))
            .fold(px(0.), |sum, w| sum + w);
        let available = window.viewport_size().width - SIDEBAR_WIDTH - PADDING * 2.;
        let track = |items: Pixels| items + px(2.) * (count - 1.) + px(10.);
        let labels = track(text + px(32.) * count);
        let layout = if title + GAP + labels <= available {
            TabLayout::Labels
        } else if title + GAP + track(px(40.) * count) <= available {
            TabLayout::Icons
        } else {
            TabLayout::Below
        };
        let below = layout == TabLayout::Below;
        let compact = match layout {
            TabLayout::Labels => false,
            TabLayout::Icons => true,
            TabLayout::Below => track(text + px(16.) * count) > available,
        };
        let padding = match (compact, below) {
            (true, _) => 12.,
            (false, true) => 8.,
            (false, false) => 16.,
        };

        let current = self.tab;
        let theme = cx.theme();
        let bar = h_flex()
            .flex_none()
            .p(px(4.))
            .gap(px(2.))
            .rounded_full()
            .bg(theme.muted)
            .border_1()
            .border_color(theme.border.alpha(0.06))
            .when(below, |el| el.w_full())
            .children(tabs.into_iter().map(|(tab, label, icon)| {
                let selected = tab == current;
                div()
                    .id(label)
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(below, |el| el.flex_1())
                    .px(px(padding))
                    .py(px(7.))
                    .rounded_full()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .when(selected, |el| {
                        el.bg(cx.theme().primary)
                            .text_color(cx.theme().primary_foreground)
                    })
                    .when(!selected, |el| {
                        el.text_color(cx.theme().muted_foreground)
                            .hover(|s| s.text_color(cx.theme().foreground))
                    })
                    .map(|el| {
                        if compact {
                            el.child(Icon::new(icon).size(px(16.)))
                                .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
                        } else {
                            el.child(label)
                        }
                    })
                    .on_click(cx.listener(move |view, _, _, cx| view.set_tab(tab, cx)))
            }));

        let row = h_flex()
            .gap(GAP)
            .items_center()
            .child(families::avatar(self, AVATAR, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(26.))
                            .font_weight(FontWeight::BOLD)
                            .truncate()
                            .child(name),
                    )
                    .map(|el| match soc {
                        Some(soc) => el.child(
                            h_flex()
                                .gap(px(6.))
                                .items_center()
                                .child(
                                    div()
                                        .text_size(px(15.))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(battery_color(soc, cx))
                                        .child(format!("{soc}%")),
                                )
                                .when(!details.is_empty(), |el| {
                                    el.child(caption(format!("· {}", details.join(" · ")), cx))
                                }),
                        ),
                        None => el.when_some(status, |el, s| el.child(caption(s, cx))),
                    })
                    .when_some(error, |el, e| {
                        el.child(caption(e, cx).text_color(cx.theme().danger))
                    }),
            )
            .when(disconnected, |el| {
                el.child(
                    Button::new("reconnect")
                        .primary()
                        .label("Connect")
                        .on_click(cx.listener(|view, _, _, cx| view.device.read(cx).connect(cx))),
                )
            });
        if below {
            v_flex().gap(px(18.)).child(row).child(bar)
        } else {
            v_flex().child(row.child(bar))
        }
    }
}

const AVATAR: f32 = 96.;
const GAP: Pixels = px(16.);
const CONNECT: Pixels = px(110.);
const PADDING: Pixels = px(24.);

#[derive(Clone, Copy, PartialEq, Eq)]
enum TabLayout {
    Labels,
    Icons,
    Below,
}

fn text_width(text: &str, size: f32, weight: FontWeight, window: &Window, cx: &App) -> Pixels {
    if text.is_empty() {
        return px(0.);
    }
    let font = Font {
        family: cx.theme().font_family.clone(),
        weight,
        ..window.text_style().font()
    };
    let run = TextRun {
        len: text.len(),
        font,
        color: black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(SharedString::from(text.to_string()), px(size), &[run], None)
        .width
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
        let scrolled = self.scroll.offset().y < px(0.);
        v_flex()
            .size_full()
            .child(
                div()
                    .flex_none()
                    .px(PADDING)
                    .pt(PADDING)
                    .pb(px(18.))
                    .border_b_1()
                    .border_color(if scrolled {
                        cx.theme().border.alpha(0.08)
                    } else {
                        transparent_black()
                    })
                    .child(self.header(window, cx)),
            )
            .child(
                div()
                    .id("device-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px(PADDING)
                    .pt(px(18.))
                    .pb(PADDING)
                    .child(body),
            )
    }
}
