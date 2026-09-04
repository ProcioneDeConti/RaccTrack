//! North-America-wide emergency-squawk watch. Polls adsb.lol's global
//! `/v2/squawk/{7500|7600|7700}` endpoints on a slow interval (independent of
//! the map viewport) and fires an alert the first time a hex appears.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};

use crate::alerts::AlertEvent;
use crate::config::AppSettings;
use crate::history::History;
use crate::ingest::model::AircraftResponse;
use crate::region::Area;
use crate::state::{AircraftEvent, EventKind};
use crate::util::now_ms;

const INTERVAL: Duration = Duration::from_secs(75);
const CODES: [(&str, &str); 3] = [
    ("7500", "squawk 7500 — unlawful interference (hijack)"),
    ("7600", "squawk 7600 — lost communications"),
    ("7700", "squawk 7700 — general emergency"),
];
/// Drop a hex from the "already alerted" set after this many polls without it.
const FORGET_AFTER: u32 = 3;

pub struct EmergencyWatch {
    client: reqwest::Client,
    settings: Arc<Mutex<AppSettings>>,
    history: Arc<History>,
    /// hex -> polls-since-last-seen
    seen: Mutex<HashMap<String, u32>>,
}

impl EmergencyWatch {
    pub fn new(
        client: reqwest::Client,
        settings: Arc<Mutex<AppSettings>>,
        history: Arc<History>,
    ) -> Self {
        Self {
            client,
            settings,
            history,
            seen: Mutex::new(HashMap::new()),
        }
    }

    pub async fn run(self: Arc<Self>, app: AppHandle) {
        // Let app startup / the poller settle before adding traffic.
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            let mut extra = Duration::ZERO;
            if self.settings.lock().emergency_watch_enabled {
                extra = self.poll(&app).await;
            }
            tokio::time::sleep(INTERVAL + extra).await;
        }
    }

    /// Returns extra cool-down to add after this cycle (on rate limiting).
    async fn poll(&self, app: &AppHandle) -> Duration {
        // hex -> (squawk code, human reason)
        let mut present: HashMap<String, (&'static str, &'static str)> = HashMap::new();
        let mut rate_limited = false;

        for (i, (code, reason)) in CODES.iter().enumerate() {
            if i > 0 {
                tokio::time::sleep(Duration::from_millis(700)).await;
            }
            match self.fetch(code).await {
                Ok(hexes) => {
                    for h in hexes {
                        present.entry(h).or_insert((code, reason));
                    }
                }
                Err(e) => {
                    if e.to_string().contains("429") {
                        rate_limited = true;
                    }
                    tracing::debug!("emergency watch {code} failed: {e}");
                }
            }
        }

        let now = now_ms();
        let mut seen = self.seen.lock();

        let record_history = self.settings.lock().history_enabled;
        for (hex, (code, reason)) in &present {
            if !seen.contains_key(hex) {
                let _ = app.emit(
                    "alert",
                    &AlertEvent {
                        hex: hex.clone(),
                        reason: format!("{reason} (North America)"),
                        watch_id: None,
                        emergency: true,
                        at: now,
                    },
                );
                if record_history {
                    self.history.record(&[AircraftEvent {
                        hex: hex.clone(),
                        at: now,
                        kind: EventKind::Emergency,
                        flight: None,
                        from: None,
                        to: Some(code.to_string()),
                        lat: None,
                        lon: None,
                    }]);
                }
                tracing::info!("emergency watch: {hex} {reason}");
            }
            seen.insert(hex.clone(), 0);
        }

        // Age out hexes no longer present.
        seen.retain(|hex, misses| {
            if present.contains_key(hex) {
                true
            } else {
                *misses += 1;
                *misses < FORGET_AFTER
            }
        });

        let _ = app.emit("emergency-count", present.len());

        if rate_limited {
            Duration::from_secs(120)
        } else {
            Duration::ZERO
        }
    }

    async fn fetch(&self, code: &str) -> anyhow::Result<Vec<String>> {
        // adsb.lol has been intermittently unreachable; fall back to adsb.fi
        // (which uses `/v2/sqk/` rather than `/v2/squawk/`).
        let urls = [
            format!("https://api.adsb.lol/v2/squawk/{code}"),
            format!("https://opendata.adsb.fi/api/v2/sqk/{code}"),
        ];
        let mut last_err: Option<anyhow::Error> = None;
        for url in urls {
            match self.fetch_url(&url).await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    tracing::debug!("emergency watch {code}: {url} failed: {e}");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no squawk endpoints")))
    }

    async fn fetch_url(&self, url: &str) -> anyhow::Result<Vec<String>> {
        let resp = self
            .client
            .get(url)
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP {}", resp.status());
        }
        let body: AircraftResponse = resp.json().await?;
        Ok(body
            .ac
            .into_iter()
            .filter_map(|a| {
                let hex = a.hex?.to_lowercase();
                // Keep only aircraft with a position inside North America.
                match (a.lat, a.lon) {
                    (Some(la), Some(lo)) if Area::NORTH_AMERICA.contains(la, lo) => Some(hex),
                    (None, None) => Some(hex), // no position — include, better safe
                    _ => None,
                }
            })
            .collect())
    }
}
