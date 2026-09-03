//! Circular geofence alerts — notify when an aircraft enters a radius around a
//! point (default: home), optionally filtered by altitude and military status.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::alerts::AlertEvent;
use crate::db::Db;
use crate::state::AircraftDiff;
use crate::util::now_ms;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Geofence {
    #[serde(default)]
    pub id: i64,
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    pub radius_nm: f64,
    #[serde(default)]
    pub max_alt_ft: Option<f64>,
    #[serde(default)]
    pub mil_only: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

pub struct Geofences {
    db: Arc<Db>,
    /// (fence_id, hex) already alerted while the aircraft is inside.
    inside: Mutex<HashSet<(i64, String)>>,
}

impl Geofences {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            inside: Mutex::new(HashSet::new()),
        }
    }

    pub fn list(&self) -> Result<Vec<Geofence>> {
        self.db.with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT id, json, enabled FROM geofences ORDER BY id")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (id, json, enabled) = row?;
                if let Ok(mut g) = serde_json::from_str::<Geofence>(&json) {
                    g.id = id;
                    g.enabled = enabled != 0;
                    out.push(g);
                }
            }
            Ok(out)
        })
    }

    pub fn add(&self, g: &Geofence) -> Result<Geofence> {
        let json = serde_json::to_string(g)?;
        let id = self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO geofences(label, json, enabled) VALUES(?1, ?2, ?3)",
                rusqlite::params![g.label, json, g.enabled as i64],
            )?;
            Ok(c.last_insert_rowid())
        })?;
        Ok(Geofence { id, ..g.clone() })
    }

    pub fn remove(&self, id: i64) -> Result<()> {
        self.inside.lock().retain(|(fid, _)| *fid != id);
        self.db
            .with_conn(|c| Ok(c.execute("DELETE FROM geofences WHERE id = ?1", [id]).map(|_| ())?))
    }

    pub fn set_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        self.db.with_conn(|c| {
            c.execute(
                "UPDATE geofences SET enabled = ?2 WHERE id = ?1",
                rusqlite::params![id, enabled as i64],
            )?;
            Ok(())
        })
    }

    /// Fire an alert the first time an aircraft comes inside a fence.
    pub fn evaluate(&self, diff: &AircraftDiff) -> Vec<AlertEvent> {
        let fences: Vec<Geofence> = self
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|f| f.enabled)
            .collect();
        if fences.is_empty() {
            return Vec::new();
        }

        let mut inside = self.inside.lock();
        for hex in &diff.removed {
            inside.retain(|(_, h)| h != hex);
        }

        let now = now_ms();
        let mut out = Vec::new();

        for ac in diff.added.iter().chain(diff.updated.iter()) {
            let (Some(lat), Some(lon)) = (ac.lat, ac.lon) else {
                continue;
            };
            for f in &fences {
                let d = haversine_nm(lat, lon, f.lat, f.lon);
                let key = (f.id, ac.hex.clone());
                if d > f.radius_nm {
                    inside.remove(&key);
                    continue;
                }
                if f.mil_only && !ac.military {
                    continue;
                }
                if let Some(ceil) = f.max_alt_ft {
                    let alt = if ac.on_ground {
                        0.0
                    } else {
                        ac.alt_baro.or(ac.alt_geom).unwrap_or(f64::MAX)
                    };
                    if alt > ceil {
                        continue;
                    }
                }
                if inside.insert(key) {
                    let cs = ac
                        .flight
                        .as_deref()
                        .or(ac.registration.as_deref())
                        .unwrap_or(&ac.hex);
                    out.push(AlertEvent {
                        hex: ac.hex.clone(),
                        reason: format!(
                            "{} entered “{}” ({:.0} nm)",
                            cs, f.label, d
                        ),
                        watch_id: None,
                        emergency: false,
                        at: now,
                    });
                }
            }
        }
        out
    }
}

fn haversine_nm(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 3440.065_f64;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().min(1.0).asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_roughly_right() {
        // KJFK -> KLGA is about 9 nm
        let d = haversine_nm(40.6413, -73.7781, 40.7769, -73.8740);
        assert!((d - 9.0).abs() < 2.0, "{d}");
    }
}
