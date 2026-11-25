use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use xilo_engine::Snapshot;
use xilo_models::Input;
use xilo_store::Automations;

use super::{Header, choice_row, controls, header, save};
use crate::device_view::DeviceView;
use crate::widgets::{caption, divided, input_color, live_card, segment, segmented};

const LEVELS: [u8; 4] = [10, 20, 30, 50];

pub fn render(snap: &Snapshot, base: &Automations, cx: &mut Context<DeviceView>) -> AnyElement {
    let reserve = &base.reserve;
    let color = input_color(cx);
    let outputs: Vec<_> = snap
        .model
        .as_deref()
        .map(|m| controls(m, snap))
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.input == Input::Switch && !c.id.contains("screen"))
        .collect();
    let soc = snap.int("battery_soc");
    let chosen = outputs
        .iter()
        .filter(|c| reserve.outputs.contains(&c.id))
        .count();
    let status = match soc {
        _ if !reserve.enabled => ("Off".to_string(), cx.theme().muted_foreground),
        _ if chosen == 0 && !outputs.is_empty() => {
            ("Choose an output".to_string(), cx.theme().warning)
        }
        Some(soc) if snap.is_ready() && soc <= i64::from(reserve.level) => (
            format!("Battery {soc}% · At the reserve"),
            cx.theme().warning,
        ),
        Some(soc) if snap.is_ready() => (format!("Watching · Battery {soc}%"), cx.theme().success),
        _ => ("Waiting for the device".to_string(), cx.theme().warning),
    };
    let top = header(
        Header {
            id: "reserve",
            icon: IconName::BatteryMedium,
            color,
            title: "Keep a battery reserve",
            status,
            text: "When the battery drops to the reserve level, Xilo turns off the outputs you choose. \
                   This keeps some charge for when you need it. \
                   If you turn an output on again, Xilo leaves it on until the battery charges above the reserve.",
            enabled: reserve.enabled,
        },
        base,
        |a| a.reserve.enabled = !a.reserve.enabled,
        cx,
    );

    let level = choice_row(
        "Reserve level",
        "reserve-level",
        LEVELS.iter().map(|l| (*l, format!("{l}%"))).collect(),
        reserve.level,
        base,
        |a, v| a.reserve.level = v,
        cx,
    );
    let chips = segmented(cx).children(outputs.iter().map(|c| {
        let selected = reserve.outputs.contains(&c.id);
        let mut next = base.clone();
        if selected {
            next.reserve.outputs.retain(|o| *o != c.id);
        } else {
            next.reserve.outputs.push(c.id.clone());
        }
        // The last chosen output stays, so the reserve always has something to turn off.
        let last = selected && chosen == 1;
        segment(
            SharedString::from(format!("reserve-{}", c.id)),
            c.label.clone(),
            selected,
            cx,
        )
        .when(!last, |chip| {
            chip.on_click(cx.listener(move |view, _, _, cx| save(view, next.clone(), cx)))
        })
    }));
    let turn_off = h_flex()
        .px(px(14.))
        .py(px(12.))
        .gap(px(12.))
        .justify_between()
        .items_center()
        .child(div().text_size(px(15.)).child("Turn off"))
        .child(if outputs.is_empty() {
            caption("This model has no outputs that Xilo can switch.", cx).into_any_element()
        } else {
            chips.into_any_element()
        })
        .into_any_element();

    let rows = reserve.enabled.then_some([level, turn_off]);
    live_card(reserve.enabled, color, cx)
        .p(px(4.))
        .child(divided(
            std::iter::once(top).chain(rows.into_iter().flatten()),
            cx,
        ))
        .into_any_element()
}
