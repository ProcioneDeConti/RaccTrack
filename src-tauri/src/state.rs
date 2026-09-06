//! In-memory live aircraft state: current snapshot, per-aircraft trail buffers,
//! and diffing so the frontend only receives changes.

use std::collections::VecDeque;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::ingest::model::Aircraft;

/// Keep an aircraft on the map this long after we last saw it in a snapshot,
/// so brief gaps between polls don't make icons flicker.
const STALE_MS: i64 = 60_000;
const TRAIL_MAX_POINTS: usize = 400;
const TRAIL_MAX_AGE_MS: i64 = 45 * 60 * 1000;
/// Minimum move (deg, ~roughly) before appending a new trail point.
const TRAIL_MIN_MOVE_DEG: f64 = 0.0015;
const TRAIL_MIN_INTERVAL_MS: i64 = 3_000;

/// When we first saw an aircraft airborne this session. `saw_departure` is true
/// only if we had it on the ground first and watched it lift off (so `since_ms`
/// is a real off-block/rotation time); otherwise it's a lower bound — we picked
/// the flight up mid-air.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AirborneInfo {
    pub since_ms: i64,
    pub saw_departure: bool,
}

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
    /// Notable state changes detected this cycle — persisted to history +
    /// emitted separately by the poller, not part of the frontend diff payload.
    #[serde(skip)]
    pub events: Vec<AircraftEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Squawk,
    Emergency,
    EmergencyClear,
    Callsign,
    Takeoff,
    Landing,
    /// A watchlist / preset rule fired (recorded from `alerts::evaluate`).
    Alert,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Squawk => "squawk",
            EventKind::Emergency => "emergency",
            EventKind::EmergencyClear => "emergency_clear",
            EventKind::Callsign => "callsign",
            EventKind::Takeoff => "takeoff",
            EventKind::Landing => "landing",
            EventKind::Alert => "alert",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "squawk" => EventKind::Squawk,
            "emergency" => EventKind::Emergency,
            "emergency_clear" => EventKind::EmergencyClear,
            "callsign" => EventKind::Callsign,
            "takeoff" => EventKind::Takeoff,
            "landing" => EventKind::Landing,
            "alert" => EventKind::Alert,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AircraftEvent {
    pub hex: String,
    pub at: i64,
    pub kind: EventKind,
    /// Callsign at the time of the event, if known.
    pub flight: Option<String>,
    /// Squawk/callsign: the previous value.
    pub from: Option<String>,
    /// Squawk/callsign: the new value. Emergency: the squawk code. Alert: reason.
    pub to: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

const EMERGENCY_SQUAWKS: [&str; 3] = ["7500", "7600", "7700"];

fn is_emergency_squawk(s: Option<&str>) -> bool {
    s.map(|s| EMERGENCY_SQUAWKS.contains(&s)).unwrap_or(false)
}

/// Compare an aircraft's previous and current state, pushing any notable
/// transitions onto `out`.
fn detect_events(prev: &Aircraft, next: &Aircraft, now: i64, out: &mut Vec<AircraftEvent>) {
    let mk = |kind, from, to| AircraftEvent {
        hex: next.hex.clone(),
        at: now,
        kind,
        flight: next.flight.clone(),
        from,
        to,
        lat: next.lat,
        lon: next.lon,
    };

    if prev.squawk != next.squawk {
        let was = is_emergency_squawk(prev.squawk.as_deref());
        let is = is_emergency_squawk(next.squawk.as_deref());
        if is && !was {
            out.push(mk(EventKind::Emergency, prev.squawk.clone(), next.squawk.clone()));
        } else if was && !is {
            out.push(mk(
                EventKind::EmergencyClear,
                prev.squawk.clone(),
                next.squawk.clone(),
            ));
        } else if next.squawk.is_some() {
            out.push(mk(EventKind::Squawk, prev.squawk.clone(), next.squawk.clone()));
        }
    }

    if let (Some(a), Some(b)) = (prev.flight.as_deref(), next.flight.as_deref()) {
        if !a.is_empty() && !b.is_empty() && a != b {
            out.push(mk(EventKind::Callsign, prev.flight.clone(), next.flight.clone()));
        }
    }

    if prev.on_ground && !next.on_ground {
        out.push(mk(EventKind::Takeoff, None, None));
    } else if !prev.on_ground && next.on_ground {
        out.push(mk(EventKind::Landing, None, None));
    }
}

pub struct LiveState {
    aircraft: DashMap<String, Aircraft>,
    trails: DashMap<String, VecDeque<TrailPoint>>,
    airborne: DashMap<String, AirborneInfo>,
    total: parking_lot::Mutex<u64>,
}

impl LiveState {
    pub fn new() -> Self {
        Self {
            aircraft: DashMap::new(),
            trails: DashMap::new(),
            airborne: DashMap::new(),
            total: parking_lot::Mutex::new(0),
        }
    }

    pub fn total(&self) -> u64 {
        *self.total.lock()
    }

    pub fn get(&self, hex: &str) -> Option<Aircraft> {
        self.aircraft.get(hex).map(|r| r.clone())
    }

    /// When this aircraft was first seen airborne this session, if it's up now.
    pub fn airborne(&self, hex: &str) -> Option<AirborneInfo> {
        self.airborne.get(hex).map(|r| *r)
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

            // Track "airborne since" for the flight-progress widget.
            let prev_on_ground = self.aircraft.get(&ac.hex).map(|r| r.on_ground);
            if ac.on_ground {
                self.airborne.remove(&ac.hex);
            } else {
                self.airborne
                    .entry(ac.hex.clone())
                    .or_insert_with(|| AirborneInfo {
                        since_ms: now_ms,
                        saw_departure: prev_on_ground == Some(true),
                    });
            }

            match self.aircraft.entry(ac.hex.clone()) {
                dashmap::mapref::entry::Entry::Occupied(mut e) => {
                    detect_events(e.get(), &ac, now_ms, &mut diff.events);
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
            self.airborne.remove(&hex);
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
            roll: None,
            track_rate: None,
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
    fn detects_squawk_emergency_and_movement() {
        let s = LiveState::new();
        let mut a = ac("a", 40.0, -73.0, 0);
        a.on_ground = true;
        s.ingest(vec![a.clone()], 1, 0);

        // takeoff + squawk assignment
        a.on_ground = false;
        a.squawk = Some("1200".into());
        let d = s.ingest(vec![a.clone()], 1, 1000);
        let kinds: Vec<_> = d.events.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&EventKind::Takeoff));
        assert!(kinds.contains(&EventKind::Squawk));
        assert!(s.airborne("a").unwrap().saw_departure);

        // squawk -> emergency
        a.squawk = Some("7700".into());
        let d = s.ingest(vec![a.clone()], 1, 2000);
        assert_eq!(d.events.iter().filter(|e| e.kind == EventKind::Emergency).count(), 1);

        // emergency cleared
        a.squawk = Some("4321".into());
        let d = s.ingest(vec![a.clone()], 1, 3000);
        assert_eq!(
            d.events.iter().filter(|e| e.kind == EventKind::EmergencyClear).count(),
            1
        );

        // landing drops the airborne record
        a.on_ground = true;
        let d = s.ingest(vec![a], 1, 4000);
        assert!(d.events.iter().any(|e| e.kind == EventKind::Landing));
        assert!(s.airborne("a").is_none());
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
