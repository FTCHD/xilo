use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::time_field::{TimeField, TimeFieldState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use jiff::civil::Time;
use xilo_engine::Snapshot;
use xilo_engine::automations::Action;
use xilo_models::{CommandDef, Input, ModelSpec};
use xilo_store::{Automations, EVERY_DAY, Schedule};

use super::{controls, save};
use crate::device_view::DeviceView;
use crate::families::generic::control_icon;
use crate::widgets::{
    caption, card, divided, icon_badge, live_card, output_color, section, segment, segmented,
};

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WEEKDAYS: u8 = 0x1f;
const WEEKEND: u8 = 0x60;

/// A schedule that is being added or changed.
pub struct Editor {
    index: Option<usize>,
    command: String,
    value: i64,
    days: u8,
    time: Entity<TimeFieldState>,
}

fn days_text(days: u8) -> String {
    match days & EVERY_DAY {
        EVERY_DAY => "Every day".into(),
        WEEKDAYS => "Weekdays".into(),
        WEEKEND => "Weekends".into(),
        d => DAYS
            .iter()
            .enumerate()
            .filter(|(i, _)| d & (1 << i) != 0)
            .map(|(_, name)| *name)
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// For example "Turn AC Output off" or "Set Light to Low".
fn instruction(action: &Action) -> String {
    if action.switch {
        format!("Turn {} {}", action.label, action.value.to_lowercase())
    } else {
        format!("Set {} to {}", action.label, action.value)
    }
}

fn values(c: &CommandDef) -> Vec<(i64, String)> {
    match c.input {
        Input::Switch => vec![(c.on_value, "On".into()), (c.off_value, "Off".into())],
        _ => c
            .options
            .iter()
            .map(|o| (o.value, o.label.clone()))
            .collect(),
    }
}

fn local_hour() -> u8 {
    jiff::Timestamp::now()
        .to_zoned_datetime(jiff::tz::TimeZone::system())
        .hour() as u8
}

/// Opens the editor for a new schedule with the first control, at the next full hour.
pub fn add(view: &mut DeviceView, window: &mut Window, cx: &mut Context<DeviceView>) {
    let snap = view.device.read(cx).snap.clone();
    let Some(first) = snap
        .model
        .as_deref()
        .and_then(|m| controls(m, &snap).into_iter().next())
    else {
        return;
    };
    let schedule = Schedule {
        value: values(&first).first().map_or(0, |v| v.0),
        command: first.id,
        hour: (local_hour() + 1) % 24,
        minute: 0,
        days: EVERY_DAY,
        enabled: true,
    };
    open(view, None, schedule, window, cx);
}

fn open(
    view: &mut DeviceView,
    index: Option<usize>,
    schedule: Schedule,
    window: &mut Window,
    cx: &mut Context<DeviceView>,
) {
    let at = Time::new(schedule.hour as i8, schedule.minute as i8, 0, 0).unwrap_or_default();
    let time = cx.new(|cx| TimeFieldState::new(window, cx));
    time.update(cx, |t, cx| t.set_time(at, window, cx));
    view.schedule_editor = Some(Editor {
        index,
        command: schedule.command,
        value: schedule.value,
        days: schedule.days,
        time,
    });
    cx.notify();
}

fn edit(view: &mut DeviceView, cx: &mut Context<DeviceView>, change: impl FnOnce(&mut Editor)) {
    if let Some(editor) = view.schedule_editor.as_mut() {
        change(editor);
        cx.notify();
    }
}

fn commit(view: &mut DeviceView, mut next: Automations, cx: &mut Context<DeviceView>) {
    let Some(editor) = view.schedule_editor.take() else {
        return;
    };
    let time = editor.time.read(cx).time();
    let schedule = Schedule {
        command: editor.command,
        value: editor.value,
        hour: time.hour() as u8,
        minute: time.minute() as u8,
        days: editor.days,
        enabled: true,
    };
    match editor.index {
        Some(i) if i < next.schedules.len() => next.schedules[i] = schedule,
        _ => next.schedules.push(schedule),
    }
    next.schedules.sort_by_key(|s| (s.hour, s.minute));
    save(view, next, cx);
}

pub fn render(
    view: &mut DeviceView,
    snap: &Snapshot,
    base: &Automations,
    _window: &mut Window,
    cx: &mut Context<DeviceView>,
) -> AnyElement {
    let model = snap.model.as_deref();
    let options = model.map(|m| controls(m, snap)).unwrap_or_default();
    let editing = view.schedule_editor.is_some();

    let add = (!options.is_empty() && !editing).then(|| {
        Button::new("add-schedule")
            .small()
            .icon(Icon::new(IconName::Plus))
            .label("Add Schedule")
            .on_click(cx.listener(|view, _, window, cx| add(view, window, cx)))
    });
    let title = h_flex()
        .justify_between()
        .items_center()
        .child(section("Schedules", cx))
        .children(add);

    let rows: Vec<_> = base
        .schedules
        .iter()
        .enumerate()
        .map(|(i, s)| row(i, s, model, base, cx))
        .collect();
    let list = (!rows.is_empty()).then(|| card(cx).p(px(4.)).child(divided(rows, cx)));
    let empty = (base.schedules.is_empty() && !editing).then(|| {
        card(cx).child(caption(
            if options.is_empty() {
                "This model has no controls that a schedule can use."
            } else {
                "Turn outputs on or off at set times, for example the AC output off at night. \
                 Schedules run while Xilo is open or in the menu bar, and the device is in Bluetooth range."
            },
            cx,
        ))
    });
    let editor = view
        .schedule_editor
        .as_ref()
        .map(|e| editor(e, &options, base, cx));

    v_flex()
        .gap(px(14.))
        .child(title)
        .children(editor)
        .children(list)
        .children(empty)
        .into_any_element()
}

fn row(
    index: usize,
    s: &Schedule,
    model: Option<&ModelSpec>,
    base: &Automations,
    cx: &mut Context<DeviceView>,
) -> AnyElement {
    let action = Action::new(model, &s.command, s.value);
    let muted = cx.theme().muted_foreground;
    let mut toggled = base.clone();
    toggled.schedules[index].enabled = !s.enabled;
    let mut removed = base.clone();
    removed.schedules.remove(index);
    let schedule = s.clone();
    h_flex()
        .px(px(14.))
        .py(px(12.))
        .gap(px(14.))
        .items_center()
        .child(icon_badge(
            control_icon(&s.command),
            if s.enabled { output_color(cx) } else { muted },
            px(36.),
        ))
        .child(
            div()
                .w(px(64.))
                .flex_none()
                .text_size(px(20.))
                .font_weight(FontWeight::BOLD)
                .when(!s.enabled, |el| el.text_color(muted))
                .child(format!("{:02}:{:02}", s.hour, s.minute)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(15.))
                        .when(!s.enabled, |el| el.text_color(muted))
                        .child(instruction(&action)),
                )
                .child(caption(days_text(s.days), cx)),
        )
        .child(
            Button::new(("edit-schedule", index))
                .ghost()
                .small()
                .icon(Icon::new(IconName::Pencil))
                .on_click(cx.listener(move |view, _, window, cx| {
                    open(view, Some(index), schedule.clone(), window, cx)
                })),
        )
        .child(
            Button::new(("remove-schedule", index))
                .ghost()
                .small()
                .icon(Icon::new(IconName::Trash))
                .on_click(cx.listener(move |view, _, _, cx| save(view, removed.clone(), cx))),
        )
        .child(
            Switch::new(("schedule", index))
                .checked(s.enabled)
                .on_click(
                    cx.listener(move |view, _: &bool, _, cx| save(view, toggled.clone(), cx)),
                ),
        )
        .into_any_element()
}

fn field(label: &'static str, control: impl IntoElement) -> AnyElement {
    h_flex()
        .px(px(14.))
        .py(px(12.))
        .gap(px(12.))
        .justify_between()
        .items_center()
        .child(div().text_size(px(15.)).child(label))
        .child(control)
        .into_any_element()
}

fn editor(
    e: &Editor,
    options: &[CommandDef],
    base: &Automations,
    cx: &mut Context<DeviceView>,
) -> Div {
    let command = options
        .iter()
        .find(|c| c.id == e.command)
        .or(options.first());
    let controls = segmented(cx).children(options.iter().enumerate().map(|(i, c)| {
        let id = c.id.clone();
        let value = values(c).first().map_or(0, |v| v.0);
        segment(
            ("schedule-control", i),
            c.label.clone(),
            c.id == e.command,
            cx,
        )
        .on_click(cx.listener(move |view, _, _, cx| {
            let id = id.clone();
            edit(view, cx, |e| {
                e.command = id;
                e.value = value;
            })
        }))
    }));
    let choices = segmented(cx).children(
        command
            .map(values)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, (value, label))| {
                segment(("schedule-value", i), label, value == e.value, cx).on_click(
                    cx.listener(move |view, _, _, cx| edit(view, cx, |e| e.value = value)),
                )
            }),
    );
    let days = segmented(cx).children(DAYS.iter().enumerate().map(|(i, name)| {
        let bit = 1u8 << i;
        segment(("schedule-day", i), *name, e.days & bit != 0, cx)
            .on_click(cx.listener(move |view, _, _, cx| edit(view, cx, |e| e.days ^= bit)))
    }));
    let switch = command.is_some_and(|c| c.input == Input::Switch);
    let next = base.clone();
    let footer = h_flex()
        .px(px(14.))
        .py(px(12.))
        .gap(px(8.))
        .justify_end()
        .child(
            Button::new("cancel-schedule")
                .ghost()
                .label("Cancel")
                .on_click(cx.listener(|view, _, _, cx| {
                    view.schedule_editor = None;
                    cx.notify();
                })),
        )
        .child(
            Button::new("save-schedule")
                .primary()
                .label(if e.index.is_some() { "Save" } else { "Add" })
                .disabled(e.days == 0)
                .on_click(cx.listener(move |view, _, _, cx| commit(view, next.clone(), cx))),
        );
    live_card(true, cx.theme().primary, cx).p(px(4.)).child(
        v_flex()
            .child(divided(
                [
                    field("Control", controls),
                    field(if switch { "Turn" } else { "Set to" }, choices),
                    field("Time", TimeField::new(&e.time)),
                    field("Days", days),
                ],
                cx,
            ))
            .child(footer),
    )
}
