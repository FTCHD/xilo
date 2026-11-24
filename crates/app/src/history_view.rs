use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use xilo_store::History;

use crate::device::DeviceModel;
use crate::services::unix_now;
use crate::state::AppState;
use crate::widgets::{caption, card, section};

pub fn time_text(ts: i64) -> String {
    Timestamp::from_second(ts)
        .map(|t| {
            t.to_zoned_datetime(TimeZone::system())
                .strftime("%b %-d, %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn day_text(ts: i64) -> String {
    Timestamp::from_second(ts)
        .map(|t| {
            t.to_zoned_datetime(TimeZone::system())
                .strftime("%a %-d")
                .to_string()
        })
        .unwrap_or_default()
}

fn utc_offset() -> i64 {
    Timestamp::from_second(unix_now())
        .map(|t| i64::from(t.to_zoned_datetime(TimeZone::system()).offset().seconds()))
        .unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Range {
    Day,
    Week,
    Month,
}

impl Range {
    fn seconds(self) -> i64 {
        match self {
            Range::Day => 86_400,
            Range::Week => 7 * 86_400,
            Range::Month => 30 * 86_400,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Range::Day => "24 hours",
            Range::Week => "7 days",
            Range::Month => "30 days",
        }
    }
}

#[derive(Default, Clone)]
struct Data {
    soc: Vec<(i64, f64)>,
    input: Vec<(i64, f64)>,
    output: Vec<(i64, f64)>,
    energy_in: Vec<(i64, f64)>,
    energy_out: Vec<(i64, f64)>,
}

pub struct HistoryView {
    device: Entity<DeviceModel>,
    history: Option<Arc<Mutex<History>>>,
    range: Range,
    data: Data,
    _refresh: Task<()>,
}

/// Reduces a series to at most `n` points by averaging buckets.
fn downsample(points: &[(i64, f64)], n: usize) -> Vec<(i64, f64)> {
    if points.len() <= n || n == 0 {
        return points.to_vec();
    }
    points
        .chunks(points.len().div_ceil(n))
        .map(|c| {
            let avg = c.iter().map(|p| p.1).sum::<f64>() / c.len() as f64;
            (c[c.len() / 2].0, avg)
        })
        .collect()
}

impl HistoryView {
    pub fn new(
        device: Entity<DeviceModel>,
        state: Entity<AppState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let history = state.read(cx).history.clone();
        let refresh = cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                if this.update(cx, |view, cx| view.load(cx)).is_err() {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
            }
        });
        Self {
            device,
            history,
            range: Range::Day,
            data: Data::default(),
            _refresh: refresh,
        }
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(history) = &self.history else {
            return;
        };
        let key = self.device.read(cx).key.clone();
        let to = unix_now();
        let from = to - self.range.seconds();
        let Ok(h) = history.lock() else {
            return;
        };
        let series =
            |field: &str| downsample(&h.series(&key, field, from, to).unwrap_or_default(), 240);
        let offset = utc_offset();
        self.data = Data {
            soc: series("battery_soc"),
            input: series("total_input_power"),
            output: series("total_output_power"),
            energy_in: h
                .daily_energy(&key, "total_input_power", from, to, offset)
                .unwrap_or_default(),
            energy_out: h
                .daily_energy(&key, "total_output_power", from, to, offset)
                .unwrap_or_default(),
        };
        cx.notify();
    }

    fn export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(history) = self.history.clone() else {
            return;
        };
        let device = self.device.read(cx);
        let key = device.key.clone();
        let name = device.name().replace(['/', ':'], "-");
        let to = unix_now();
        let from = to - self.range.seconds();
        let dir = xilo_store::download_dir();
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("{name} history.csv")));
        let window = window.window_handle();
        cx.spawn(async move |_, cx: &mut AsyncApp| {
            let Ok(Ok(Some(path))) = rx.await else {
                return;
            };
            let result = (|| -> Result<usize, String> {
                let h = history.lock().map_err(|e| e.to_string())?;
                let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
                h.export_csv(&key, xilo_store::history::RECORDED, from, to, &mut file)
                    .map_err(|e| e.to_string())
            })();
            let note = match result {
                Ok(rows) => Notification::success(format!("Exported {rows} rows.")),
                Err(err) => Notification::error(format!("Export failed: {err}")),
            };
            let _ = cx.update_window(window, |_, window, cx| window.push_notification(note, cx));
        })
        .detach();
    }
}

fn peak_of(series: &[(Vec<(i64, f64)>, Hsla)], max: Option<f64>) -> f64 {
    max.unwrap_or_else(|| {
        series
            .iter()
            .flat_map(|(s, _)| s.iter().map(|p| p.1))
            .fold(10.0_f64, f64::max)
            * 1.1
    })
}

/// A line chart with the top value on the left and the time range below.
fn chart(
    series: Vec<(Vec<(i64, f64)>, Hsla)>,
    from: i64,
    to: i64,
    max: Option<f64>,
    unit: &str,
    height: Pixels,
    cx: &App,
) -> Div {
    let peak = peak_of(&series, max);
    h_flex()
        .gap(px(8.))
        .child(
            v_flex()
                .h(height)
                .w(px(44.))
                .justify_between()
                .child(caption(format!("{peak:.0}{unit}"), cx).text_size(px(11.)))
                .child(caption(format!("0{unit}"), cx).text_size(px(11.))),
        )
        .child(
            v_flex()
                .flex_1()
                .gap(px(4.))
                .child(
                    div()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .child(line_chart(series, from, to, Some(peak), height)),
                )
                .child(
                    h_flex()
                        .justify_between()
                        .child(caption(time_text(from), cx).text_size(px(11.)))
                        .child(caption(time_text(to), cx).text_size(px(11.))),
                ),
        )
}

