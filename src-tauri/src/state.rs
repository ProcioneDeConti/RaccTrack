//! In-memory live aircraft state: current snapshot, per-aircraft trail buffers,
//! and diffing so the frontend only receives changes.

use std::collections::VecDeque;

use dashmap::DashMap;
use serde::Serialize;

use crate::ingest::model::Aircraft;

/// Keep an aircraft on the map this long after we last saw it in a snapshot,
/// so brief gaps between polls don't make icons flicker.
const STALE_MS: i64 = 60_000;
const TRAIL_MAX_POINTS: usize = 400;
const TRAIL_MAX_AGE_MS: i64 = 45 * 60 * 1000;
/// Minimum move (deg, ~roughly) before appending a new trail point.
const TRAIL_MIN_MOVE_DEG: f64 = 0.0015;
const TRAIL_MIN_INTERVAL_MS: i64 = 3_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrailPoint {
    pub lat: f64,
    pub lon: f64,
    pub alt_baro: Option<f64>,
    pub on_ground: bool,
    pub t: i64,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AircraftDiff {
    pub added: Vec<Aircraft>,
    pub updated: Vec<Aircraft>,
    pub removed: Vec<String>,
    pub total: u64,
    pub generated_at: i64,
}

pub struct LiveState {
    aircraft: DashMap<String, Aircraft>,
    trails: DashMap<String, VecDeque<TrailPoint>>,
    total: parking_lot::Mutex<u64>,
}

impl LiveState {
    pub fn new() -> Self {
        Self {
            aircraft: DashMap::new(),
            trails: DashMap::new(),
            total: parking_lot::Mutex::new(0),
        }
    }

    pub fn total(&self) -> u64 {
        *self.total.lock()
    }

    pub fn get(&self, hex: &str) -> Option<Aircraft> {
        self.aircraft.get(hex).map(|r| r.clone())
    }

    pub fn snapshot(&self) -> Vec<Aircraft> {
        self.aircraft.iter().map(|r| r.clone()).collect()
    }

    pub fn trail(&self, hex: &str) -> Vec<TrailPoint> {
        self.trails
            .get(hex)
            .map(|r| r.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn all_trails(&self) -> std::collections::HashMap<String, Vec<TrailPoint>> {
        self.trails
            .iter()
            .map(|r| (r.key().clone(), r.iter().cloned().collect()))
            .collect()
    }

    /// Merge a fresh snapshot, returning what changed.
    pub fn ingest(&self, incoming: Vec<Aircraft>, feed_total: u64, now_ms: i64) -> AircraftDiff {
        *self.total.lock() = feed_total;

        let mut diff = AircraftDiff {
            total: feed_total,
            generated_at: now_ms,
            ..Default::default()
        };

        let mut present: std::collections::HashSet<String> =
            std::collections::HashSet::with_capacity(incoming.len());

        for ac in incoming {
            present.insert(ac.hex.clone());
            self.append_trail(&ac, now_ms);

            match self.aircraft.entry(ac.hex.clone()) {
                dashmap::mapref::entry::Entry::Occupied(mut e) => {
                    e.insert(ac.clone());
                    diff.updated.push(ac);
                }
                dashmap::mapref::entry::Entry::Vacant(e) => {
                    e.insert(ac.clone());
                    diff.added.push(ac);
                }
            }
        }

        // Evict aircraft we haven't seen recently.
        let mut to_remove = Vec::new();
        for r in self.aircraft.iter() {
            if !present.contains(r.key()) && now_ms - r.observed_at > STALE_MS {
                to_remove.push(r.key().clone());
            }
        }
        for hex in to_remove {
            self.aircraft.remove(&hex);
            self.trails.remove(&hex);
            diff.removed.push(hex);
        }

        diff
    }

    fn append_trail(&self, ac: &Aircraft, now_ms: i64) {
        let (Some(lat), Some(lon)) = (ac.lat, ac.lon) else {
            return;
        };
        let mut buf = self.trails.entry(ac.hex.clone()).or_default();

        if let Some(last) = buf.back() {
            let moved = (last.lat - lat).abs() > TRAIL_MIN_MOVE_DEG
                || (last.lon - lon).abs() > TRAIL_MIN_MOVE_DEG;
            let waited = now_ms - last.t >= TRAIL_MIN_INTERVAL_MS;
            if !moved && !waited {
                return;
            }
        }

        buf.push_back(TrailPoint {
            lat,
            lon,
            alt_baro: ac.alt_baro,
            on_ground: ac.on_ground,
            t: now_ms,
        });

        while buf.len() > TRAIL_MAX_POINTS {
            buf.pop_front();
        }
        while buf.front().map(|p| now_ms - p.t > TRAIL_MAX_AGE_MS).unwrap_or(false) {
            buf.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::model::{Aircraft, PositionSource};

    fn ac(hex: &str, lat: f64, lon: f64, t: i64) -> Aircraft {
        Aircraft {
            hex: hex.into(),
            flight: None,
            registration: None,
            type_code: None,
            description: None,
            category: None,
            lat: Some(lat),
            lon: Some(lon),
            alt_baro: Some(10000.0),
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
            source: "test".into(),
            observed_at: t,
        }
    }

    #[test]
    fn added_then_updated_then_removed() {
        let s = LiveState::new();
        let d1 = s.ingest(vec![ac("a", 40.0, -73.0, 0)], 1, 0);
        assert_eq!(d1.added.len(), 1);

        let d2 = s.ingest(vec![ac("a", 40.1, -73.0, 1000)], 1, 1000);
        assert_eq!(d2.updated.len(), 1);
        assert!(d2.added.is_empty());

        // Not present, but within stale window -> kept.
        let d3 = s.ingest(vec![], 0, 2000);
        assert!(d3.removed.is_empty());

        // Now past the stale window -> removed.
        let d4 = s.ingest(vec![], 0, 70_000);
        assert_eq!(d4.removed, vec!["a".to_string()]);
    }

    #[test]
    fn trail_grows_and_is_bounded() {
        let s = LiveState::new();
        for i in 0..(TRAIL_MAX_POINTS as i64 + 50) {
            s.ingest(vec![ac("a", 40.0 + i as f64 * 0.01, -73.0, i * 4000)], 1, i * 4000);
        }
        assert!(s.trail("a").len() <= TRAIL_MAX_POINTS);
    }
}
