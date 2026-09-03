//! Basemap tile cache. A custom URI scheme (`ofmtiles`) serves OpenFreeMap
//! resources from SQLite, fetching and storing on a miss, so panning fills the
//! cache and a pre-downloaded area keeps working offline.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;

use crate::db::Db;
use crate::region::Area;
use crate::util::now_ms;

pub const UPSTREAM: &str = "https://tiles.openfreemap.org";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TileCacheStats {
    pub tiles: i64,
    pub bytes: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub done: usize,
    pub total: usize,
    pub finished: bool,
}

pub struct TileCache {
    db: Arc<Db>,
    client: reqwest::Client,
    max_bytes: AtomicI64,
    since_sweep: AtomicI64,
}

pub struct TileBytes {
    pub data: Vec<u8>,
    pub content_type: String,
}

impl TileCache {
    pub fn new(db: Arc<Db>, client: reqwest::Client, max_mb: u64) -> Self {
        Self {
            db,
            client,
            max_bytes: AtomicI64::new((max_mb as i64) * 1024 * 1024),
            since_sweep: AtomicI64::new(0),
        }
    }

    pub fn set_max_mb(&self, max_mb: u64) {
        self.max_bytes
            .store((max_mb as i64) * 1024 * 1024, Ordering::Relaxed);
        let _ = self.enforce_limit();
    }

    /// Serve a resource by its path (everything after the host), e.g.
    /// `planet/20260830_.../5/9/12.pbf` or `styles/dark`.
    pub async fn serve(&self, path: &str) -> Result<TileBytes> {
        let path = path.trim_start_matches('/');
        if let Some(hit) = self.get(path)? {
            self.touch(path);
            return Ok(hit);
        }
        match self.fetch_upstream(path).await {
            Ok(fetched) => {
                self.put(path, &fetched)?;
                self.maybe_sweep(fetched.data.len() as i64);
                tracing::trace!("tile cache: fetched {path} ({} B)", fetched.data.len());
                Ok(fetched)
            }
            Err(e) => {
                tracing::warn!("tile cache: {path} -> {e}");
                Err(e)
            }
        }
    }

    async fn fetch_upstream(&self, path: &str) -> Result<TileBytes> {
        let url = format!("{UPSTREAM}/{path}");
        let resp = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .with_context(|| format!("fetch {url}"))?;
        if !resp.status().is_success() {
            return Err(anyhow!("upstream {} for {path}", resp.status()));
        }
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| guess_content_type(path).to_string());
        let data = resp.bytes().await?.to_vec();
        Ok(TileBytes { data, content_type })
    }

    fn get(&self, path: &str) -> Result<Option<TileBytes>> {
        self.db.with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT data, content_type FROM tiles WHERE path = ?1")?;
            let mut rows = stmt.query([path])?;
            if let Some(r) = rows.next()? {
                Ok(Some(TileBytes {
                    data: r.get(0)?,
                    content_type: r.get(1)?,
                }))
            } else {
                Ok(None)
            }
        })
    }

    fn put(&self, path: &str, tb: &TileBytes) -> Result<()> {
        let now = now_ms();
        self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO tiles(path, data, content_type, bytes, fetched_at, last_used)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(path) DO UPDATE SET
                   data = excluded.data, content_type = excluded.content_type,
                   bytes = excluded.bytes, fetched_at = excluded.fetched_at,
                   last_used = excluded.last_used",
                rusqlite::params![path, tb.data, tb.content_type, tb.data.len() as i64, now],
            )?;
            Ok(())
        })
    }

    fn touch(&self, path: &str) {
        let _ = self.db.with_conn(|c| {
            c.execute(
                "UPDATE tiles SET last_used = ?2 WHERE path = ?1",
                rusqlite::params![path, now_ms()],
            )?;
            Ok(())
        });
    }

    pub fn stats(&self) -> Result<TileCacheStats> {
        self.db.with_conn(|c| {
            let (tiles, bytes): (i64, i64) = c.query_row(
                "SELECT COUNT(*), COALESCE(SUM(bytes), 0) FROM tiles",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            Ok(TileCacheStats { tiles, bytes })
        })
    }

    pub fn clear(&self) -> Result<()> {
        self.db.with_conn(|c| {
            c.execute("DELETE FROM tiles", [])?;
            Ok(())
        })
    }

    fn maybe_sweep(&self, added: i64) {
        let n = self.since_sweep.fetch_add(added, Ordering::Relaxed) + added;
        if n > 8 * 1024 * 1024 {
            self.since_sweep.store(0, Ordering::Relaxed);
            let _ = self.enforce_limit();
        }
    }

    fn enforce_limit(&self) -> Result<()> {
        let max = self.max_bytes.load(Ordering::Relaxed);
        self.db.with_conn(|c| {
            let total: i64 =
                c.query_row("SELECT COALESCE(SUM(bytes),0) FROM tiles", [], |r| r.get(0))?;
            if total <= max {
                return Ok(());
            }
            let mut over = total - max;
            let mut stmt =
                c.prepare("SELECT path, bytes FROM tiles ORDER BY last_used ASC")?;
            let victims: Vec<(String, i64)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .filter_map(|x| x.ok())
                .collect();
            for (path, bytes) in victims {
                if over <= 0 {
                    break;
                }
                c.execute("DELETE FROM tiles WHERE path = ?1", [&path])?;
                over -= bytes;
            }
            Ok(())
        })
    }

    /// Download every tile covering `area` for zooms `min_z..=max_z`, plus the
    /// style, glyphs and sprite so the area renders fully offline.
    pub async fn download_area<F: Fn(DownloadProgress)>(
        &self,
        area: Area,
        min_z: u8,
        max_z: u8,
        progress: F,
    ) -> Result<()> {
        let area = area.clamped();
        let template = self.vector_tile_template().await?;

        // Build the work list.
        let mut jobs: Vec<String> = vec![
            "styles/dark".to_string(),
            "sprites/ofm_f384/ofm.json".to_string(),
            "sprites/ofm_f384/ofm.png".to_string(),
            "sprites/ofm_f384/ofm@2x.json".to_string(),
            "sprites/ofm_f384/ofm@2x.png".to_string(),
        ];
        // Latin + common symbol glyph ranges are enough for the basemap labels.
        for fs in ["Noto Sans Regular", "Noto Sans Bold"] {
            for r in (0..8192).step_by(256) {
                jobs.push(format!("fonts/{}/{}-{}.pbf", urlenc(fs), r, r + 255));
            }
        }
        for z in min_z..=max_z {
            for (x, y) in tiles_in_area(&area, z) {
                jobs.push(
                    template
                        .replace("{z}", &z.to_string())
                        .replace("{x}", &x.to_string())
                        .replace("{y}", &y.to_string()),
                );
                if z <= 6 {
                    jobs.push(format!("natural_earth/ne2sr/{z}/{x}/{y}.png"));
                }
            }
        }

        let total = jobs.len();
        progress(DownloadProgress {
            done: 0,
            total,
            finished: false,
        });

        for (i, job) in jobs.iter().enumerate() {
            if let Err(e) = self.serve(job).await {
                tracing::debug!("tile prefetch failed for {job}: {e}");
            }
            if i % 16 == 0 || i + 1 == total {
                progress(DownloadProgress {
                    done: i + 1,
                    total,
                    finished: false,
                });
            }
        }

        self.enforce_limit()?;
        progress(DownloadProgress {
            done: total,
            total,
            finished: true,
        });
        Ok(())
    }

    async fn vector_tile_template(&self) -> Result<String> {
        let tb = self.serve("planet").await?;
        let json: serde_json::Value = serde_json::from_slice(&tb.data)?;
        let t = json["tiles"][0]
            .as_str()
            .ok_or_else(|| anyhow!("planet tilejson has no tiles[0]"))?;
        // Strip the upstream host, keep the path with {z}/{x}/{y} placeholders.
        Ok(t.trim_start_matches(UPSTREAM).trim_start_matches('/').to_string())
    }
}

