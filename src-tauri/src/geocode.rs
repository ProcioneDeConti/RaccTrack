//! Home-location search: raw coordinates parsed locally, everything else
//! (states, counties, cities, ZIPs, addresses) via the key-free Photon geocoder
//! (https://photon.komoot.io/), cached in SQLite.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::util::now_ms;

const CACHE_TTL_MS: i64 = 30 * 24 * 3600 * 1000;
const MIN_SPACING: Duration = Duration::from_millis(1100);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoResult {
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    /// [west, south, east, north] when the place has an extent (states, cities…).
    pub bbox: Option<[f64; 4]>,
    pub kind: String,
}

pub struct Geocoder {
    db: Arc<Db>,
    client: reqwest::Client,
    last_call: Mutex<Option<Instant>>,
}

impl Geocoder {
    pub fn new(db: Arc<Db>, client: reqwest::Client) -> Self {
        Self {
            db,
            client,
            last_call: Mutex::new(None),
        }
    }

    pub async fn search(&self, query: &str) -> Result<Vec<GeoResult>> {
        let q = query.trim();
        if q.is_empty() {
            return Err(anyhow!("empty search"));
        }

        if let Some(r) = parse_coords(q) {
            return Ok(vec![r]);
        }

        let key = q.to_lowercase();
        if let Some(cached) = self.cached(&key)? {
            return Ok(cached);
        }

        self.throttle().await;
        let results = self.fetch_photon(q).await?;
        self.store(&key, &results)?;
        Ok(results)
    }

    async fn throttle(&self) {
        let wait = {
            let last = self.last_call.lock();
            last.map(|t| MIN_SPACING.saturating_sub(t.elapsed()))
                .unwrap_or_default()
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
        *self.last_call.lock() = Some(Instant::now());
    }

    async fn fetch_photon(&self, query: &str) -> Result<Vec<GeoResult>> {
        let url = format!(
            "https://photon.komoot.io/api/?q={}&limit=6&lang=en",
            urlencoding(query)
        );
        let resp = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(12))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(anyhow!("geocoder returned HTTP {}", resp.status()));
        }
        let body: PhotonResponse = resp.json().await?;
        Ok(body
            .features
            .into_iter()
            .filter_map(feature_to_result)
            .collect())
    }

    fn cached(&self, key: &str) -> Result<Option<Vec<GeoResult>>> {
        self.db.with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT json, fetched_at FROM geocode_cache WHERE query = ?1")?;
            let mut rows = stmt.query([key])?;
            if let Some(r) = rows.next()? {
                let json: String = r.get(0)?;
                let fetched_at: i64 = r.get(1)?;
                if now_ms() - fetched_at < CACHE_TTL_MS {
                    if let Ok(v) = serde_json::from_str::<Vec<GeoResult>>(&json) {
                        return Ok(Some(v));
                    }
                }
            }
            Ok(None)
        })
    }

    fn store(&self, key: &str, results: &[GeoResult]) -> Result<()> {
        let json = serde_json::to_string(results)?;
        self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO geocode_cache(query, json, fetched_at) VALUES(?1, ?2, ?3)
                 ON CONFLICT(query) DO UPDATE SET json = excluded.json, fetched_at = excluded.fetched_at",
                rusqlite::params![key, json, now_ms()],
            )?;
            Ok(())
        })
    }
}

#[derive(Deserialize)]
struct PhotonResponse {
    #[serde(default)]
    features: Vec<PhotonFeature>,
}

#[derive(Deserialize)]
struct PhotonFeature {
    geometry: PhotonGeometry,
    properties: PhotonProps,
}

#[derive(Deserialize)]
struct PhotonGeometry {
    coordinates: [f64; 2], // [lon, lat]
}

#[derive(Deserialize, Default)]
struct PhotonProps {
    name: Option<String>,
    housenumber: Option<String>,
    street: Option<String>,
    postcode: Option<String>,
    city: Option<String>,
    district: Option<String>,
    county: Option<String>,
    state: Option<String>,
    country: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    osm_value: Option<String>,
    /// [minLon, maxLat, maxLon, minLat]
    extent: Option<[f64; 4]>,
}

fn feature_to_result(f: PhotonFeature) -> Option<GeoResult> {
    let [lon, lat] = f.geometry.coordinates;
    let p = f.properties;

    let mut parts: Vec<String> = Vec::new();
    match (&p.housenumber, &p.street) {
        (Some(n), Some(s)) => parts.push(format!("{n} {s}")),
        (None, Some(s)) => parts.push(s.clone()),
        _ => {}
    }
    for opt in [&p.name, &p.district, &p.city, &p.county, &p.state] {
        if let Some(v) = opt {
            if !parts.iter().any(|x| x.eq_ignore_ascii_case(v)) {
                parts.push(v.clone());
            }
        }
    }
    if let Some(pc) = &p.postcode {
        if !parts.iter().any(|x| x.contains(pc.as_str())) {
            parts.push(pc.clone());
        }
    }
    if let Some(cc) = &p.country {
        if cc != "United States" && cc != "United States of America" {
            parts.push(cc.clone());
        }
    }

    let kind = p.kind.or(p.osm_value).unwrap_or_else(|| "place".into());
    let label = if parts.is_empty() {
        format!("{lat:.4}, {lon:.4}")
    } else {
        parts.join(", ")
    };

    let bbox = p.extent.map(|[min_lon, max_lat, max_lon, min_lat]| {
        [min_lon, min_lat, max_lon, max_lat]
    });

    Some(GeoResult {
        label,
        lat,
        lon,
        bbox,
        kind,
    })
}

/// Parse "lat, lon" / "lat lon" / "lat/lon" coordinate strings.
fn parse_coords(s: &str) -> Option<GeoResult> {
    let cleaned: String = s
        .chars()
        .map(|c| if c == '/' || c == ';' { ',' } else { c })
        .collect();
    let nums: Vec<f64> = cleaned
        .split([',', ' ', '\t'])
        .filter(|t| !t.is_empty())
        .map(|t| t.trim_end_matches(['°', 'N', 'S', 'E', 'W', 'n', 's', 'e', 'w']))
        .filter_map(|t| t.parse::<f64>().ok())
        .collect();
    if nums.len() != 2 {
        return None;
    }
    let (a, b) = (nums[0], nums[1]);

    // Disambiguate order: a value with |v| > 90 must be a longitude.
    let (lat, lon) = if a.abs() > 90.0 && b.abs() <= 90.0 {
        (b, a)
    } else if b.abs() > 90.0 && a.abs() <= 90.0 {
        (a, b)
    } else {
        (a, b) // default: "lat, lon"
    };

    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }

    Some(GeoResult {
        label: format!("{lat:.5}, {lon:.5}"),
        lat,
        lon,
        bbox: None,
        kind: "coordinates".into(),
    })
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lat_lon() {
        let r = parse_coords("41.5, -81.7").unwrap();
        assert!((r.lat - 41.5).abs() < 1e-9);
        assert!((r.lon + 81.7).abs() < 1e-9);
        assert_eq!(r.kind, "coordinates");
    }

    #[test]
    fn parses_lon_lat_when_unambiguous() {
        // first value out of latitude range -> it's the longitude
        let r = parse_coords("-122.4 47.6").unwrap();
        assert!((r.lat - 47.6).abs() < 1e-9);
        assert!((r.lon + 122.4).abs() < 1e-9);
    }

    #[test]
    fn rejects_non_coords() {
        assert!(parse_coords("Cleveland, Ohio").is_none());
        assert!(parse_coords("44139").is_none());
    }
}
