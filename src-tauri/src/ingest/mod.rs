//! Aircraft ingestion: a source-agnostic trait plus HTTP implementations for
//! the free community aggregators. A future `LocalReceiverSource` (reading a
//! dump1090/readsb feed) implements the same trait and slots into the
//! orchestrator without touching the rest of the app.

pub mod model;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use model::{Aircraft, AircraftResponse, RawAircraft};
use std::time::Duration;

use crate::region::Area;

/// A single point query the API understands: centre + radius in nautical miles.
#[derive(Debug, Clone, Copy)]
pub struct PointQuery {
    pub lat: f64,
    pub lon: f64,
    pub radius_nm: f64,
}

pub const MAX_RADIUS_NM: f64 = 250.0;
const NM_PER_DEG_LAT: f64 = 60.0;
/// Keep the per-poll request budget small (rate-limit friendliness).
const MAX_QUERIES_PER_POLL: usize = 4;

/// Break an arbitrary area into a small set of point queries that together
/// cover it, each within [`MAX_RADIUS_NM`].
pub fn queries_for_area(area: &Area) -> Vec<PointQuery> {
    let area = area.clamped();
    let mid_lat = (area.south + area.north) / 2.0;
    let nm_per_deg_lon = (NM_PER_DEG_LAT * mid_lat.to_radians().cos()).max(1.0);

    let h_nm = (area.north - area.south) * NM_PER_DEG_LAT;
    let w_nm = (area.east - area.west) * nm_per_deg_lon;

    // One query is enough when the whole area fits in a 250 nm circle.
    if (h_nm / 2.0).hypot(w_nm / 2.0) <= MAX_RADIUS_NM {
        return vec![PointQuery {
            lat: mid_lat,
            lon: (area.west + area.east) / 2.0,
            radius_nm: ((h_nm / 2.0).hypot(w_nm / 2.0)).max(1.0).min(MAX_RADIUS_NM),
        }];
    }

    // Otherwise grid it. Cell edge ~ 300 nm so a cell's circumscribed circle is
    // under the cap.
    let cell_nm = 300.0_f64;
    let rows = ((h_nm / cell_nm).ceil() as usize).max(1);
    let cols = ((w_nm / cell_nm).ceil() as usize).max(1);
    let (rows, cols) = shrink_to_budget(rows, cols);

    let row_deg = (area.north - area.south) / rows as f64;
    let col_deg = (area.east - area.west) / cols as f64;
    let radius = ((row_deg * NM_PER_DEG_LAT / 2.0)
        .hypot(col_deg * nm_per_deg_lon / 2.0))
    .min(MAX_RADIUS_NM);

    let mut out = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            out.push(PointQuery {
                lat: area.south + row_deg * (r as f64 + 0.5),
                lon: area.west + col_deg * (c as f64 + 0.5),
                radius_nm: radius.max(1.0),
            });
        }
    }
    out
}

fn shrink_to_budget(mut rows: usize, mut cols: usize) -> (usize, usize) {
    while rows * cols > MAX_QUERIES_PER_POLL {
        if rows >= cols {
            rows -= 1;
        } else {
            cols -= 1;
        }
    }
    (rows.max(1), cols.max(1))
}

#[async_trait]
pub trait AircraftSource: Send + Sync {
    fn name(&self) -> &str;

    /// Fetch every aircraft matching the given queries. Implementations should
    /// de-duplicate by hex across queries.
    async fn snapshot(&self, queries: &[PointQuery]) -> Result<Vec<RawAircraft>>;

    /// Fetch a single aircraft by ICAO hex, on demand — used for an aircraft
    /// that isn't in the viewport feed (e.g. an NA-wide emergency-squawk hit).
    /// Default: not supported (empty result).
    async fn by_hex(&self, _hex: &str) -> Result<Vec<RawAircraft>> {
        Ok(Vec::new())
    }
}

/// HTTP source for the ADSBExchange-v2 compatible community APIs
/// (adsb.lol, adsb.fi, airplanes.live, ...).
pub struct HttpV2Source {
    name: &'static str,
    /// Base without trailing slash, e.g. `https://api.adsb.lol` or
    /// `https://opendata.adsb.fi/api`.
    base: String,
    client: reqwest::Client,
}

impl HttpV2Source {
    pub fn new(name: &'static str, base: impl Into<String>, client: reqwest::Client) -> Self {
        Self {
            name,
            base: base.into(),
            client,
        }
    }

    pub fn adsb_lol(client: reqwest::Client) -> Self {
        Self::new("adsb.lol", "https://api.adsb.lol", client)
    }

    pub fn adsb_fi(client: reqwest::Client) -> Self {
        Self::new("adsb.fi", "https://opendata.adsb.fi/api", client)
    }

    async fn get(&self, url: &str) -> Result<Vec<RawAircraft>> {
        let resp = self
            .client
            .get(url)
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .with_context(|| format!("request to {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("{} returned HTTP {}", self.name, status));
        }
        let body: AircraftResponse = resp
            .json()
            .await
            .with_context(|| format!("decoding {} response", self.name))?;
        Ok(body.ac)
    }
}

#[async_trait]
impl AircraftSource for HttpV2Source {
    fn name(&self) -> &str {
        self.name
    }

    async fn by_hex(&self, hex: &str) -> Result<Vec<RawAircraft>> {
        let url = format!("{}/v2/hex/{}", self.base, hex.to_lowercase());
        self.get(&url).await
    }

    async fn snapshot(&self, queries: &[PointQuery]) -> Result<Vec<RawAircraft>> {
        let mut seen = std::collections::HashMap::<String, RawAircraft>::new();
        let mut last_err = None;
        let mut any_ok = false;

        for (i, q) in queries.iter().enumerate() {
            if i > 0 {
                // Be gentle: space out the per-poll requests.
                tokio::time::sleep(Duration::from_millis(450)).await;
            }
            let url = format!(
                "{}/v2/lat/{:.5}/lon/{:.5}/dist/{}",
                self.base,
                q.lat,
                q.lon,
                q.radius_nm.round() as i64
            );
            match self.get(&url).await {
                Ok(list) => {
                    any_ok = true;
                    for a in list {
                        if let Some(hex) = a.hex.clone() {
                            seen.entry(hex).or_insert(a);
                        }
                    }
                }
                Err(e) => last_err = Some(e),
            }
        }

        if !any_ok {
            return Err(last_err.unwrap_or_else(|| anyhow!("no queries issued")));
        }
        Ok(seen.into_values().collect())
    }
}

/// Convert raw rows into normalized aircraft, tagging the source and timestamp.
pub fn normalize(raw: Vec<RawAircraft>, source: &str, now_ms: i64) -> Vec<Aircraft> {
    raw.into_iter()
        .filter_map(|r| Aircraft::from_raw(r, source, now_ms))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::Area;

    #[test]
    fn small_area_is_one_query() {
        let a = Area {
            west: -74.5,
            south: 40.0,
            east: -73.5,
            north: 41.0,
        };
        let q = queries_for_area(&a);
        assert_eq!(q.len(), 1);
        assert!(q[0].radius_nm <= MAX_RADIUS_NM);
    }

    #[test]
    fn continental_area_is_gridded_within_budget() {
        let a = Area {
            west: -125.0,
            south: 25.0,
            east: -67.0,
            north: 49.0,
        };
        let q = queries_for_area(&a);
        assert!(q.len() > 1);
        assert!(q.len() <= MAX_QUERIES_PER_POLL);
        assert!(q.iter().all(|p| p.radius_nm <= MAX_RADIUS_NM + 0.001));
    }
}