fn urlenc(s: &str) -> String {
    s.replace(' ', "%20")
}

fn guess_content_type(path: &str) -> &'static str {
    if path.ends_with(".pbf") {
        "application/x-protobuf"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".json") || !path.contains('.') {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

/// Slippy-map tile coordinates covering an area at a given zoom.
fn tiles_in_area(area: &Area, z: u8) -> Vec<(u32, u32)> {
    let n = 2f64.powi(z as i32);
    let x0 = lon_to_x(area.west, n).min(lon_to_x(area.east, n));
    let x1 = lon_to_x(area.west, n).max(lon_to_x(area.east, n));
    let y0 = lat_to_y(area.north, n).min(lat_to_y(area.south, n));
    let y1 = lat_to_y(area.north, n).max(lat_to_y(area.south, n));
    let max = (n as u32).saturating_sub(1);

    let mut out = Vec::new();
    for x in x0..=x1.min(max) {
        for y in y0..=y1.min(max) {
            out.push((x, y));
        }
    }
    out
}

fn lon_to_x(lon: f64, n: f64) -> u32 {
    (((lon + 180.0) / 360.0 * n).floor().max(0.0)) as u32
}

fn lat_to_y(lat: f64, n: f64) -> u32 {
    let r = lat.to_radians();
    (((1.0 - (r.tan() + 1.0 / r.cos()).ln() / std::f64::consts::PI) / 2.0 * n)
        .floor()
        .max(0.0)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_math_matches_known_values() {
        // z=1 splits the world into 2x2.
        let west = Area {
            west: -170.0,
            south: 10.0,
            east: -100.0,
            north: 60.0,
        };
        let t = tiles_in_area(&west, 1);
        assert!(t.contains(&(0, 0)));
        assert!(t.iter().all(|(x, y)| *x < 2 && *y < 2));
    }

    #[test]
    fn na_area_z3_is_bounded() {
        let t = tiles_in_area(&Area::NORTH_AMERICA, 3);
        assert!(!t.is_empty());
        assert!(t.len() <= 64);
    }

    #[test]
    fn content_type_guess() {
        assert_eq!(guess_content_type("a/b/1.pbf"), "application/x-protobuf");
        assert_eq!(guess_content_type("styles/dark"), "application/json");
    }
}
