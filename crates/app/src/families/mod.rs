pub mod c300;
pub mod generic;
pub mod view_model;

use gpui_kit::*;

use crate::device_view::DeviceView;

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
        return crate::widgets::asset(
            "",
            px(size),
            gpui_kit::assets::IconName::BatteryCharging,
            cx,
        );
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
        _ => crate::widgets::asset(
            &format!("{}/product", model.assets),
            px(size),
            gpui_kit::assets::IconName::BatteryCharging,
            cx,
        ),
    }
}
