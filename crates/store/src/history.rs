use std::io::Write;
use std::path::Path;

use rusqlite::{Connection, params};

use crate::StoreError;

/// Fields recorded when they exist on a model.
pub const RECORDED: &[&str] = &[
    "battery_soc",
    "temperature",
    "total_input_power",
    "total_output_power",
    "ac_input_power",
    "ac_output_power",
    "solar_input_power",
    "car_output_power",
    "usb_c1_power",
    "usb_c2_power",
    "usb_c3_power",
    "usb_a_power",
    "usb_a1_power",
    "usb_a2_power",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub ts: i64,
    pub kind: String,
    pub text: String,
}

pub struct History {
    conn: Connection,
}

impl History {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, StoreError> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS samples (
                 device TEXT NOT NULL, ts INTEGER NOT NULL, field TEXT NOT NULL, value REAL NOT NULL);
             CREATE INDEX IF NOT EXISTS samples_by_field ON samples(device, field, ts);
             CREATE TABLE IF NOT EXISTS events (
                 device TEXT NOT NULL, ts INTEGER NOT NULL, kind TEXT NOT NULL, text TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS events_by_device ON events(device, ts);",
        )?;
        Ok(Self { conn })
    }

    pub fn record(
        &mut self,
        device: &str,
        ts: i64,
        values: &[(&str, f64)],
    ) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO samples (device, ts, field, value) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (field, value) in values {
                stmt.execute(params![device, ts, field, value])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn event(&self, device: &str, ts: i64, kind: &str, text: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO events (device, ts, kind, text) VALUES (?1, ?2, ?3, ?4)",
            params![device, ts, kind, text],
        )?;
        Ok(())
    }

    pub fn series(
        &self,
        device: &str,
        field: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<(i64, f64)>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT ts, value FROM samples WHERE device = ?1 AND field = ?2 AND ts >= ?3 AND ts <= ?4 ORDER BY ts",
        )?;
        let rows = stmt.query_map(params![device, field, from, to], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn events(&self, device: &str, from: i64, to: i64) -> Result<Vec<Event>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT ts, kind, text FROM events WHERE device = ?1 AND ts >= ?2 AND ts <= ?3 ORDER BY ts DESC",
        )?;
        let rows = stmt.query_map(params![device, from, to], |r| {
            Ok(Event {
                ts: r.get(0)?,
                kind: r.get(1)?,
                text: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Watt-hours per day for a power field, integrated with the trapezoid
    /// rule. Gaps longer than `max_gap` seconds are not bridged.
    pub fn daily_energy(
        &self,
        device: &str,
        field: &str,
        from: i64,
        to: i64,
        utc_offset: i64,
    ) -> Result<Vec<(i64, f64)>, StoreError> {
        const MAX_GAP: i64 = 300;
        let samples = self.series(device, field, from, to)?;
        let mut days: Vec<(i64, f64)> = Vec::new();
        for pair in samples.windows(2) {
            let ((t0, w0), (t1, w1)) = (pair[0], pair[1]);
            let dt = t1 - t0;
            if dt <= 0 || dt > MAX_GAP {
                continue;
            }
            let wh = (w0 + w1) / 2.0 * dt as f64 / 3600.0;
            let day = (t0 + utc_offset).div_euclid(86_400) * 86_400 - utc_offset;
            match days.last_mut() {
                Some((d, total)) if *d == day => *total += wh,
                _ => days.push((day, wh)),
            }
        }
        Ok(days)
    }

    pub fn export_csv(
        &self,
        device: &str,
        fields: &[&str],
        from: i64,
        to: i64,
        out: &mut dyn Write,
    ) -> Result<usize, StoreError> {
        writeln!(out, "timestamp,{}", fields.join(","))?;
        let mut rows: std::collections::BTreeMap<i64, Vec<Option<f64>>> = Default::default();
        for (i, field) in fields.iter().enumerate() {
            for (ts, value) in self.series(device, field, from, to)? {
                rows.entry(ts).or_insert_with(|| vec![None; fields.len()])[i] = Some(value);
            }
        }
        for (ts, values) in &rows {
            let cells: Vec<String> = values
                .iter()
                .map(|v| v.map(|x| x.to_string()).unwrap_or_default())
                .collect();
            writeln!(out, "{ts},{}", cells.join(","))?;
        }
        Ok(rows.len())
    }

    pub fn prune(&self, before: i64) -> Result<usize, StoreError> {
        let samples = self
            .conn
            .execute("DELETE FROM samples WHERE ts < ?1", params![before])?;
        let events = self
            .conn
            .execute("DELETE FROM events WHERE ts < ?1", params![before])?;
        Ok(samples + events)
    }

    pub fn forget(&self, device: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM samples WHERE device = ?1", params![device])?;
        self.conn
            .execute("DELETE FROM events WHERE device = ?1", params![device])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_queries_and_integrates() {
        let mut h = History::memory().expect("db");
        for i in 0..=360 {
            h.record(
                "dev",
                1_700_000_000 + i * 10,
                &[("total_output_power", 100.0), ("battery_soc", 90.0)],
            )
            .expect("record");
        }
        let series = h.series("dev", "battery_soc", 0, i64::MAX).expect("series");
        assert_eq!(series.len(), 361);
        let days = h
            .daily_energy("dev", "total_output_power", 0, i64::MAX, 0)
            .expect("energy");
        let total: f64 = days.iter().map(|d| d.1).sum();
        assert!(
            (total - 100.0).abs() < 0.01,
            "one hour at 100 W is 100 Wh, got {total}"
        );

        h.event("dev", 1_700_000_000, "automation", "USB wake")
            .expect("event");
        assert_eq!(h.events("dev", 0, i64::MAX).expect("events").len(), 1);

        let mut csv = Vec::new();
        let rows = h
            .export_csv(
                "dev",
                &["battery_soc", "total_output_power"],
                0,
                i64::MAX,
                &mut csv,
            )
            .expect("csv");
        assert_eq!(rows, 361);
        assert!(
            String::from_utf8_lossy(&csv).starts_with("timestamp,battery_soc,total_output_power\n")
        );

        assert!(h.prune(1_700_001_000).expect("prune") > 0);
    }
}
