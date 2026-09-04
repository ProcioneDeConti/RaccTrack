//! Persistence for the self-collected flight-event history.
//!
//! Events are produced by `state::detect_events` (per-diff state changes) and by
//! `alerts` / `emergency_watch`, written here, and read back by the detail panel
//! (per-hex) and the Events feed (global).

use std::sync::Arc;

use anyhow::Result;
use rusqlite::Row;

use crate::db::Db;
use crate::state::{AircraftEvent, EventKind};

pub struct History {
    db: Arc<Db>,
}

fn row_to_event(r: &Row) -> rusqlite::Result<AircraftEvent> {
    let kind: String = r.get("kind")?;
    Ok(AircraftEvent {
        hex: r.get("hex")?,
        at: r.get("at")?,
        kind: EventKind::parse(&kind).unwrap_or(EventKind::Alert),
        flight: r.get("flight")?,
        from: r.get("from_val")?,
        to: r.get("to_val")?,
        lat: r.get("lat")?,
        lon: r.get("lon")?,
    })
}

impl History {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Append a batch of events. Best-effort — a write failure is logged, not
    /// propagated (history is not load-bearing).
    pub fn record(&self, events: &[AircraftEvent]) {
        if events.is_empty() {
            return;
        }
        let res = self.db.with_conn(|c| {
            let mut stmt = c.prepare_cached(
                "INSERT INTO aircraft_events(hex, at, kind, flight, from_val, to_val, lat, lon)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for e in events {
                stmt.execute(rusqlite::params![
                    e.hex,
                    e.at,
                    e.kind.as_str(),
                    e.flight,
                    e.from,
                    e.to,
                    e.lat,
                    e.lon,
                ])?;
            }
            Ok(())
        });
        if let Err(e) = res {
            tracing::warn!("history write failed: {e}");
        }
    }

    pub fn for_hex(&self, hex: &str, limit: i64) -> Result<Vec<AircraftEvent>> {
        self.db.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT hex, at, kind, flight, from_val, to_val, lat, lon
                 FROM aircraft_events WHERE hex = ?1 ORDER BY at DESC, id DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![hex, limit], |r| row_to_event(r))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn recent(&self, limit: i64) -> Result<Vec<AircraftEvent>> {
        self.db.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT hex, at, kind, flight, from_val, to_val, lat, lon
                 FROM aircraft_events ORDER BY at DESC, id DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit], |r| row_to_event(r))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Delete events older than `cutoff_ms`. Returns rows removed.
    pub fn prune(&self, cutoff_ms: i64) -> Result<usize> {
        self.db.with_conn(|c| {
            Ok(c.execute("DELETE FROM aircraft_events WHERE at < ?1", [cutoff_ms])?)
        })
    }

    pub fn clear(&self) -> Result<()> {
        self.db
            .with_conn(|c| Ok(c.execute("DELETE FROM aircraft_events", [])?).map(|_| ()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(hex: &str, at: i64, kind: EventKind) -> AircraftEvent {
        AircraftEvent {
            hex: hex.into(),
            at,
            kind,
            flight: None,
            from: None,
            to: None,
            lat: None,
            lon: None,
        }
    }

    #[test]
    fn record_query_prune() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let h = History::new(db);
        h.record(&[
            ev("aaa", 1_000, EventKind::Squawk),
            ev("aaa", 2_000, EventKind::Takeoff),
            ev("bbb", 3_000, EventKind::Emergency),
        ]);

        assert_eq!(h.for_hex("aaa", 10).unwrap().len(), 2);
        assert_eq!(h.recent(10).unwrap()[0].hex, "bbb"); // newest first

        assert_eq!(h.prune(2_500).unwrap(), 2);
        assert_eq!(h.recent(10).unwrap().len(), 1);
    }
}
