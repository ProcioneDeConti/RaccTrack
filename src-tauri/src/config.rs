//! User-facing application settings, persisted as a single JSON blob.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::db::Db;

const KEY: &str = "app_settings";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeLocation {
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    /// [west, south, east, north] — present for area places (states, cities…).
    #[serde(default)]
    pub bbox: Option<[f64; 4]>,
}

/// A saved location. One is `primary` (drives the "go to" button + range
/// rings); each can carry a proximity alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub id: String,
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default)]
    pub kind: Option<String>,
    /// [west, south, east, north] — for camera framing of area places.
    #[serde(default)]
    pub bbox: Option<[f64; 4]>,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub alert: PlaceAlert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceAlert {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_alert_radius")]
    pub radius_nm: f64,
    /// Only alert when the aircraft is at or below this baro altitude (ft).
    #[serde(default)]
    pub ceiling_ft: Option<f64>,
    /// Only alert for military / interesting / PIA / LADD airframes.
    #[serde(default)]
    pub notable_only: bool,
}

impl Default for PlaceAlert {
    fn default() -> Self {
        Self {
            enabled: false,
            radius_nm: default_alert_radius(),
            ceiling_ft: None,
            notable_only: false,
        }
    }
}

fn default_alert_radius() -> f64 {
    10.0
}

pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let t = crate::util::now_ms() as u64;
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("p{t:x}{n:x}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub poll_interval_ms: u64,
    pub source_order: Vec<String>,
    /// Use a local dump1090/readsb `aircraft.json` feed. When on it's tried
    /// first, falling back to the community aggregators if unreachable.
    #[serde(default)]
    pub local_receiver_enabled: bool,
    #[serde(default = "default_local_receiver_url")]
    pub local_receiver_url: String,
    pub basemap: String,
    /// Legacy single home — migrated into `places` on load, then left null.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<HomeLocation>,
    #[serde(default)]
    pub places: Vec<Place>,
    /// Contact URL or email — planespotters.net requires one in the User-Agent
    /// to serve exact-airframe photos. Empty = use model photos only.
    #[serde(default)]
    pub contact: String,
    #[serde(default)]
    pub layers: MapLayers,
    #[serde(default = "default_range_rings")]
    pub range_rings_nm: Vec<f64>,
    #[serde(default)]
    pub pinned: Vec<String>,
    #[serde(default = "default_true")]
    pub emergency_watch_enabled: bool,
    #[serde(default = "default_true")]
    pub history_enabled: bool,
    #[serde(default = "default_history_days")]
    pub history_retention_days: u32,
    #[serde(default = "default_true")]
    pub logbook_enabled: bool,
    pub tile_cache_enabled: bool,
    pub tile_cache_max_mb: u64,
    pub units: String, // "imperial" | "metric"
    pub notifications_enabled: bool,
    pub show_all_trails: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MapLayers {
    #[serde(default)]
    pub airports: bool,
    #[serde(default)]
    pub weather: bool,
    #[serde(default)]
    pub radar: bool,
    #[serde(default)]
    pub airspace: bool,
    #[serde(default)]
    pub range_rings: bool,
}

fn default_range_rings() -> Vec<f64> {
    vec![25.0, 50.0, 100.0]
}
fn default_true() -> bool {
    true
}
fn default_history_days() -> u32 {
    30
}
fn default_local_receiver_url() -> String {
    crate::ingest::local::DEFAULT_URL.to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            poll_interval_ms: 3_000,
            // adsb.fi has proven more tolerant of viewport polling; adsb.lol is
            // the fallback. Both are free community ODbL feeds.
            source_order: vec!["adsb.fi".into(), "adsb.lol".into()],
            local_receiver_enabled: false,
            local_receiver_url: default_local_receiver_url(),
            basemap: "darkMatter".into(),
            home: None,
            places: Vec::new(),
            contact: String::new(),
            layers: MapLayers::default(),
            range_rings_nm: default_range_rings(),
            pinned: Vec::new(),
            emergency_watch_enabled: true,
            history_enabled: true,
            history_retention_days: 30,
            logbook_enabled: true,
            tile_cache_enabled: false,
            tile_cache_max_mb: 500,
            units: "imperial".into(),
            notifications_enabled: true,
            show_all_trails: false,
        }
    }
}

impl AppSettings {
    pub fn load(db: &Db) -> Self {
        let mut s: Self = db
            .get_setting(KEY)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        s.migrate();
        s
    }

    /// Fold the legacy single `home` into `places`.
    fn migrate(&mut self) {
        if let Some(home) = self.home.take() {
            if self.places.is_empty() {
                self.places.push(Place {
                    id: new_id(),
                    label: home.label,
                    lat: home.lat,
                    lon: home.lon,
                    kind: None,
                    bbox: home.bbox,
                    primary: true,
                    alert: PlaceAlert::default(),
                });
            }
        }
        // Exactly one primary.
        if !self.places.is_empty() && !self.places.iter().any(|p| p.primary) {
            self.places[0].primary = true;
        }
    }

    pub fn primary_place(&self) -> Option<&Place> {
        self.places
            .iter()
            .find(|p| p.primary)
            .or_else(|| self.places.first())
    }

    pub fn save(&self, db: &Db) -> Result<()> {
        db.set_setting(KEY, &serde_json::to_string(self)?)
    }

    /// Apply a partial JSON patch and clamp to sane bounds.
    pub fn apply_patch(&mut self, patch: serde_json::Value) {
        if let Ok(mut current) = serde_json::to_value(&*self) {
            merge(&mut current, patch);
            if let Ok(next) = serde_json::from_value::<AppSettings>(current) {
                *self = next;
            }
        }
        self.poll_interval_ms = self.poll_interval_ms.clamp(1_000, 30_000);
        self.tile_cache_max_mb = self.tile_cache_max_mb.clamp(50, 8_000);
        self.history_retention_days = self.history_retention_days.clamp(1, 3650);
        if self.units != "metric" {
            self.units = "imperial".into();
        }
        for p in &mut self.places {
            p.alert.radius_nm = p.alert.radius_nm.clamp(0.5, 250.0);
        }
        self.migrate(); // keep the one-primary invariant
    }
}

fn merge(base: &mut serde_json::Value, patch: serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(b), serde_json::Value::Object(p)) => {
            for (k, v) in p {
                merge(b.entry(k).or_insert(serde_json::Value::Null), v);
            }
        }
        (b, p) => *b = p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_updates_and_clamps() {
        let mut s = AppSettings::default();
        s.apply_patch(serde_json::json!({ "pollIntervalMs": 999999, "units": "metric" }));
        assert_eq!(s.poll_interval_ms, 30_000);
        assert_eq!(s.units, "metric");
        assert!(s.notifications_enabled); // untouched
    }

    #[test]
    fn persists_via_db() {
        let db = Db::open_in_memory().unwrap();
        let mut s = AppSettings::default();
        s.units = "metric".into();
        s.save(&db).unwrap();
        assert_eq!(AppSettings::load(&db).units, "metric");
    }
}
