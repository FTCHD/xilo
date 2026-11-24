use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use xilo_engine::Snapshot;
use xilo_engine::automations::usb_watts;
use xilo_store::Automations;

use super::{Header, choice_row, header};
use crate::device_view::DeviceView;
use crate::widgets::{divided, live_card, output_color};

const IDLE_CHOICES: [u32; 4] = [60, 80, 100, 110];
const BATTERY_CHOICES: [u8; 4] = [5, 10, 20, 30];

pub fn render(snap: &Snapshot, base: &Automations, cx: &mut Context<DeviceView>) -> AnyElement {
    let wake = &base.usb_wake;
    let status = if !wake.enabled {
        ("Off".to_string(), cx.theme().muted_foreground)
    } else if !snap.is_ready() {
        ("Waiting for the device".to_string(), cx.theme().warning)
    } else {
        (
            format!("Watching · USB load {:.0} W", usb_watts(snap)),
            cx.theme().success,
        )
    };
    let top = header(
        Header {
            id: "usb-wake",
            icon: IconName::Usb,
            color: output_color(cx),
            title: "Keep USB ports on",
            status,
            text: "The device turns its USB ports off after about two hours of low load. \
                   Xilo turns the car output on briefly before that happens, which keeps the ports on. \
                   If the car output was off, Xilo turns it off again 3 seconds later.",
            enabled: wake.enabled,
        },
        base,
        |a| a.usb_wake.enabled = !a.usb_wake.enabled,
        cx,
    );
    let rows = wake.enabled.then(|| {
        [
            choice_row(
                "Wake after idle",
                "idle",
                IDLE_CHOICES
                    .iter()
                    .map(|m| (*m, format!("{m} min")))
                    .collect(),
                wake.idle_minutes,
                base,
                |a, v| a.usb_wake.idle_minutes = v,
                cx,
            ),
            choice_row(
                "Skip below battery",
                "wake-battery",
                BATTERY_CHOICES
                    .iter()
                    .map(|b| (*b, format!("{b}%")))
                    .collect(),
                wake.min_battery,
                base,
                |a, v| a.usb_wake.min_battery = v,
                cx,
            ),
        ]
    });
    live_card(wake.enabled, output_color(cx), cx)
        .p(px(4.))
        .child(divided(
            std::iter::once(top).chain(rows.into_iter().flatten()),
            cx,
        ))
        .into_any_element()
}
