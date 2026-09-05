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
    /// This place is where the RTL-SDR dongle's antenna actually sits —
    /// drives the coverage-polygon calculation. Independent of `primary`
    /// (e.g. "Home" can stay primary for viewing while a separate "Attic
    /// antenna" place marks the receiver). At most one should be set; if
    /// several are, the coverage calculation just uses the first found.
    #[serde(default)]
    pub rtlsdr_location: bool,
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
    /// A user-drawn polygon geofence — `[lat, lon]` vertices, an open ring
    /// (no repeated closing point). When present with >= 3 points, this
    /// replaces the circular `radius_nm` test entirely for this place.
    #[serde(default)]
    pub shape: Option<Vec<[f64; 2]>>,
}

impl Default for PlaceAlert {
    fn default() -> Self {
        Self {
            enabled: false,
            radius_nm: default_alert_radius(),
            ceiling_ft: None,
            notable_only: false,
            shape: None,
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
    /// Decode ADS-B directly from a USB RTL-SDR dongle plugged into this
    /// machine, bypassing the community feeds entirely for local traffic.
    /// When enabled it's tried first, ahead of even the local receiver.
    #[serde(default)]
    pub rtlsdr_enabled: bool,
    /// `rs_rtl::DeviceId::Index` — which dongle to use when more than one is
    /// plugged in.
    #[serde(default)]
    pub rtlsdr_device_index: u32,
    /// Manual gain in tenths of dB (e.g. 297 = 29.7 dB); `None` = auto gain.
    #[serde(default)]
    pub rtlsdr_gain_tenths_db: Option<i32>,
    /// Which dongle ATC voice listening uses — independent of
    /// `rtlsdr_device_index` so a second physical dongle can run ADS-B and
    /// ATC audio at once; if it's the *same* index as the ADS-B one, `atc.rs`
    /// pauses ADS-B decoding for the duration of the listening session
    /// instead (a single dongle can't do both at the same time).
    #[serde(default)]
    pub atc_device_index: u32,
    /// Which dongle ACARS decoding uses — same independence rationale as
    /// `atc_device_index`.
    #[serde(default)]
    pub acars_device_index: u32,
    /// VHF frequencies (MHz) ACARS listens on/scans across. Defaults to the
    /// commonly-cited North America primary + secondary channels; 131.550 is
    /// by far the most active one.
    #[serde(default = "default_acars_freqs")]
    pub acars_freqs: Vec<f64>,
    /// Master switch for the community aggregators (adsb.lol / adsb.fi).
    /// Off means *only* whatever local sources (RTL-SDR / local receiver)
    /// are enabled — no online lookups at all.
    #[serde(default = "default_true")]
    pub online_sources_enabled: bool,
    /// Show the estimated RTL-SDR reception polygon on the map — computed
    /// from terrain line-of-sight around whichever place has
    /// `rtlsdr_location` set, for `coverage_target_alt_ft`.
    #[serde(default)]
    pub coverage_enabled: bool,
    #[serde(default = "default_coverage_alt")]
    pub coverage_target_alt_ft: u32,
    /// Antenna height above *ground* (not sea level) at the receiver, feet.
    #[serde(default = "default_antenna_height")]
    pub coverage_antenna_height_ft: f64,
    pub basemap: String,
    /// App chrome (panels/rail/buttons) theme — independent of the basemap's
    /// own light/dark tiles. "auto" follows the basemap (legacy behavior).
    #[serde(default = "default_ui_theme")]
    pub ui_theme: String,
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
    #[serde(default)]
    pub colors: MapColors,
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
    /// User has dismissed the first-launch safety/data disclaimer
    /// (see DISCLAIMER.md) — shown again on every launch until set.
    #[serde(default)]
    pub disclaimer_acknowledged: bool,
}

/// User overrides for the data-driven map colours (frontend `theme/colors.ts`
/// picks the defaults; this only carries overrides the user has chosen).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MapColors {
    /// Airspace category (e.g. "CLASS_B", "RESTRICTED") -> hex color.
    #[serde(default)]
    pub airspace: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub geofence_fill: Option<String>,
    #[serde(default)]
    pub geofence_line: Option<String>,
    /// Fill pattern for place-alert geofences — "solid" | "stripe" | "hash" |
    /// "dot" | "check". `None`/anything unrecognized means solid.
    #[serde(default)]
    pub geofence_pattern: Option<String>,
    #[serde(default)]
    pub coverage_fill: Option<String>,
    #[serde(default)]
    pub coverage_line: Option<String>,
    /// Same pattern vocabulary as `geofence_pattern`, for the RTL-SDR
    /// reception coverage polygon.
    #[serde(default)]
    pub coverage_pattern: Option<String>,
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
    /// Whether aircraft render at all — unlike the other (opt-in, off by
    /// default) overlays above, this defaults to *on* since aircraft are the
    /// core feature, not an optional layer; existing installs upgrading
    /// without this field in their saved settings should see no change.
    #[serde(default = "default_true")]
    pub aircraft: bool,
}

fn default_range_rings() -> Vec<f64> {
    vec![25.0, 50.0, 100.0]
}
fn default_true() -> bool {
    true
}
fn default_coverage_alt() -> u32 {
    5_000
}
fn default_antenna_height() -> f64 {
    20.0
}
fn default_history_days() -> u32 {
    30
}
fn default_local_receiver_url() -> String {
    crate::ingest::local::DEFAULT_URL.to_string()
}
fn default_ui_theme() -> String {
    "auto".into()
}
fn default_acars_freqs() -> Vec<f64> {
    vec![131.550, 130.025, 130.450, 131.725, 131.825, 136.700, 136.975]
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
            rtlsdr_enabled: false,
            rtlsdr_device_index: 0,
            atc_device_index: 0,
            acars_device_index: 0,
            acars_freqs: default_acars_freqs(),
            rtlsdr_gain_tenths_db: None,
            online_sources_enabled: true,
            coverage_enabled: false,
            coverage_target_alt_ft: default_coverage_alt(),
            coverage_antenna_height_ft: default_antenna_height(),
            basemap: "darkMatter".into(),
            ui_theme: default_ui_theme(),
            home: None,
            places: Vec::new(),
            contact: String::new(),
            // `#[derive(Default)]` ignores `#[serde(default = "default_true")]`
            // (that only applies when deserializing a JSON blob missing the
            // field) — a genuinely fresh install goes through this literal,
            // not serde, so `aircraft` needs setting explicitly here too.
            layers: MapLayers { aircraft: true, ..Default::default() },
            colors: MapColors::default(),
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
            disclaimer_acknowledged: false,
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
                    rtlsdr_location: false,
                });
            }
        }
        // Exactly one primary.
        if !self.places.is_empty() && !self.places.iter().any(|p| p.primary) {
            self.places[0].primary = true;
        }
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
        if !matches!(self.ui_theme.as_str(), "auto" | "light" | "dark") {
            self.ui_theme = default_ui_theme();
        }
        for p in &mut self.places {
            p.alert.radius_nm = p.alert.radius_nm.clamp(0.5, 250.0);
            // A degenerate or absurdly large ring isn't a usable geofence —
            // drop it back to the circle rather than persist garbage.
            if let Some(shape) = &p.alert.shape {
                if shape.len() < 3 || shape.len() > 200 {
                    p.alert.shape = None;
                }
            }
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
