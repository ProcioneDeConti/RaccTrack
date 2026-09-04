//! Local ADS-B receiver source — reads a dump1090-fa / readsb / tar1090
//! `aircraft.json` from a URL on the local network. It sees everything in
//! antenna range regardless of the map viewport, so it ignores the bbox
//! queries the poller passes.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use parking_lot::Mutex;

use super::{AircraftSource, PointQuery};
use crate::config::AppSettings;
use crate::ingest::model::{AircraftResponse, RawAircraft};

pub const NAME: &str = "local receiver";
pub const DEFAULT_URL: &str = "http://localhost:8080/data/aircraft.json";

pub struct LocalReceiverSource {
    client: reqwest::Client,
    settings: Arc<Mutex<AppSettings>>,
}

impl LocalReceiverSource {
    pub fn new(client: reqwest::Client, settings: Arc<Mutex<AppSettings>>) -> Self {
        Self { client, settings }
    }

    fn url(&self) -> String {
        let u = self.settings.lock().local_receiver_url.trim().to_string();
        if u.is_empty() {
            DEFAULT_URL.to_string()
        } else {
            u
        }
    }

    async fn fetch(&self) -> Result<Vec<RawAircraft>> {
        let url = self.url();
        let resp = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .with_context(|| format!("local receiver at {url}"))?;
        if !resp.status().is_success() {
            return Err(anyhow!("local receiver returned HTTP {}", resp.status()));
        }
        let body: AircraftResponse = resp
            .json()
            .await
            .context("decoding local receiver aircraft.json")?;
        Ok(body.ac)
    }
}

#[async_trait]
impl AircraftSource for LocalReceiverSource {
    fn name(&self) -> &str {
        NAME
    }

    async fn snapshot(&self, _queries: &[PointQuery]) -> Result<Vec<RawAircraft>> {
        self.fetch().await
    }

    async fn by_hex(&self, hex: &str) -> Result<Vec<RawAircraft>> {
        let all = self.fetch().await?;
        Ok(all
            .into_iter()
            .filter(|a| {
                a.hex
                    .as_deref()
                    .map(|h| h.eq_ignore_ascii_case(hex))
                    .unwrap_or(false)
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::model::Aircraft;

    // A trimmed dump1090-fa aircraft.json body (note `aircraft`, not `ac`).
    const SAMPLE: &str = r#"{
      "now": 1717430000.1, "messages": 12345,
      "aircraft": [
        {"hex":"a1b2c3","flight":"UAL123  ","alt_baro":37000,"gs":451.2,
         "track":88.1,"lat":41.9,"lon":-87.6,"squawk":"2617","category":"A3",
         "nav_qnh":1013.2,"nic":8,"rc":186,"seen_pos":0.4,"messages":900,
         "seen":0.1,"rssi":-14.2,"mlat":[],"tisb":[]},
        {"hex":"aa0000","alt_baro":"ground","gs":12.0,"lat":42.0,"lon":-87.9,
         "seen":2.0,"mlat":[],"tisb":[]}
      ]
    }"#;

    #[test]
    fn parses_dump1090_aircraft_json() {
        let body: AircraftResponse = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(body.ac.len(), 2);
        let list: Vec<Aircraft> = body
            .ac
            .into_iter()
            .filter_map(|r| Aircraft::from_raw(r, NAME, 0))
            .collect();
        assert_eq!(list.len(), 2);
        let ual = list.iter().find(|a| a.hex == "a1b2c3").unwrap();
        assert_eq!(ual.flight.as_deref(), Some("UAL123"));
        assert_eq!(ual.alt_baro, Some(37000.0));
        let gnd = list.iter().find(|a| a.hex == "aa0000").unwrap();
        assert!(gnd.on_ground);
        assert_eq!(gnd.alt_baro, None);
    }
}
