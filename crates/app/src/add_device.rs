use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_engine::Found;

use crate::runtime::Runtime;
use crate::state::AppState;
use crate::widgets::{asset, caption, card, section};

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

    fn guess(&self, found: &Found, cx: &App) -> Option<(String, String)> {
        let data = found.advert.manufacturer_data.get(&SOLIX_COMPANY)?;
        let registry = Runtime::get(cx).engine.registry().clone();
        let models = registry.guess_from_advert(data);
        let first = models.first()?;
        let names: Vec<String> = models.iter().map(|m| m.name.clone()).collect();
        Some((first.assets.clone(), names.join(" or ")))
    }
}

impl Render for AddDevice {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<AnyElement> = self
            .found
            .iter()
            .filter_map(|f| {
                let addr = f.advert.addr.clone()?;
                let name = f
                    .advert
                    .name
                    .clone()
                    .unwrap_or_else(|| "SOLIX device".into());
                let guess = self.guess(f, cx);
                let assets = guess.as_ref().map(|g| g.0.clone()).unwrap_or_default();
                let hint = guess.map(|g| g.1).unwrap_or_else(|| addr.id.clone());
                let known = f.known;
                let rssi = f
                    .advert
                    .rssi
                    .map(|r| format!("{r} dBm"))
                    .unwrap_or_default();
                let state = self.state.clone();
                let id = addr.id.clone();
                let label = name.clone();
                Some(
                    card(cx)
                        .child(
                            h_flex()
                                .gap(px(14.))
                                .items_center()
                                .child(asset(
                                    &format!("{assets}/product"),
                                    px(52.),
                                    IconName::BatteryCharging,
                                    cx,
                                ))
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .child(
                                            div()
                                                .text_size(px(16.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(name),
                                        )
                                        .child(caption(hint, cx)),
                                )
                                .child(caption(rssi, cx))
                                .child(
                                    Button::new(SharedString::from(format!("add-{id}")))
                                        .small()
                                        .when(!known, |b| b.primary())
                                        .label(if known { "Open" } else { "Add" })
                                        .on_click(move |_, _, cx| {
                                            let id = id.clone();
                                            let label = label.clone();
                                            state.update(cx, |s, cx| {
                                                s.add_device(id, Some(label), cx)
                                            });
                                        }),
                                ),
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
            .child(div().text_size(px(22.)).font_weight(FontWeight::BOLD).child("Add a Device"))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(section(if self.scanning { "Looking for devices..." } else { "Nearby" }, cx))
                    .child(
                        Button::new("rescan")
                            .small()
                            .icon(Icon::new(IconName::RefreshCw))
                            .label("Scan Again")
                            .loading(self.scanning)
                            .on_click(cx.listener(|view, _, _, cx| view.scan(cx))),
                    ),
            )
            .when_some(self.error.clone(), |el, err| {
                el.child(
                    card(cx)
                        .border_1()
                        .border_color(cx.theme().danger)
                        .child(div().text_size(px(14.)).child(err)),
                )
            })
            .children(rows)
            .when(empty && self.scanned && !self.scanning, |el| {
                el.child(card(cx).child(
                    v_flex()
                        .gap(px(8.))
                        .child(div().text_size(px(15.)).child("No SOLIX devices found."))
                        .child(caption("• Turn the device on and keep it near this Mac.", cx))
                        .child(caption(
                            "• A device that is connected to the Anker app on a phone does not advertise. Close the app or turn off Bluetooth on the phone.",
                            cx,
                        ))
                        .child(caption(
                            "• Some devices stop advertising after they are set up for Wi-Fi. Press the IoT button on the device to make it visible again.",
                            cx,
                        ))
                        .child(caption("• Check that Xilo may use Bluetooth in System Settings > Privacy & Security.", cx)),
                ))
            })
    }
}
