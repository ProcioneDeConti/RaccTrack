//! Aviation weather from the FAA's Aviation Weather Center (aviationweather.gov).
//! Public, no key. METARs by bounding box for the map overlay; METAR + TAF by
//! station for the airport panel. Cached in `kv_cache`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::region::Area;

const METAR_TTL_MS: i64 = 5 * 60 * 1000;
const BASE: &str = "https://aviationweather.gov/api/data";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metar {
    pub icao: String,
    pub name: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub obs_time: Option<i64>,
    pub raw: String,
    pub flight_category: Option<String>, // VFR / MVFR / IFR / LIFR
    pub temp_c: Option<f64>,
    pub dewpoint_c: Option<f64>,
    pub wind_dir: Option<serde_json::Value>, // number or "VRB"
    pub wind_kt: Option<f64>,
    pub gust_kt: Option<f64>,
    pub visibility: Option<serde_json::Value>, // number or "10+"
    pub altimeter_hpa: Option<f64>,
    pub wx_string: Option<String>,
    pub clouds: Vec<Cloud>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cloud {
    pub cover: Option<String>,
    pub base_ft: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationWx {
    pub metar: Option<Metar>,
    pub taf_raw: Option<String>,
}

pub struct Weather {
    db: Arc<Db>,
    client: reqwest::Client,
}

impl Weather {
    pub fn new(db: Arc<Db>, client: reqwest::Client) -> Self {
        Self { db, client }
    }

    pub async fn metars_in(&self, area: Area) -> Result<Vec<Metar>> {
        let a = area.clamped();
        // Round the bbox so nearby viewports share a cache entry.
        let key = format!(
            "wx:metar:{:.1},{:.1},{:.1},{:.1}",
            (a.south * 2.0).floor() / 2.0,
            (a.west * 2.0).floor() / 2.0,
            (a.north * 2.0).ceil() / 2.0,
            (a.east * 2.0).ceil() / 2.0,
        );
        if let Some(json) = self.db.kv_get(&key, METAR_TTL_MS)? {
            if let Ok(v) = serde_json::from_str(&json) {
                return Ok(v);
            }
        }
        let url = format!(
            "{BASE}/metar?format=json&bbox={:.2},{:.2},{:.2},{:.2}",
            a.south, a.west, a.north, a.east
        );
        match self.fetch_metars(&url).await {
            Ok(list) => {
                let _ = self.db.kv_put(&key, &serde_json::to_string(&list)?);
                Ok(list)
            }
            Err(e) => {
                tracing::debug!("metar bbox fetch failed: {e}");
                Ok(self
                    .db
                    .kv_get_stale(&key)?
                    .and_then(|j| serde_json::from_str(&j).ok())
                    .unwrap_or_default())
            }
        }
    }

    pub async fn station(&self, icao: &str) -> Result<StationWx> {
        let icao = icao.trim().to_uppercase();
        let key = format!("wx:stn:{icao}");
        if let Some(json) = self.db.kv_get(&key, METAR_TTL_MS)? {
            if let Ok(v) = serde_json::from_str(&json) {
                return Ok(v);
            }
        }
        let metar = self
            .fetch_metars(&format!("{BASE}/metar?format=json&ids={icao}"))
            .await
            .ok()
            .and_then(|mut v| v.drain(..).next());
        let taf_raw = self
            .fetch_text(&format!("{BASE}/taf?format=raw&ids={icao}"))
            .await
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let wx = StationWx { metar, taf_raw };
        let _ = self.db.kv_put(&key, &serde_json::to_string(&wx)?);
        Ok(wx)
    }

    async fn fetch_metars(&self, url: &str) -> Result<Vec<Metar>> {
        let resp = self
            .client
            .get(url)
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(anyhow!("aviationweather HTTP {}", resp.status()));
        }
        let raw: Vec<RawMetar> = resp.json().await?;
        Ok(raw.into_iter().filter_map(RawMetar::into_metar).collect())
    }

    async fn fetch_text(&self, url: &str) -> Result<String> {
        let resp = self
            .client
            .get(url)
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        Ok(resp.text().await?)
    }
}

#[derive(Deserialize)]
struct RawMetar {
    #[serde(rename = "icaoId")]
    icao_id: Option<String>,
    name: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    #[serde(rename = "obsTime")]
    obs_time: Option<i64>,
    #[serde(rename = "rawOb")]
    raw_ob: Option<String>,
    #[serde(rename = "fltCat")]
    flt_cat: Option<String>,
    temp: Option<f64>,
    dewp: Option<f64>,
    wdir: Option<serde_json::Value>,
    wspd: Option<f64>,
    wgst: Option<f64>,
    visib: Option<serde_json::Value>,
    altim: Option<f64>,
    #[serde(rename = "wxString")]
    wx_string: Option<String>,
    #[serde(default)]
    clouds: Vec<RawCloud>,
}

#[derive(Deserialize)]
struct RawCloud {
    cover: Option<String>,
    base: Option<f64>,
}

impl RawMetar {
    fn into_metar(self) -> Option<Metar> {
        let icao = self.icao_id?;
        let (lat, lon) = (self.lat?, self.lon?);
        Some(Metar {
            icao,
            name: self.name,
            lat,
            lon,
            obs_time: self.obs_time.map(|t| t * 1000),
            raw: self.raw_ob.unwrap_or_default(),
            flight_category: self.flt_cat,
            temp_c: self.temp,
            dewpoint_c: self.dewp,
            wind_dir: self.wdir,
            wind_kt: self.wspd,
            gust_kt: self.wgst,
            visibility: self.visib,
            altimeter_hpa: self.altim,
            wx_string: self.wx_string,
            clouds: self
                .clouds
                .into_iter()
                .map(|c| Cloud {
                    cover: c.cover,
                    base_ft: c.base,
                })
                .collect(),
        })
    }
}
