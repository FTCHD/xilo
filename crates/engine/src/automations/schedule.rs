use jiff::SignedDuration;
use jiff::civil::{Date, DateTime, Time};
use xilo_models::{Input, ModelSpec};

/// How long a schedule waits for the device. After this, it counts as missed.
const GRACE: SignedDuration = SignedDuration::from_mins(10);

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
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Run(ScheduleRule),
    Missed(ScheduleRule),
}

/// Finds the schedules that are due, in local wall-clock time.
/// Times before the first tick, or more than 10 minutes in the past, do not run.
#[derive(Debug, Default)]
pub struct Scheduler {
    rules: Vec<ScheduleRule>,
    checked: Option<DateTime>,
    pending: Vec<(ScheduleRule, DateTime)>,
}

impl Scheduler {
    pub fn new(rules: Vec<ScheduleRule>) -> Self {
        Self {
            rules,
            ..Default::default()
        }
    }

    pub fn set_rules(&mut self, rules: Vec<ScheduleRule>) {
        if rules != self.rules {
            self.rules = rules;
            self.pending.clear();
        }
    }

    pub fn tick(&mut self, now: DateTime, ready: bool) -> Vec<Step> {
        if let Some(checked) = self.checked.filter(|c| *c < now) {
            let start = checked.max(now.saturating_sub(GRACE));
            for rule in self.rules.iter().filter(|r| r.enabled) {
                let mut date = start.date();
                while date <= now.date() {
                    let at = date.to_datetime(rule.time);
                    if at > start && at <= now && rule.runs_on(date) {
                        self.pending.push((rule.clone(), at));
                    }
                    let Ok(next) = date.tomorrow() else { break };
                    date = next;
                }
            }
        }
        self.checked = Some(self.checked.map_or(now, |c| c.max(now)));

        if ready {
            return self
                .pending
                .drain(..)
                .map(|(rule, _)| Step::Run(rule))
                .collect();
        }
        let (late, waiting) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|(_, at)| now.duration_since(*at) > GRACE);
        self.pending = waiting;
        late.into_iter()
            .map(|(rule, _)| Step::Missed(rule))
            .collect()
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

    /// 2026-11-23 is a Monday.
    fn at(day: i8, h: i8, m: i8) -> DateTime {
        date(2026, 11, day).at(h, m, 0, 0)
    }

    fn runs(steps: &[Step]) -> usize {
        steps.iter().filter(|s| matches!(s, Step::Run(_))).count()
    }

    #[test]
    fn runs_once_at_its_time() {
        let mut s = Scheduler::new(vec![rule(23, 0, 0x7f)]);
        assert!(s.tick(at(23, 22, 58), true).is_empty());
        assert!(s.tick(at(23, 22, 59), true).is_empty());
        assert_eq!(runs(&s.tick(at(23, 23, 0), true)), 1);
        assert!(s.tick(at(23, 23, 1), true).is_empty());
        assert_eq!(runs(&s.tick(at(24, 23, 0), true)), 1, "again the next day");
    }

    #[test]
    fn follows_the_days() {
        let mut s = Scheduler::new(vec![rule(8, 0, WEEKDAYS)]);
        s.tick(at(28, 7, 59), true);
        assert!(s.tick(at(28, 8, 0), true).is_empty(), "Saturday");
        s.tick(at(30, 7, 59), true);
        assert_eq!(runs(&s.tick(at(30, 8, 0), true)), 1, "Monday");
    }

    #[test]
    fn crosses_midnight() {
        let mut s = Scheduler::new(vec![rule(0, 0, 0x7f)]);
        s.tick(at(23, 23, 59), true);
        assert_eq!(runs(&s.tick(at(24, 0, 0), true)), 1);
    }

    #[test]
    fn waits_for_the_device_then_gives_up() {
        let mut s = Scheduler::new(vec![rule(9, 0, 0x7f), rule(9, 30, 0x7f)]);
        s.tick(at(23, 8, 59), false);
        assert!(s.tick(at(23, 9, 0), false).is_empty());
        assert_eq!(runs(&s.tick(at(23, 9, 5), true)), 1, "device came back");

        assert!(s.tick(at(23, 9, 30), false).is_empty());
        let steps = s.tick(at(23, 9, 41), false);
        assert!(matches!(steps.as_slice(), [Step::Missed(_)]));
        assert!(s.tick(at(23, 9, 42), true).is_empty());
    }

    #[test]
    fn skips_old_times_and_repeated_clock_times() {
        let mut s = Scheduler::new(vec![rule(9, 0, 0x7f)]);
        assert!(s.tick(at(23, 9, 0), true).is_empty(), "first tick");
        s.tick(at(23, 10, 0), true);
        assert!(s.tick(at(24, 12, 0), true).is_empty(), "the Mac slept");

        s.tick(at(25, 8, 59), true);
        assert_eq!(runs(&s.tick(at(25, 9, 0), true)), 1);
        assert!(s.tick(at(25, 8, 30), true).is_empty(), "clock went back");
        assert!(s.tick(at(25, 9, 1), true).is_empty());
    }

    #[test]
    fn disabled_rules_do_not_run() {
        let mut off = rule(9, 0, 0x7f);
        off.enabled = false;
        let mut s = Scheduler::new(vec![off]);
        s.tick(at(23, 8, 59), true);
        assert!(s.tick(at(23, 9, 0), true).is_empty());
    }

    #[test]
    fn describes_actions() {
        let model = Registry::builtin().get("A1723").map(|m| m.as_ref());
        assert_eq!(
            Action::new(model, "ac_switch", 0).sentence(),
            "turned AC Output off"
        );
        assert_eq!(
            Action::new(model, "light_mode", 1).sentence(),
            "set Light to Low"
        );
    }
}
