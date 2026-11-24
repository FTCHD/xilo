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
