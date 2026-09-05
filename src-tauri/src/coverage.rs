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
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::util::now_ms;

const TTL_MS: i64 = 30 * 24 * 3600 * 1000; // terrain doesn't move; 30 days is just cache hygiene
// Open-Meteo's anonymous elevation endpoint enforces a per-minute request
// cap — the original 36 bearings x 20 samples (8 batched requests fired back
// to back) tripped it on a single compute. Rather than keep resolution low
// enough to dodge that in a handful of quick requests, we now spread far
// more batches out with a generous gap between them (plus retry/backoff if
// one still gets rate-limited) — slower wall-clock time for a one-shot,
// user-triggered compute is a fine trade for a much finer polygon.
const BEARING_COUNT: usize = 72; // every 5°
const SAMPLES_PER_BEARING: usize = 24;
const BATCH_SIZE: usize = 100; // Open-Meteo's per-request cap
/// Gap between sequential elevation batches, so a multi-request compute
/// doesn't present as one instantaneous burst to the rate limiter. 1.5s
/// (40/min) still got rate-limited in practice, so this is deliberately
/// well under 10/min — slow, but the whole point is trading time for not
/// getting throttled.
const BATCH_GAP: std::time::Duration = std::time::Duration::from_secs(6);
/// Extra backoff before retrying a batch that came back rate-limited —
/// separate from (and much longer than) `BATCH_GAP`, which only paces
/// requests that weren't rejected.
const RATE_LIMIT_BACKOFF: std::time::Duration = std::time::Duration::from_secs(20);
const MAX_BATCH_RETRIES: u32 = 6;
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

/// Live progress of an in-flight `compute()`, for the Settings panel to poll
/// (same pattern as `RtlSdrStatus`) and render as a progress bar — a compute
/// at the current resolution, paced to stay under the elevation API's rate
/// limit, takes on the order of minutes, so leaving the user looking at a
/// bare "Computing…" with no sense of progress isn't good enough.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageProgress {
    pub running: bool,
    pub batches_done: u32,
    pub batches_total: u32,
    /// Epoch ms the current compute started — lets the frontend derive an
    /// ETA from actual observed pace (`elapsed / done * remaining`) instead
    /// of hardcoding assumptions about batch timing here that would drift
    /// out of sync if `BATCH_GAP`/retries change.
    pub started_at_ms: i64,
}

pub struct Coverage {
    db: Arc<Db>,
    client: reqwest::Client,
    progress: Arc<Mutex<CoverageProgress>>,
}

impl Coverage {
    pub fn new(db: Arc<Db>, client: reqwest::Client) -> Self {
        Self {
            db,
            client,
            progress: Arc::new(Mutex::new(CoverageProgress::default())),
        }
    }

    pub fn progress(&self) -> CoverageProgress {
        *self.progress.lock()
    }

