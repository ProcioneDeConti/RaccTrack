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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub poll_interval_ms: u64,
    pub source_order: Vec<String>,
    pub basemap: String,
    #[serde(default)]
    pub home: Option<HomeLocation>,
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

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            poll_interval_ms: 3_000,
            // adsb.fi has proven more tolerant of viewport polling; adsb.lol is
            // the fallback. Both are free community ODbL feeds.
            source_order: vec!["adsb.fi".into(), "adsb.lol".into()],
            basemap: "darkMatter".into(),
            home: None,
            contact: String::new(),
            layers: MapLayers::default(),
            range_rings_nm: default_range_rings(),
            pinned: Vec::new(),
            emergency_watch_enabled: true,
            history_enabled: true,
            history_retention_days: 30,
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
        db.get_setting(KEY)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
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
