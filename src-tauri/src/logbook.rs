//! Spotter logbook — a lifetime record of every airframe that's been in view,
//! with first/last seen, an appearance count, and a free-text note. Passive:
//! the poller records `diff.added` each cycle.

use std::sync::Arc;

use anyhow::Result;
use rusqlite::Row;
use serde::Serialize;

use crate::db::Db;
use crate::ingest::model::Aircraft;

/// A gap longer than this since `last_seen` counts as a fresh appearance.
const NEW_APPEARANCE_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sighting {
    pub hex: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub count: i64,
    pub flight: Option<String>,
    pub registration: Option<String>,
    pub type_code: Option<String>,
    pub description: Option<String>,
    pub military: bool,
    pub note: Option<String>,
}

fn row_to_sighting(r: &Row) -> rusqlite::Result<Sighting> {
    Ok(Sighting {
        hex: r.get("hex")?,
        first_seen: r.get("first_seen")?,
        last_seen: r.get("last_seen")?,
        count: r.get("count")?,
        flight: r.get("flight")?,
        registration: r.get("registration")?,
        type_code: r.get("type_code")?,
        description: r.get("description")?,
        military: r.get::<_, i64>("military")? != 0,
        note: r.get("note")?,
    })
}

pub struct Logbook {
    db: Arc<Db>,
}

impl Logbook {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Upsert a batch of aircraft (typically `diff.added`). Best-effort.
    pub fn record(&self, aircraft: &[Aircraft], now: i64) {
        if aircraft.is_empty() {
            return;
        }
        let res = self.db.with_conn(|c| {
            let mut stmt = c.prepare_cached(
                "INSERT INTO sightings
                   (hex, first_seen, last_seen, count, flight, registration, type_code, description, military, note)
                 VALUES (?1, ?2, ?2, 1, ?3, ?4, ?5, ?6, ?7, NULL)
                 ON CONFLICT(hex) DO UPDATE SET
                   last_seen    = ?2,
                   count        = count + (CASE WHEN ?2 - last_seen > ?8 THEN 1 ELSE 0 END),
                   flight       = COALESCE(?3, flight),
                   registration = COALESCE(?4, registration),
                   type_code    = COALESCE(?5, type_code),
                   description  = COALESCE(?6, description),
                   military     = MAX(military, ?7)",
            )?;
            for a in aircraft {
                stmt.execute(rusqlite::params![
                    a.hex,
                    now,
                    a.flight,
                    a.registration,
                    a.type_code,
                    a.description,
                    a.military as i64,
                    NEW_APPEARANCE_MS,
                ])?;
            }
            Ok(())
        });
        if let Err(e) = res {
            tracing::warn!("logbook write failed: {e}");
        }
    }

    pub fn get(&self, hex: &str) -> Result<Option<Sighting>> {
        self.db.with_conn(|c| {
            let mut stmt = c.prepare("SELECT * FROM sightings WHERE hex = ?1")?;
            let mut rows = stmt.query([hex])?;
            match rows.next()? {
                Some(r) => Ok(Some(row_to_sighting(r)?)),
                None => Ok(None),
            }
        })
    }