    pub async fn compute(
        &self,
        lat: f64,
        lon: f64,
        antenna_height_ft: f64,
        target_alt_ft: u32,
    ) -> Result<CoverageResult> {
        // "coverage2": bumped from "coverage" when the sampling fix below
        // landed, so installs with a cached pre-fix (overestimated) polygon
        // don't keep serving it for the rest of the 30-day TTL.
        let key = format!(
            "coverage2:{lat:.4},{lon:.4},{antenna_height_ft:.0},{target_alt_ft}"
        );
        if let Some(json) = self.db.kv_get(&key, TTL_MS)? {
            if let Ok(v) = serde_json::from_str(&json) {
                return Ok(v);
            }
        }

        // Check-and-set under one lock acquisition: the frontend already
        // disables "Recompute" while a compute is running, but there's a
        // narrow window right after the Settings panel reopens (resuming an
        // already-in-flight compute started before it was closed) where
        // that guard hasn't caught up yet. A duplicate compute wouldn't
        // just waste work — it'd double up on an elevation API that's
        // already sensitive to rate limiting, and both would stomp on this
        // same `progress` state.
        {
            let mut p = self.progress.lock();
            if p.running {
                return Err(anyhow!("a coverage compute is already running"));
            }
            // +1 for the single-point receiver-elevation lookup ahead of the
            // main bearing sweep (its own, separate `elevation()` call).
            let batches_total = 1 + (BEARING_COUNT * SAMPLES_PER_BEARING).div_ceil(BATCH_SIZE);
            *p = CoverageProgress {
                running: true,
                batches_done: 0,
                batches_total: batches_total as u32,
                started_at_ms: now_ms(),
            };
        }
        let result = self
            .compute_uncached(lat, lon, antenna_height_ft, target_alt_ft)
            .await;
        self.progress.lock().running = false;
        let result = result?;
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
        let sample_distances = sample_distances(horizon_nm, SAMPLES_PER_BEARING);

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
    /// order. Free, keyless, global (SRTM90 + ASTER30). At the current
    /// resolution this is dozens of batches — `BATCH_GAP` paces them to stay
    /// under the per-minute rate limit, and any batch that still gets
    /// rejected is retried after `RATE_LIMIT_BACKOFF` rather than failing
    /// the whole (multi-minute) compute over one transient rejection.
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

            let mut attempt = 0;
            loop {
                let body = self.client.get(&url).send().await?.text().await?;
                match serde_json::from_str::<Resp>(&body) {
                    Ok(resp) => {
                        out.extend(resp.elevation);
                        self.progress.lock().batches_done += 1;
                        break;
                    }
                    Err(_) => {
                        let reason = serde_json::from_str::<ErrResp>(&body)
                            .map(|e| e.reason)
                            .ok();
                        attempt += 1;
                        if attempt > MAX_BATCH_RETRIES {
                            return Err(match reason {
                                Some(r) => anyhow!("elevation source rate limit exceeded: {r}"),
                                None => anyhow!("elevation source returned an unreadable response"),
                            });
                        }
                        tracing::warn!(
                            "coverage: elevation batch {i} rejected (attempt {attempt}/{MAX_BATCH_RETRIES}), backing off: {reason:?}"
                        );
                        tokio::time::sleep(RATE_LIMIT_BACKOFF).await;
                    }
                }
            }
        }
        Ok(out)
    }
}

/// `count` sample distances (nm) along a bearing, out to `horizon_nm`,
/// quadratically rather than evenly spaced: a nearby obstruction (a hill a
/// few nm away, say) blocks line-of-sight to *every* farther distance on
/// that bearing once it sets `max_terrain_angle` in the caller's sweep, so it
/// matters far more than a distant one — but evenly spacing samples across
/// the whole horizon (which can be 100-300nm) put the first sample tens of
/// nm out, so anything closer than that was structurally invisible and every
/// bearing degraded to just the smooth-earth horizon regardless of real
/// nearby terrain. Squaring the fraction clusters samples close to the
/// receiver (first of 12 at horizon/144 instead of horizon/12) while still
/// reaching the full horizon at the last one.
fn sample_distances(horizon_nm: f64, count: usize) -> Vec<f64> {
    (1..=count)
        .map(|i| horizon_nm * (i as f64 / count as f64).powi(2))
        .collect()
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
    fn sample_distances_cluster_near_the_receiver() {
        let d = sample_distances(120.0, 12);
        assert_eq!(d.len(), 12);
        // Old linear spacing put the first sample at horizon/12 = 10nm,
        // missing any obstruction closer than that entirely. Quadratic
        // spacing pulls it in to horizon/144 = 0.83nm.
        assert!(d[0] < 1.0, "expected first sample well under 1nm, got {}", d[0]);
        assert!((d[11] - 120.0).abs() < 1e-9, "last sample should reach the full horizon");
        assert!(d.windows(2).all(|w| w[1] > w[0]), "samples must be strictly increasing");
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
