use std::time::Duration;

use gpui_kit::*;
use gpui_tray::{Icon, Tray};
use xilo_models::CommandRequest;

use crate::runtime::Runtime;
use crate::state::AppState;
use crate::{Quit, ShowWindow};

#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = xilo, no_json)]
pub struct ToggleOutput {
    pub key: String,
    pub command: String,
    pub on: bool,
}

#[derive(Clone, PartialEq, Debug, Default, Action)]
#[action(namespace = xilo, no_json)]
pub struct ReleaseAll;

/// Where the Xilo icon is: the menu bar on macOS, the system tray on Windows and Linux.
pub const PLACE: &str = if cfg!(target_os = "macos") {
    "menu bar"
} else {
    "system tray"
};

struct TrayHandle(Tray);

impl Global for TrayHandle {}

fn icon() -> Option<Icon> {
    #[cfg(target_os = "macos")]
    {
        Icon::system_symbol("bolt.batteryblock")
            .accessibility_description("Xilo")
            .build()
            .ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let size = 32u32;
        let mut rgba = vec![0u8; (size * size * 4) as usize];
        for y in 6..26 {
            for x in 4..28 {
                let i = ((y * size + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[0x00, 0xcb, 0xeb, 0xff]);
            }
        }
        Icon::from_rgba(rgba, size, size).build().ok()
    }
}

fn menu(state: &Entity<AppState>, cx: &mut App) -> Vec<MenuItem> {
    let mut items = Vec::new();
    for device in state.read(cx).entities() {
        let d = device.read(cx);
        let snap = &d.snap;
        let line = match (snap.is_ready(), snap.int("battery_soc")) {
            (true, Some(soc)) => format!(
                "{} · {soc}% · {} W out",
                d.name(),
                snap.int("total_output_power").unwrap_or(0)
            ),
            _ => format!("{} · {}", d.name(), snap.link.label()),
        };
        items.push(MenuItem::action(line, ShowWindow));
        if snap.is_ready() {
            for (command, label) in [("ac_switch", "AC Output"), ("dc_switch", "Car Output")] {
                let has = snap
                    .model
                    .as_ref()
                    .is_some_and(|m| m.command(command).is_some());
                if !has {
                    continue;
                }
                let on = snap.flag(command);
                items.push(
                    MenuItem::action(
                        format!("    {label}"),
                        ToggleOutput {
                            key: d.key.clone(),
                            command: command.into(),
                            on: !on,
                        },
                    )
                    .checked(on),
                );
            }
        }
    }
    if !items.is_empty() {
        items.push(MenuItem::separator());
    }
    items.push(MenuItem::action("Show Xilo", ShowWindow));
    items.push(MenuItem::action("Release Devices to Phone", ReleaseAll));
    items.push(MenuItem::separator());
    items.push(MenuItem::action("Quit Xilo", Quit));
    items
}

fn title(state: &Entity<AppState>, cx: &App) -> Option<String> {
    let socs: Vec<i64> = state
        .read(cx)
        .entities()
        .filter_map(|d| {
            let snap = &d.read(cx).snap;
            snap.is_ready().then(|| snap.int("battery_soc")).flatten()
        })
        .collect();
    match socs.as_slice() {
        [] => None,
        [one] => Some(format!("{one}%")),
        many => Some(format!("{}%", many.iter().min().copied().unwrap_or(0))),
    }
}

pub fn init(state: Entity<AppState>, cx: &mut App) {
    let for_menu = state.clone();
    let mut builder = Tray::builder()
        .tooltip("Xilo")
        .menu(move |cx| menu(&for_menu, cx));
    if let Some(icon) = icon() {
        builder = builder.icon(icon);
    }
    let tray = match builder.build(cx) {
        Ok(tray) => tray,
        Err(err) => {
            tracing::warn!("{PLACE} icon unavailable: {err}");
            return;
        }
    };
    cx.set_global(TrayHandle(tray));

    let toggle_state = state.clone();
    cx.on_action(move |action: &ToggleOutput, cx| {
        let Some(device) = toggle_state.read(cx).device(&action.key) else {
            return;
        };
        let handle = device.read(cx).handle.clone();
        let request = CommandRequest::new(action.command.clone(), Some(i64::from(action.on)));
        Runtime::spawn(cx, async move {
            if let Err(err) = handle.send(request).await {
                tracing::warn!("{PLACE} command failed: {err}");
            }
        });
    });
    let release_state = state.clone();
    cx.on_action(move |_: &ReleaseAll, cx| {
        for device in release_state.read(cx).entities() {
            device.read(cx).release(cx);
        }
    });

    cx.spawn(async move |cx: &mut AsyncApp| {
        let mut last: Option<String> = None;
        loop {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let alive = cx.update(|cx| {
                let text = title(&state, cx);
                let Some(tray) = cx.try_global::<TrayHandle>().map(|t| t.0.clone()) else {
                    return false;
                };
                if text != last {
                    let _ = tray.set_title(text.clone(), cx);
                    last = text;
                }
                let _ = tray.refresh_menu(cx);
                true
            });
            if !alive {
                break;
            }
        }
    })
    .detach();
}
