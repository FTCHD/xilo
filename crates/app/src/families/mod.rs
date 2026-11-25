pub mod c300;
pub mod generic;
pub mod view_model;

use gpui_kit::assets::IconName;
use gpui_kit::*;

use crate::device_view::DeviceView;
use crate::widgets::{asset, battery_avatar};

pub fn dashboard(
    view: &mut DeviceView,
    window: &mut Window,
    cx: &mut Context<DeviceView>,
) -> AnyElement {
    let family = view
        .device
        .read(cx)
        .snap
        .model
        .as_ref()
        .map(|m| m.family.dashboard.clone());
    match family.as_deref() {
        Some("c300" | "c1000") => c300::render(view, window, cx),
        _ => generic::render(view, window, cx),
    }
}

/// The picture in the detail header: the live charge level for models with
/// Anker home images, else the product image.
pub fn avatar(view: &DeviceView, size: f32, cx: &App) -> AnyElement {
    let snap = &view.device.read(cx).snap;
    let Some(model) = snap.model.as_ref() else {
        return battery_avatar(None, None, size, cx);
    };
    match model.family.dashboard.as_str() {
        "c1000" => c300::avatar(
            &view_model::build_c1000(snap, &model.assets, view.blink_on),
            size,
            cx,
        ),
        "c300" => c300::avatar(
            &view_model::build(snap, &model.assets, view.blink_on),
            size,
            cx,
        ),
        _ => {
            let product = format!("{}/product", model.assets);
            if crate::assets::vendor(&product).is_some() {
                asset(&product, px(size), IconName::BatteryCharging, cx)
            } else {
                let soc = snap.is_ready().then(|| snap.int("battery_soc")).flatten();
                battery_avatar(
                    soc.map(|s| s as f32 / 100.),
                    soc.map(|s| format!("{s}%")),
                    size,
                    cx,
                )
            }
        }
    }
}
