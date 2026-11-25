use jiff::civil::{Date, Time};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp, ZonedDateTime};
use xilo_models::{Input, ModelSpec};

/// How long a schedule waits for the device. After this, it counts as missed.
const GRACE: SignedDuration = SignedDuration::from_mins(10);
/// How far back a tick looks for times it did not see, for example while the computer slept.
const LOOKBACK: SignedDuration = SignedDuration::from_hours(7 * 24);

#[derive(Clone, Debug, PartialEq)]
pub struct ScheduleRule {
    pub command: String,
    pub value: i64,
    pub time: Time,
    /// Monday is bit 0, Sunday is bit 6.
    pub days: u8,
    pub enabled: bool,
}

impl ScheduleRule {
    pub fn runs_on(&self, date: Date) -> bool {
        self.days & (1 << date.weekday().to_monday_zero_offset()) != 0
    }

    /// The last time the rule was due after `after` and up to `until`.
    /// A time that a clock change skips counts as the time after the gap.
    fn last_due(&self, tz: &TimeZone, after: Timestamp, until: Timestamp) -> Option<Timestamp> {
        let first = tz.to_datetime(after).date();
        let mut date = tz.to_datetime(until).date();
        loop {
            if self.runs_on(date)
                && let Ok(at) = tz.to_timestamp(date.to_datetime(self.time))
                && at > after
                && at <= until
            {
                return Some(at);
            }
            if date <= first {
                return None;
            }
            date = date.yesterday().ok()?;
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Run(ScheduleRule),
    /// The rule could not run in time: the device was away, or the computer was asleep.
    Missed(ScheduleRule),
}

/// Finds the schedules that are due, in local wall-clock time.
/// Times before the first tick do not run. A rule that is more than 10 minutes late is missed,
/// and is reported once, however many times it was due.
#[derive(Debug, Default)]
pub struct Scheduler {
    rules: Vec<ScheduleRule>,
    checked: Option<Timestamp>,
    pending: Vec<(ScheduleRule, Timestamp)>,
}

impl Scheduler {
    pub fn new(rules: Vec<ScheduleRule>) -> Self {
        Self {
            rules,
            ..Default::default()
        }
    }

    /// Rules that did not change keep waiting for the device.
    pub fn set_rules(&mut self, rules: Vec<ScheduleRule>) {
        if rules != self.rules {
            self.pending.retain(|(rule, _)| rules.contains(rule));
            self.rules = rules;
        }
    }

    pub fn tick(&mut self, now: &ZonedDateTime, ready: bool) -> Vec<Step> {
        let tz = now.time_zone();
        let now = now.timestamp();
        let mut steps = Vec::new();
        if let Some(checked) = self.checked.filter(|c| *c < now) {
            let after = checked.max(now.saturating_sub(LOOKBACK).unwrap_or(checked));
            for rule in self.rules.iter().filter(|r| r.enabled) {
                let Some(at) = rule.last_due(tz, after, now) else {
                    continue;
                };
                if now.duration_since(at) > GRACE {
                    steps.push(Step::Missed(rule.clone()));
                } else if !self.pending.iter().any(|(r, _)| r == rule) {
                    self.pending.push((rule.clone(), at));
                }
            }
        }
        self.checked = Some(self.checked.map_or(now, |c| c.max(now)));

        if ready {
            steps.extend(self.pending.drain(..).map(|(rule, _)| Step::Run(rule)));
            return steps;
        }
        let (late, waiting) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|(_, at)| now.duration_since(*at) > GRACE);
        self.pending = waiting;
        steps.extend(late.into_iter().map(|(rule, _)| Step::Missed(rule)));
        steps
    }
}

/// A command and value in words, for example "AC Output" and "Off".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub label: String,
    pub value: String,
    pub switch: bool,
}