fn line_chart(
    series: Vec<(Vec<(i64, f64)>, Hsla)>,
    from: i64,
    to: i64,
    max: Option<f64>,
    height: Pixels,
) -> AnyElement {
    canvas(
        move |_, _, _| series,
        move |bounds, series, window, _| {
            let peak = peak_of(&series, max);
            let w = f32::from(bounds.size.width);
            let h = f32::from(bounds.size.height);
            let span = (to - from).max(1) as f32;
            for (points, color) in series {
                let mut path = PathBuilder::stroke(px(2.));
                let mut started = false;
                let mut last_ts = i64::MIN;
                for (ts, v) in points {
                    let x = bounds.origin.x + px((ts - from) as f32 / span * w);
                    let y = bounds.origin.y + px(h - (v / peak) as f32 * h);
                    if !started || ts - last_ts > 600 {
                        path.move_to(point(x, y));
                        started = true;
                    } else {
                        path.line_to(point(x, y));
                    }
                    last_ts = ts;
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
            }
        },
    )
    .w_full()
    .h(height)
    .into_any_element()
}

fn bars(
    energy_in: &[(i64, f64)],
    energy_out: &[(i64, f64)],
    input: Hsla,
    output: Hsla,
    cx: &App,
) -> AnyElement {
    let mut days: Vec<i64> = energy_in.iter().chain(energy_out).map(|d| d.0).collect();
    days.sort_unstable();
    days.dedup();
    let value =
        |list: &[(i64, f64)], day: i64| list.iter().find(|d| d.0 == day).map_or(0.0, |d| d.1);
    let peak = energy_in
        .iter()
        .chain(energy_out)
        .map(|d| d.1)
        .fold(1.0_f64, f64::max);
    let bar = |wh: f64, color: Hsla| {
        div()
            .w(px(12.))
            .h(px((wh / peak * 120.0) as f32))
            .rounded_t(px(4.))
            .bg(color)
    };
    h_flex()
        .gap(px(12.))
        .items_end()
        .h(px(170.))
        .children(days.into_iter().map(|day| {
            let (i, o) = (value(energy_in, day), value(energy_out, day));
            v_flex()
                .items_center()
                .gap(px(4.))
                .child(
                    h_flex()
                        .gap(px(2.))
                        .items_end()
                        .child(bar(i, input))
                        .child(bar(o, output)),
                )
                .child(caption(day_text(day), cx))
                .child(caption(format!("{:.0}/{:.0} Wh", i, o), cx).text_size(px(10.)))
        }))
        .into_any_element()
}

fn legend(items: &[(&'static str, Hsla)], cx: &App) -> Div {
    h_flex()
        .gap(px(14.))
        .children(items.iter().map(|(label, color)| {
            h_flex()
                .gap(px(6.))
                .items_center()
                .child(div().size(px(10.)).rounded(px(3.)).bg(*color))
                .child(caption(*label, cx))
        }))
}

impl Render for HistoryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.history.is_none() {
            return card(cx)
                .child(caption(
                    "History is not available because the database could not be opened.",
                    cx,
                ))
                .into_any_element();
        }
        let to = unix_now();
        let from = to - self.range.seconds();
        let brand = cx.theme().primary;
        let input = cx.theme().success;
        let output = cx.theme().warning;
        let empty = self.data.soc.is_empty() && self.data.output.is_empty();
        let current = self.range;
        let ranges =
            h_flex()
                .gap(px(6.))
                .children(
                    [Range::Day, Range::Week, Range::Month]
                        .into_iter()
                        .map(|r| {
                            Button::new(SharedString::from(r.label()))
                                .small()
                                .when(r == current, |b| b.primary())
                                .when(r != current, |b| b.ghost())
                                .label(r.label())
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.range = r;
                                    view.load(cx);
                                }))
                        }),
                );
        v_flex()
            .gap(px(14.))
            .child(
                h_flex()
                    .justify_between()
                    .child(ranges)
                    .child(
                        Button::new("export")
                            .small()
                            .label("Export CSV")
                            .on_click(cx.listener(|view, _, window, cx| view.export(window, cx))),
                    ),
            )
            .when(empty, |el| {
                el.child(card(cx).child(caption(
                    "No history yet. Xilo records values every 10 seconds while the device is connected.",
                    cx,
                )))
            })
            .when(!empty, |el| {
                el.child(section("Battery", cx))
                    .child(card(cx).child(chart(vec![(self.data.soc.clone(), brand)], from, to, Some(100.0), "%", px(140.), cx)))
                    .child(section("Power", cx))
                    .child(
                        card(cx).child(
                            v_flex()
                                .gap(px(8.))
                                .child(legend(&[("Input", input), ("Output", output)], cx))
                                .child(chart(
                                    vec![(self.data.input.clone(), input), (self.data.output.clone(), output)],
                                    from,
                                    to,
                                    None,
                                    " W",
                                    px(160.),
                                    cx,
                                )),
                        ),
                    )
                    .child(section("Energy per Day", cx))
                    .child(
                        card(cx).child(
                            v_flex()
                                .gap(px(8.))
                                .child(legend(&[("In", input), ("Out", output)], cx))
                                .child(bars(&self.data.energy_in, &self.data.energy_out, input, output, cx)),
                        ),
                    )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn downsample_keeps_shape() {
        let points: Vec<(i64, f64)> = (0..1000).map(|i| (i, 5.0)).collect();
        let out = super::downsample(&points, 100);
        assert!(out.len() <= 100);
        assert!(out.iter().all(|p| (p.1 - 5.0).abs() < f64::EPSILON));
    }
}
