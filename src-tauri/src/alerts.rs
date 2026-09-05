//! Watchlist storage and alert evaluation.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::config::Place;
use crate::db::Db;
use crate::ingest::model::Aircraft;
use crate::state::AircraftDiff;
use crate::util::now_ms;

const EMERGENCY_SQUAWKS: [&str; 4] = ["7500", "7600", "7700", "7777"];

/// Great-circle distance in nautical miles.
fn haversine_nm(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 3440.065_f64; // earth radius, nm
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

/// Ray-casting point-in-polygon test. `poly` is an open ring of `[lat, lon]`
/// vertices (no repeated closing point needed). Good enough at geofence scale
/// (a few nm) where the lat/lon grid is locally near-flat.
fn point_in_polygon(lat: f64, lon: f64, poly: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (yi, xi) = (poly[i][0], poly[i][1]);
        let (yj, xj) = (poly[j][0], poly[j][1]);
        if ((yi > lat) != (yj > lat)) && (lon < (xj - xi) * (lat - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Stable fired-set key for a place id — large positive so it can't collide
/// with a watch row id (autoincrement, small) or the emergency key (-1).
fn place_key(id: &str) -> i64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    100_000_000 + (h % 1_000_000_000) as i64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WatchKind {
    Hex,
    Registration,
    Type,
    Callsign,
    Preset,
}

impl WatchKind {
    fn as_str(&self) -> &'static str {
        match self {
            WatchKind::Hex => "hex",
            WatchKind::Registration => "registration",
            WatchKind::Type => "type",
            WatchKind::Callsign => "callsign",
            WatchKind::Preset => "preset",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "hex" => Some(WatchKind::Hex),
            "registration" => Some(WatchKind::Registration),
            "type" => Some(WatchKind::Type),
            "callsign" => Some(WatchKind::Callsign),
            "preset" => Some(WatchKind::Preset),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchEntry {
    pub id: i64,
    pub kind: WatchKind,
    pub value: String,
    pub label: Option<String>,
    pub enabled: bool,
}

impl WatchEntry {
    fn matches(&self, ac: &Aircraft) -> bool {
        let v = self.value.trim();
        match self.kind {
            WatchKind::Hex => ac.hex.eq_ignore_ascii_case(v),
            WatchKind::Registration => ac
                .registration
                .as_deref()
                .map(|r| r.eq_ignore_ascii_case(v))
                .unwrap_or(false),
            WatchKind::Type => ac
                .type_code
                .as_deref()
                .map(|t| t.eq_ignore_ascii_case(v))
                .unwrap_or(false),
            WatchKind::Callsign => ac
                .flight
                .as_deref()
                .map(|f| f.eq_ignore_ascii_case(v))
                .unwrap_or(false),
            WatchKind::Preset => crate::notable::matches(v, ac),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertEvent {
    pub hex: String,
    pub reason: String,
    pub watch_id: Option<i64>,
    pub emergency: bool,
    pub at: i64,
}

pub struct Alerts {
    db: Arc<Db>,
    /// (hex, watch_id or -1 for emergency) already alerted this appearance.
    fired: Mutex<HashSet<(String, i64)>>,
}

impl Alerts {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            fired: Mutex::new(HashSet::new()),
        }
    }

    pub fn list(&self) -> Result<Vec<WatchEntry>> {
        self.db.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT id, kind, value, label, enabled FROM watchlist ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (id, kind, value, label, enabled) = row?;
                if let Some(kind) = WatchKind::parse(&kind) {
                    out.push(WatchEntry {
                        id,
                        kind,
                        value,
                        label,
                        enabled: enabled != 0,
                    });
                }
            }
            Ok(out)
        })
    }

    pub fn add(&self, kind: WatchKind, value: &str, label: Option<&str>) -> Result<WatchEntry> {
        let value = value.trim();
        if value.is_empty() {
            bail!("empty watch value");
        }
        let id = self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO watchlist(kind, value, label, enabled) VALUES(?1, ?2, ?3, 1)
                 ON CONFLICT(kind, value) DO UPDATE SET label = excluded.label, enabled = 1",
                rusqlite::params![kind.as_str(), value, label],
            )?;
            Ok(c.last_insert_rowid())
        })?;
        // last_insert_rowid is 0 on the upsert path; re-read to be safe.
        Ok(self
            .list()?
            .into_iter()
            .find(|w| w.kind == kind && w.value == value)
            .unwrap_or(WatchEntry {
                id,
                kind,
                value: value.to_string(),
                label: label.map(|s| s.to_string()),
                enabled: true,
            }))
    }

    pub fn remove(&self, id: i64) -> Result<()> {
        self.db
            .with_conn(|c| Ok(c.execute("DELETE FROM watchlist WHERE id = ?1", [id]).map(|_| ())?))
    }

    pub fn set_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        self.db.with_conn(|c| {
            c.execute(
                "UPDATE watchlist SET enabled = ?2 WHERE id = ?1",
                rusqlite::params![id, enabled as i64],
            )?;
            Ok(())
        })
    }

    /// Evaluate a fresh diff against the watchlist + emergency rules + the
    /// proximity alerts on saved places.
    pub fn evaluate(&self, diff: &AircraftDiff, places: &[Place]) -> Vec<AlertEvent> {
        let watches = self.list().unwrap_or_default();
        let geofences: Vec<&Place> = places.iter().filter(|p| p.alert.enabled).collect();
        let mut out = Vec::new();
        let now = now_ms();

        // Clear fired state for aircraft that left view.
        {
            let mut fired = self.fired.lock();
            for hex in &diff.removed {
                fired.retain(|(h, _)| h != hex);
            }
        }

        let candidates = diff.added.iter().chain(diff.updated.iter());
        for ac in candidates {
            // Emergency: squawk or emergency field.
            let emergency = ac
                .squawk
                .as_deref()
                .map(|s| EMERGENCY_SQUAWKS.contains(&s))
                .unwrap_or(false)
                || ac
                    .emergency
                    .as_deref()
                    .map(|e| !e.eq_ignore_ascii_case("none"))
                    .unwrap_or(false);

            if emergency && self.mark(&ac.hex, -1) {
                out.push(AlertEvent {
                    hex: ac.hex.clone(),
                    reason: match ac.squawk.as_deref() {
                        Some("7500") => "squawk 7500 — unlawful interference".into(),
                        Some("7600") => "squawk 7600 — radio failure".into(),
                        Some("7700") => "squawk 7700 — general emergency".into(),
                        Some(s) => format!("emergency (squawk {s})"),
                        None => "emergency indication".into(),
                    },
                    watch_id: None,
                    emergency: true,
                    at: now,
                });
            }

            for w in watches.iter().filter(|w| w.enabled) {
                if w.matches(ac) && self.mark(&ac.hex, w.id) {
                    let reason = if w.kind == WatchKind::Preset {
                        format!(
                            "{} aircraft",
                            crate::notable::label_for(&w.value).unwrap_or(&w.value)
                        )
                    } else {
                        format!(
                            "watch {} = {}{}",
                            w.kind.as_str(),
                            w.value,
                            w.label.as_deref().map(|l| format!(" ({l})")).unwrap_or_default()
                        )
                    };
                    out.push(AlertEvent {
                        hex: ac.hex.clone(),
                        reason,
                        watch_id: Some(w.id),
                        emergency: false,
                        at: now,
                    });
                }
            }

            // Proximity alerts on saved places.
            if let (Some(lat), Some(lon)) = (ac.lat, ac.lon) {
                for p in &geofences {
                    let polygon = p.alert.shape.as_deref().filter(|s| s.len() >= 3);
                    let (inside, reason_prefix) = match polygon {
                        Some(poly) => (
                            point_in_polygon(lat, lon, poly),
                            format!("inside {}", p.label),
                        ),
                        None => {
                            let d = haversine_nm(p.lat, p.lon, lat, lon);
                            (d <= p.alert.radius_nm, format!("{:.0} nm from {}", d, p.label))
                        }
                    };
                    if !inside {
                        continue;
                    }
                    if let Some(ceil) = p.alert.ceiling_ft {
                        if ac.alt_baro.map(|a| a > ceil).unwrap_or(true) {
                            continue;
                        }
                    }
                    if p.alert.notable_only
                        && !(ac.military || ac.interesting || ac.pia || ac.ladd)
                    {
                        continue;
                    }
                    if self.mark(&ac.hex, place_key(&p.id)) {
                        let alt = ac
                            .alt_baro
                            .map(|a| format!(" at {} ft", a.round() as i64))
                            .unwrap_or_default();
                        out.push(AlertEvent {
                            hex: ac.hex.clone(),
                            reason: format!("{reason_prefix}{alt}"),
                            watch_id: None,
                            emergency: false,
                            at: now,
                        });
                    }
                }
            }
        }
        out
    }

    /// Returns true if this (hex, key) had not fired yet.
    fn mark(&self, hex: &str, key: i64) -> bool {
        self.fired.lock().insert((hex.to_string(), key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::model::PositionSource;

    fn base(hex: &str) -> Aircraft {
        Aircraft {
            hex: hex.into(),
            flight: None,
            registration: None,
            type_code: None,
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

    fn diff_with(added: Vec<Aircraft>) -> AircraftDiff {
        AircraftDiff {
            added,
            ..Default::default()
        }
    }

    #[test]
    fn emergency_fires_once_until_gone() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let a = Alerts::new(db);
        let mut ac = base("abc");
        ac.squawk = Some("7700".into());

        let ev = a.evaluate(&diff_with(vec![ac.clone()]), &[]);
        assert_eq!(ev.len(), 1);
        assert!(ev[0].emergency);

        // Same aircraft still present -> no repeat.
        let ev = a.evaluate(
            &AircraftDiff {
                updated: vec![ac.clone()],
                ..Default::default()
            },
            &[],
        );
        assert!(ev.is_empty());

        // Leaves and returns -> fires again.
        let ev = a.evaluate(
            &AircraftDiff {
                removed: vec!["abc".into()],
                ..Default::default()
            },
            &[],
        );
        assert!(ev.is_empty());
        let ev = a.evaluate(&diff_with(vec![ac]), &[]);
        assert_eq!(ev.len(), 1);
    }

    #[test]
    fn watch_by_type_matches() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let a = Alerts::new(db);
        a.add(WatchKind::Type, "B738", Some("737")).unwrap();

        let mut ac = base("x");
        ac.type_code = Some("B738".into());
        let ev = a.evaluate(&diff_with(vec![ac]), &[]);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].watch_id.is_some(), true);
    }

    #[test]
    fn place_proximity_alert() {
        use crate::config::{Place, PlaceAlert};
        let db = Arc::new(Db::open_in_memory().unwrap());
        let a = Alerts::new(db);
        let place = Place {
            id: "p1".into(),
            label: "Home".into(),
            lat: 40.0,
            lon: -73.0,
            kind: None,
            bbox: None,
            primary: true,
            rtlsdr_location: false,
            alert: PlaceAlert {
                enabled: true,
                radius_nm: 10.0,
                ceiling_ft: Some(8000.0),
                notable_only: false,
                shape: None,
            },
        };

        // ~4 nm away, 5000 ft -> fires
        let mut near = base("near");
        near.lat = Some(40.05);
        near.lon = Some(-73.0);
        near.alt_baro = Some(5000.0);
        // 4 nm away but at 30000 ft -> above the ceiling, no alert
        let mut high = base("high");
        high.lat = Some(40.05);
        high.lon = Some(-73.0);
        high.alt_baro = Some(30000.0);
        // 60 nm away -> outside radius
        let mut far = base("far");
        far.lat = Some(41.0);
        far.lon = Some(-73.0);
        far.alt_baro = Some(3000.0);

        let ev = a.evaluate(&diff_with(vec![near, high, far]), std::slice::from_ref(&place));
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].hex, "near");
        assert!(ev[0].reason.contains("Home"));

        // no repeat while it stays in view
        let mut still = base("near");
        still.lat = Some(40.05);
        still.lon = Some(-73.0);
        still.alt_baro = Some(5000.0);
        let ev = a.evaluate(
            &AircraftDiff { updated: vec![still], ..Default::default() },
            std::slice::from_ref(&place),
        );
        assert!(ev.is_empty());
    }

    #[test]
    fn place_polygon_geofence() {
        use crate::config::{Place, PlaceAlert};
        let db = Arc::new(Db::open_in_memory().unwrap());
        let a = Alerts::new(db);
        // A ~0.2°-square box around (40, -73) — plenty big at this latitude.
        let place = Place {
            id: "p1".into(),
            label: "Airfield".into(),
            lat: 40.0,
            lon: -73.0,
            kind: None,
            bbox: None,
            primary: true,
            rtlsdr_location: false,
            alert: PlaceAlert {
                enabled: true,
                radius_nm: 1.0, // deliberately small — shape should win, not this
                ceiling_ft: None,
                notable_only: false,
                shape: Some(vec![
                    [39.9, -73.1],
                    [39.9, -72.9],
                    [40.1, -72.9],
                    [40.1, -73.1],
                ]),
            },
        };

        // Inside the box but ~7 nm from centre — well outside radius_nm, but
        // the shape takes over entirely once it's set.
        let mut inside = base("in");
        inside.lat = Some(40.05);
        inside.lon = Some(-73.05);
        // Outside the box.
        let mut outside = base("out");
        outside.lat = Some(40.2);
        outside.lon = Some(-73.05);

        let ev = a.evaluate(&diff_with(vec![inside, outside]), std::slice::from_ref(&place));
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].hex, "in");
        assert!(ev[0].reason.contains("inside Airfield"));
    }
}