impl Action {
    pub fn new(model: Option<&ModelSpec>, command: &str, value: i64) -> Self {
        let Some(c) = model.and_then(|m| m.command(command)) else {
            return Self {
                label: command.to_string(),
                value: value.to_string(),
                switch: false,
            };
        };
        let switch = c.input == Input::Switch;
        let text = if switch {
            if value == c.on_value { "On" } else { "Off" }.to_string()
        } else {
            c.options
                .iter()
                .find(|o| o.value == value)
                .map(|o| o.label.clone())
                .unwrap_or_else(|| value.to_string())
        };
        Self {
            label: c.label.clone(),
            value: text,
            switch,
        }
    }

    /// For example "turned AC Output off" or "set Light to Low".
    pub fn sentence(&self) -> String {
        if self.switch {
            format!("turned {} {}", self.label, self.value.to_lowercase())
        } else {
            format!("set {} to {}", self.label, self.value)
        }
    }

    /// For example "turn AC Output off" or "set Light to Low", after "could not".
    pub fn command(&self) -> String {
        if self.switch {
            format!("turn {} {}", self.label, self.value.to_lowercase())
        } else {
            format!("set {} to {}", self.label, self.value)
        }
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::{date, time};
    use xilo_models::Registry;

    use super::*;

    const WEEKDAYS: u8 = 0x1f;

    fn rule(h: i8, m: i8, days: u8) -> ScheduleRule {
        ScheduleRule {
            command: "ac_switch".into(),
            value: 0,
            time: time(h, m, 0, 0),
            days,
            enabled: true,
        }
    }

    /// New York: the clocks go forward at 02:00 on 2026-03-08, and back at 02:00 on 2026-11-01.
    fn new_york() -> TimeZone {
        TimeZone::posix("EST5EDT,M3.2.0,M11.1.0").expect("time zone")
    }

    /// 2026-11-23 is a Monday.
    fn at(day: i8, h: i8, m: i8) -> ZonedDateTime {
        date(2026, 11, day)
            .at(h, m, 0, 0)
            .to_zoned_datetime(new_york())
            .expect("local time")
    }

    fn runs(steps: &[Step]) -> usize {
        steps.iter().filter(|s| matches!(s, Step::Run(_))).count()
    }

    fn missed(steps: &[Step]) -> usize {
        steps
            .iter()
            .filter(|s| matches!(s, Step::Missed(_)))
            .count()
    }

    #[test]
    fn runs_once_at_its_time() {
        let mut s = Scheduler::new(vec![rule(23, 0, 0x7f)]);
        assert!(s.tick(&at(23, 22, 58), true).is_empty());
        assert!(s.tick(&at(23, 22, 59), true).is_empty());
        assert_eq!(runs(&s.tick(&at(23, 23, 0), true)), 1);
        assert!(s.tick(&at(23, 23, 1), true).is_empty());
        assert_eq!(runs(&s.tick(&at(24, 23, 0), true)), 1, "again the next day");
    }

    #[test]
    fn follows_the_days() {
        let mut s = Scheduler::new(vec![rule(8, 0, WEEKDAYS)]);
        s.tick(&at(28, 7, 59), true);
        assert!(s.tick(&at(28, 8, 0), true).is_empty(), "Saturday");
        s.tick(&at(30, 7, 59), true);
        assert_eq!(runs(&s.tick(&at(30, 8, 0), true)), 1, "Monday");
    }

    #[test]
    fn crosses_midnight() {
        let mut s = Scheduler::new(vec![rule(0, 0, 0x7f)]);
        s.tick(&at(23, 23, 59), true);
        assert_eq!(runs(&s.tick(&at(24, 0, 0), true)), 1);
    }

    #[test]
    fn waits_for_the_device_then_gives_up() {
        let mut s = Scheduler::new(vec![rule(9, 0, 0x7f), rule(9, 30, 0x7f)]);
        s.tick(&at(23, 8, 59), false);
        assert!(s.tick(&at(23, 9, 0), false).is_empty());
        assert_eq!(runs(&s.tick(&at(23, 9, 5), true)), 1, "device came back");

        assert!(s.tick(&at(23, 9, 30), false).is_empty());
        let steps = s.tick(&at(23, 9, 41), false);
        assert!(matches!(steps.as_slice(), [Step::Missed(_)]));
        assert!(s.tick(&at(23, 9, 42), true).is_empty());
    }

    #[test]
    fn editing_other_rules_keeps_a_waiting_rule() {
        let mut s = Scheduler::new(vec![rule(9, 0, 0x7f)]);
        s.tick(&at(23, 8, 59), false);
        s.tick(&at(23, 9, 0), false);
        s.set_rules(vec![rule(9, 0, 0x7f), rule(12, 0, 0x7f)]);
        assert_eq!(runs(&s.tick(&at(23, 9, 3), true)), 1);
    }

    #[test]
    fn a_changed_rule_stops_waiting() {
        let mut s = Scheduler::new(vec![rule(9, 0, 0x7f)]);
        s.tick(&at(23, 8, 59), false);
        s.tick(&at(23, 9, 0), false);
        s.set_rules(vec![rule(9, 30, 0x7f)]);
        assert!(s.tick(&at(23, 9, 3), true).is_empty());
    }

    #[test]
    fn reports_a_missed_rule_once_after_sleep() {
        let mut s = Scheduler::new(vec![rule(9, 0, 0x7f)]);
        assert!(s.tick(&at(23, 9, 0), true).is_empty(), "first tick");
        s.tick(&at(23, 10, 0), true);
        let steps = s.tick(&at(26, 12, 0), true);
        assert_eq!(
            (runs(&steps), missed(&steps)),
            (0, 1),
            "the computer slept 3 days"
        );
        assert!(s.tick(&at(26, 12, 1), true).is_empty());
    }

    #[test]
    fn does_not_repeat_when_the_clock_goes_back() {
        let mut s = Scheduler::new(vec![rule(9, 0, 0x7f)]);
        s.tick(&at(25, 8, 59), true);
        assert_eq!(runs(&s.tick(&at(25, 9, 0), true)), 1);
        assert!(s.tick(&at(25, 8, 30), true).is_empty(), "clock set back");
        assert!(s.tick(&at(25, 9, 1), true).is_empty());
    }

    #[test]
    fn runs_a_skipped_time_after_the_clocks_go_forward() {
        let tz = new_york();
        let zoned = |h, m, s| {
            date(2026, 3, 8)
                .at(h, m, s, 0)
                .to_zoned_datetime(tz.clone())
                .expect("local time")
        };
        let mut s = Scheduler::new(vec![rule(2, 30, 0x7f)]);
        s.tick(&zoned(1, 59, 50), true);
        assert!(
            s.tick(&zoned(3, 0, 5), true).is_empty(),
            "02:30 is now 03:30"
        );
        assert_eq!(runs(&s.tick(&zoned(3, 30, 0), true)), 1);
        assert!(s.tick(&zoned(3, 31, 0), true).is_empty());
    }

    #[test]
    fn runs_once_when_the_clocks_go_back() {
        let tz = new_york();
        let first = date(2026, 11, 1)
            .at(1, 29, 0, 0)
            .to_zoned_datetime(tz.clone())
            .expect("local time");
        let mut s = Scheduler::new(vec![rule(1, 30, 0x7f)]);
        s.tick(&first, true);
        let mut total = 0;
        for minutes in 1..=90 {
            let now = first.timestamp() + SignedDuration::from_mins(minutes);
            total += runs(&s.tick(&now.to_zoned_datetime(tz.clone()), true));
        }
        assert_eq!(total, 1, "01:30 happens twice");
    }

    #[test]
    fn disabled_rules_do_not_run() {
        let mut off = rule(9, 0, 0x7f);
        off.enabled = false;
        let mut s = Scheduler::new(vec![off]);
        s.tick(&at(23, 8, 59), true);
        assert!(s.tick(&at(23, 9, 0), true).is_empty());
    }

    #[test]
    fn describes_actions() {
        let model = Registry::builtin().get("A1723").map(|m| m.as_ref());
        let off = Action::new(model, "ac_switch", 0);
        assert_eq!(off.sentence(), "turned AC Output off");
        assert_eq!(off.command(), "turn AC Output off");
        assert_eq!(
            Action::new(model, "light_mode", 1).sentence(),
            "set Light to Low"
        );
    }
}