    /// `sort` ∈ last | first | count | reg. `search` matches hex/reg/flight/type.
    pub fn list(&self, sort: &str, search: &str, limit: i64) -> Result<Vec<Sighting>> {
        let order = match sort {
            "first" => "first_seen DESC",
            "count" => "count DESC, last_seen DESC",
            "reg" => "registration IS NULL, registration ASC",
            _ => "last_seen DESC",
        };
        let like = format!("%{}%", search.trim().to_lowercase());
        let sql = format!(
            "SELECT * FROM sightings
             WHERE (?1 = '%%'
                    OR lower(hex) LIKE ?1
                    OR lower(COALESCE(registration,'')) LIKE ?1
                    OR lower(COALESCE(flight,'')) LIKE ?1
                    OR lower(COALESCE(type_code,'')) LIKE ?1)
             ORDER BY {order} LIMIT ?2"
        );
        self.db.with_conn(|c| {
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params![like, limit], |r| row_to_sighting(r))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn set_note(&self, hex: &str, note: &str) -> Result<()> {
        let note = note.trim();
        self.db.with_conn(|c| {
            c.execute(
                "UPDATE sightings SET note = ?2 WHERE hex = ?1",
                rusqlite::params![hex, if note.is_empty() { None } else { Some(note) }],
            )?;
            Ok(())
        })
    }

    pub fn delete(&self, hex: &str) -> Result<()> {
        self.db
            .with_conn(|c| Ok(c.execute("DELETE FROM sightings WHERE hex = ?1", [hex]).map(|_| ())?))
    }

    pub fn clear(&self) -> Result<()> {
        self.db
            .with_conn(|c| Ok(c.execute("DELETE FROM sightings", []).map(|_| ())?))
    }

    pub fn count(&self) -> Result<i64> {
        self.db
            .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM sightings", [], |r| r.get(0))?))
    }

    pub fn export_csv(&self) -> Result<String> {
        let rows = self.list("first", "", 100_000)?;
        let mut out =
            String::from("hex,registration,type,description,flight,first_seen,last_seen,count,note\n");
        for s in rows {
            let esc = |v: Option<String>| {
                let v = v.unwrap_or_default().replace('"', "\"\"");
                format!("\"{v}\"")
            };
            let iso = |ms: i64| {
                chrono_iso(ms)
            };
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                s.hex,
                esc(s.registration),
                esc(s.type_code),
                esc(s.description),
                esc(s.flight),
                iso(s.first_seen),
                iso(s.last_seen),
                s.count,
                esc(s.note),
            ));
        }
        Ok(out)
    }
}

/// Minimal UTC ISO-8601 without pulling in a date crate.
pub fn chrono_iso(ms: i64) -> String {
    let secs = ms / 1000;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    // days since 1970-01-01 -> y/m/d (civil calendar, Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::model::{Aircraft, PositionSource};

    fn ac(hex: &str, reg: Option<&str>) -> Aircraft {
        Aircraft {
            hex: hex.into(),
            flight: None,
            registration: reg.map(Into::into),
            type_code: Some("B738".into()),
            description: None,
            category: None,
            lat: Some(1.0),
            lon: Some(1.0),
            alt_baro: None,
            alt_geom: None,
            on_ground: false,
            ground_speed: None,
            ias: None,
            tas: None,
            mach: None,
            track: None,
            mag_heading: None,
            true_heading: None,
            baro_rate: None,
            geom_rate: None,
            squawk: None,
            emergency: None,
            nav_altitude: None,
            nav_heading: None,
            nav_qnh: None,
            rssi: None,
            messages: None,
            seen: None,
            seen_pos: None,
            position_source: PositionSource::Adsb,
            military: false,
            interesting: false,
            pia: false,
            ladd: false,
            source: "t".into(),
            observed_at: 0,
        }
    }

    #[test]
    fn record_counts_and_notes() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let lb = Logbook::new(db);

        lb.record(&[ac("abc", Some("N1"))], 1_000);
        let s = lb.get("abc").unwrap().unwrap();
        assert_eq!(s.count, 1);
        assert_eq!(s.registration.as_deref(), Some("N1"));

        // seen again within the hour -> no count bump
        lb.record(&[ac("abc", None)], 1_000 + 30 * 60 * 1000);
        assert_eq!(lb.get("abc").unwrap().unwrap().count, 1);

        // seen again after > 1 h -> new appearance
        lb.record(&[ac("abc", None)], 1_000 + 2 * 60 * 60 * 1000);
        let s = lb.get("abc").unwrap().unwrap();
        assert_eq!(s.count, 2);
        assert_eq!(s.first_seen, 1_000); // unchanged

        lb.set_note("abc", " first 737 ").unwrap();
        assert_eq!(lb.get("abc").unwrap().unwrap().note.as_deref(), Some("first 737"));

        assert_eq!(lb.count().unwrap(), 1);
        assert!(lb.export_csv().unwrap().contains("abc"));
    }
}
