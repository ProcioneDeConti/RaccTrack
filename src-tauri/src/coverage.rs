//! Estimated RTL-SDR reception coverage: a terrain-aware polygon around the
//! receiver, computed from real ground elevation rather than a plain circle.
//!
//! Method (the standard "effective earth radius" terrain line-of-sight
//! technique used by radio coverage tools like SPLAT!): for each bearing
//! around the receiver, walk outward sampling ground elevation, correct each
//! sample for earth curvature + atmospheric refraction (the 4/3-radius
//! model), and track the running maximum terrain elevation angle as seen
//! from the receiver. The coverage boundary on that bearing is the first
//! distance at which a target aircraft's own elevation angle (at the chosen
//! altitude) drops to or below that running-maximum terrain angle — i.e.
//! the first ridge that blocks line of sight to that altitude. Capped by the
//! plain smooth-earth radio horizon distance, `1.23 * (√h_r + √h_t)` nm
//! (heights in feet), so an unobstructed bearing still gets a sane boundary.
//!
//! Elevation data: Open-Meteo's Elevation API (free, keyless, global SRTM90
//! + ASTER30, batches up to 100 points/request) — no bundled terrain data,
//! no signup.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::db::Db;

const TTL_MS: i64 = 30 * 24 * 3600 * 1000; // terrain doesn't move; 30 days is just cache hygiene
// Open-Meteo's anonymous elevation endpoint enforces a per-minute request
// cap low enough that the original 36 bearings x 20 samples (8 batched
// requests) tripped it on a single compute. Coarser sampling keeps a whole
// compute inside 3 requests, comfortably under that limit.
const BEARING_COUNT: usize = 24; // every 15°
const SAMPLES_PER_BEARING: usize = 12;
const BATCH_SIZE: usize = 100; // Open-Meteo's per-request cap
/// Gap between sequential elevation batches, so a multi-request compute
/// doesn't present as one instantaneous burst to the rate limiter.
const BATCH_GAP: std::time::Duration = std::time::Duration::from_millis(250);
const NM_TO_M: f64 = 1852.0;
const FT_TO_M: f64 = 0.3048;
const EARTH_RADIUS_M: f64 = 6_371_000.0;
/// Standard 4/3-effective-earth-radius model for radio LOS (accounts for
/// typical atmospheric refraction bending the ray slightly around the
/// curve). `k` in the usual `k * R_earth` notation.
const K_FACTOR: f64 = 4.0 / 3.0;
/// Sane ceiling regardless of altitude — nothing realistic needs more.
const MAX_HORIZON_NM: f64 = 300.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageBearing {
    pub bearing_deg: f64,
    pub distance_nm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageResult {
    pub receiver_lat: f64,
    pub receiver_lon: f64,
    pub receiver_ground_elev_ft: f64,
    pub target_alt_ft: u32,
    pub antenna_height_ft: f64,
    pub points: Vec<CoverageBearing>,
}

pub struct Coverage {
    db: Arc<Db>,
    client: reqwest::Client,
}

impl Coverage {
    pub fn new(db: Arc<Db>, client: reqwest::Client) -> Self {
        Self { db, client }
    }

    pub async fn compute(
        &self,
        lat: f64,
        lon: f64,
        antenna_height_ft: f64,
        target_alt_ft: u32,
    ) -> Result<CoverageResult> {
        let key = format!(
            "coverage:{lat:.4},{lon:.4},{antenna_height_ft:.0},{target_alt_ft}"
        );
        if let Some(json) = self.db.kv_get(&key, TTL_MS)? {
            if let Ok(v) = serde_json::from_str(&json) {
                return Ok(v);
            }
        }

        let result = self
            .compute_uncached(lat, lon, antenna_height_ft, target_alt_ft)
            .await?;
        let _ = self.db.kv_put(&key, &serde_json::to_string(&result)?);
        Ok(result)
    }

    async fn compute_uncached(
        &self,
        lat: f64,
        lon: f64,
        antenna_height_ft: f64,
        target_alt_ft: u32,
    ) -> Result<CoverageResult> {
        let receiver_elev_m = self.elevation(&[(lat, lon)]).await?[0];
        let receiver_elev_ft = receiver_elev_m / FT_TO_M;
        let receiver_height_ft = receiver_elev_ft + antenna_height_ft;

        let horizon_nm = (1.23 * (receiver_height_ft.max(0.0).sqrt()
            + (target_alt_ft as f64).max(0.0).sqrt()))
        .min(MAX_HORIZON_NM);

        // Sample distances along every bearing, in one flat list, so we can
        // batch the elevation lookups regardless of bearing/sample layout.
        let sample_distances: Vec<f64> = (1..=SAMPLES_PER_BEARING)
            .map(|i| horizon_nm * i as f64 / SAMPLES_PER_BEARING as f64)
            .collect();

        let mut sample_coords = Vec::with_capacity(BEARING_COUNT * SAMPLES_PER_BEARING);
        for b in 0..BEARING_COUNT {
            let bearing_deg = b as f64 * (360.0 / BEARING_COUNT as f64);
            for &d_nm in &sample_distances {
                sample_coords.push(destination(lat, lon, d_nm, bearing_deg));
            }
        }
        let elevations_m = self.elevation(&sample_coords).await?;

        let mut points = Vec::with_capacity(BEARING_COUNT);
        for b in 0..BEARING_COUNT {
            let bearing_deg = b as f64 * (360.0 / BEARING_COUNT as f64);
            let base = b * SAMPLES_PER_BEARING;
            let mut boundary_nm = horizon_nm;
            let mut max_terrain_angle = f64::NEG_INFINITY;

            for (i, &d_nm) in sample_distances.iter().enumerate() {
                let d_m = d_nm * NM_TO_M;
                let curvature_drop_m = (d_m * d_m) / (2.0 * K_FACTOR * EARTH_RADIUS_M);
                let terrain_elev_ft = elevations_m[base + i] / FT_TO_M;
                let terrain_eff_ft = terrain_elev_ft - curvature_drop_m / FT_TO_M;
                let target_eff_ft = target_alt_ft as f64 - curvature_drop_m / FT_TO_M;

                let terrain_angle = (terrain_eff_ft - receiver_height_ft).atan2(d_m / FT_TO_M);
                let target_angle = (target_eff_ft - receiver_height_ft).atan2(d_m / FT_TO_M);

                if target_angle <= max_terrain_angle {
                    boundary_nm = d_nm;
                    break;
                }
                if terrain_angle > max_terrain_angle {
                    max_terrain_angle = terrain_angle;
                }
            }

            points.push(CoverageBearing { bearing_deg, distance_nm: boundary_nm });
        }

        Ok(CoverageResult {
            receiver_lat: lat,
            receiver_lon: lon,
            receiver_ground_elev_ft: receiver_elev_ft,
            target_alt_ft,
            antenna_height_ft,
            points,
        })
    }

    /// Batched Open-Meteo elevation lookup, `(lat, lon)` in, meters out, same
    /// order. Free, keyless, global (SRTM90 + ASTER30).
    async fn elevation(&self, coords: &[(f64, f64)]) -> Result<Vec<f64>> {
        #[derive(Deserialize)]
        struct Resp {
            elevation: Vec<f64>,
        }
        #[derive(Deserialize)]
        struct ErrResp {
            reason: String,
        }

        let chunks: Vec<_> = coords.chunks(BATCH_SIZE).collect();
        let mut out = Vec::with_capacity(coords.len());
        for (i, chunk) in chunks.iter().enumerate() {
            if i > 0 {
                tokio::time::sleep(BATCH_GAP).await;
            }
            let lats: Vec<String> = chunk.iter().map(|(la, _)| format!("{la:.5}")).collect();
            let lons: Vec<String> = chunk.iter().map(|(_, lo)| format!("{lo:.5}")).collect();
            let url = format!(
                "https://api.open-meteo.com/v1/elevation?latitude={}&longitude={}",
                lats.join(","),
                lons.join(",")
            );
            let body = self.client.get(&url).send().await?.text().await?;
            let resp: Resp = serde_json::from_str(&body).map_err(|_| {
                match serde_json::from_str::<ErrResp>(&body) {
                    Ok(e) => anyhow!("elevation source rate limit exceeded: {}", e.reason),
                    Err(_) => anyhow!("elevation source returned an unreadable response"),
                }
            })?;
            out.extend(resp.elevation);
        }
        Ok(out)
    }
}

/// Destination point at `dist_nm` along `bearing_deg` from (lat, lon) —
/// standard spherical-earth great-circle formula.
fn destination(lat: f64, lon: f64, dist_nm: f64, bearing_deg: f64) -> (f64, f64) {
    let d = (dist_nm * NM_TO_M) / EARTH_RADIUS_M;
    let brg = bearing_deg.to_radians();
    let lat1 = lat.to_radians();
    let lon1 = lon.to_radians();

    let lat2 = (lat1.sin() * d.cos() + lat1.cos() * d.sin() * brg.cos()).asin();
    let lon2 = lon1
        + (brg.sin() * d.sin() * lat1.cos()).atan2(d.cos() - lat1.sin() * lat2.sin());

    (lat2.to_degrees(), lon2.to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_north_moves_latitude_only() {
        let (lat2, lon2) = destination(40.0, -73.0, 60.0, 0.0);
        assert!((lat2 - 41.0).abs() < 0.01, "expected ~1 deg north, got {lat2}");
        assert!((lon2 - (-73.0)).abs() < 0.001);
    }

    #[test]
    fn horizon_formula_matches_known_value() {
        // ~30ft receiver, ~35,000ft target -> classic ADS-B rule-of-thumb ~220nm.
        let h_r = 30.0_f64;
        let h_t = 35_000.0_f64;
        let nm = 1.23 * (h_r.sqrt() + h_t.sqrt());
        assert!((200.0..=245.0).contains(&nm), "got {nm}");
    }
}
