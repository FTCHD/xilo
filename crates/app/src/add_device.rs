use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_engine::Found;

use crate::runtime::Runtime;
use crate::state::AppState;
use crate::widgets::{asset, caption, card, icon_badge, live_card, pill, section};

const SCAN_WINDOW: Duration = Duration::from_secs(6);
const SOLIX_COMPANY: u16 = 0xffff;

pub struct AddDevice {
    state: Entity<AppState>,
    scanning: bool,
    found: Vec<Found>,
    error: Option<String>,
    scanned: bool,
}

impl AddDevice {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            state,
            scanning: false,
            found: Vec::new(),
            error: Runtime::get(cx).bluetooth_error.clone(),
            scanned: false,
        };
        if view.error.is_none() {
            view.scan(cx);
        }
        view
    }

    fn scan(&mut self, cx: &mut Context<Self>) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        self.error = None;
        cx.notify();
        let engine = Runtime::get(cx).engine.clone();
        let rx = Runtime::run(cx, async move { engine.scan(SCAN_WINDOW).await });
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = rx.await;
            let _ = this.update(cx, |view, cx| {
                view.scanning = false;
                view.scanned = true;
                match result {
                    Ok(Ok(found)) => view.found = found,
                    Ok(Err(err)) => view.error = Some(err.to_string()),
                    Err(_) => view.error = Some("The scan stopped unexpectedly.".into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The likely model: from the manufacturer data, or else from the advertised name.
    fn guess(&self, found: &Found, cx: &App) -> Option<(String, String)> {
        let registry = Runtime::get(cx).engine.registry().clone();
        if let Some(data) = found.advert.manufacturer_data.get(&SOLIX_COMPANY) {
            let models = registry.guess_from_advert(data);
            if let Some(first) = models.first() {
                let names: Vec<String> = models.iter().map(|m| m.name.clone()).collect();
                return Some((first.assets.clone(), names.join(" or ")));
            }
        }
        let name = found.advert.name.as_deref()?.trim_start_matches("Anker ");
        let model = registry.all().find(|m| m.name.eq_ignore_ascii_case(name))?;
        Some((model.assets.clone(), model.code.clone()))
    }
}

impl Render for AddDevice {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut found: Vec<&Found> = self.found.iter().collect();
        found.sort_by_key(|f| (f.known, f.advert.name.clone().unwrap_or_default()));
        let rows: Vec<AnyElement> = found
            .into_iter()
            .filter_map(|f| {
                let addr = f.advert.addr.clone()?;
                let name = f
                    .advert
                    .name
                    .clone()
                    .unwrap_or_else(|| "SOLIX device".into());
                let guess = self.guess(f, cx);
                let assets = guess.as_ref().map(|g| g.0.clone()).unwrap_or_default();
                let hint = guess.map(|g| g.1).unwrap_or_else(|| "Unknown model".into());
                let known = f.known;
                let state = self.state.clone();
                let id = addr.id.clone();
                let label = name.clone();
                Some(
                    live_card(!known, cx.theme().primary, cx)
                        .p(px(14.))
                        .child(
                            h_flex()
                                .gap(px(16.))
                                .items_center()
                                .child(asset(
                                    &format!("{assets}/product"),
                                    px(64.),
                                    IconName::BatteryCharging,
                                    cx,
                                ))
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .gap(px(2.))
                                        .child(
                                            div()
                                                .text_size(px(16.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(name),
                                        )
                                        .child(caption(hint, cx)),
                                )
                                .child(signal(f.advert.rssi, cx))
                                .child(if known {
                                    pill("Added", cx.theme().muted_foreground).into_any_element()
                                } else {
                                    Button::new(SharedString::from(format!("add-{id}")))
                                        .primary()
                                        .icon(Icon::new(IconName::Plus))
                                        .label("Add")
                                        .on_click(move |_, _, cx| {
                                            let id = id.clone();
                                            let label = label.clone();
                                            state.update(cx, |s, cx| {
                                                s.add_device(id, Some(label), cx)
                                            });
                                        })
                                        .into_any_element()
                                }),
                        )
                        .into_any_element(),
                )
            })
            .collect();
        let empty = rows.is_empty();
        v_flex()
            .size_full()
            .p(px(24.))
            .gap(px(16.))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(26.))
                            .font_weight(FontWeight::BOLD)
                            .child("Add a Device"),
                    )
                    .child(
                        Button::new("rescan")
                            .icon(Icon::new(IconName::RefreshCw))
                            .label("Scan Again")
                            .loading(self.scanning)
                            .on_click(cx.listener(|view, _, _, cx| view.scan(cx))),
                    ),
            )
            .child(section("Nearby", cx))
            .when_some(self.error.clone(), |el, err| {
                el.child(
                    live_card(true, cx.theme().danger, cx).child(
                        h_flex()
                            .gap(px(12.))
                            .items_center()
                            .child(icon_badge(IconName::BluetoothOff, cx.theme().danger, px(36.)))
                            .child(div().flex_1().text_size(px(14.)).child(err)),
                    ),
                )
            })
            .children(rows)
            .when(empty && self.scanned && !self.scanning, |el| {
                el.child(card(cx).child(
                    v_flex()
                        .gap(px(10.))
                        .child(
                            h_flex()
                                .gap(px(12.))
                                .items_center()
                                .child(icon_badge(IconName::Bluetooth, cx.theme().muted_foreground, px(36.)))
                                .child(
                                    div()
                                        .text_size(px(15.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("No SOLIX devices found"),
                                ),
                        )
                        .child(caption("• Turn the device on and keep it near this Mac.", cx))
                        .child(caption(
                            "• A device that is connected to the Anker app on a phone does not advertise. Close the app or turn off Bluetooth on the phone.",
                            cx,
                        ))
                        .child(caption(
                            "• Some devices stop advertising after they are set up for Wi-Fi. Press the IoT button on the device to make it visible again.",
                            cx,
                        ))
                        .child(caption(
                            "• Check that Xilo may use Bluetooth in System Settings > Privacy & Security.",
                            cx,
                        )),
                ))
            })
    }
}

/// Signal strength as four bars.
fn signal(rssi: Option<i16>, cx: &App) -> AnyElement {
    let Some(rssi) = rssi else {
        return div().into_any_element();
    };
    let level = match rssi {
        -60.. => 4,
        -70..=-61 => 3,
        -80..=-71 => 2,
        _ => 1,
    };
    let color = if level >= 3 {
        cx.theme().success
    } else if level == 2 {
        cx.theme().warning
    } else {
        cx.theme().danger
    };
    h_flex()
        .gap(px(2.))
        .items_end()
        .h(px(16.))
        .children((1..=4).map(|i| {
            div()
                .w(px(4.))
                .h(px(4. * i as f32))
                .rounded(px(1.))
                .bg(if i <= level { color } else { cx.theme().muted })
        }))
        .into_any_element()
}
