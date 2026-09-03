//! Background polling loop: reads the current viewport, fetches from the
//! configured sources in priority order, merges into live state, and emits
//! diffs + alerts + source status to the frontend.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::alerts::Alerts;
use crate::config::AppSettings;
use crate::enrich::Enricher;
use crate::ingest::{normalize, queries_for_area, AircraftSource};
use crate::region::Area;
use crate::state::LiveState;
use crate::util::now_ms;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub active_source: String,
    pub healthy: bool,
    pub last_error: Option<String>,
    pub last_success_at: Option<i64>,
    pub requests_last_minute: u32,
}

impl Default for SourceStatus {
    fn default() -> Self {
        Self {
            active_source: "starting…".into(),
            healthy: false,
            last_error: None,
            last_success_at: None,
            requests_last_minute: 0,
        }
    }
}

pub struct Poller {
    pub live: Arc<LiveState>,
    pub enricher: Arc<Enricher>,
    pub alerts: Arc<Alerts>,
    pub geofences: Arc<crate::geofence::Geofences>,
    pub sources: Vec<Arc<dyn AircraftSource>>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub viewport: Arc<Mutex<Option<Area>>>,
    pub status: Arc<Mutex<SourceStatus>>,
    req_log: Mutex<VecDeque<i64>>,
    /// source name -> epoch ms until which it should be skipped (after a 429).
    cooldown: Mutex<HashMap<String, i64>>,
}

/// How long to rest a source after it rate-limits us.
const RATE_LIMIT_COOLDOWN_MS: i64 = 90_000;

impl Poller {
    pub fn new(
        live: Arc<LiveState>,
        enricher: Arc<Enricher>,
        alerts: Arc<Alerts>,
        geofences: Arc<crate::geofence::Geofences>,
        sources: Vec<Arc<dyn AircraftSource>>,
        settings: Arc<Mutex<AppSettings>>,
        viewport: Arc<Mutex<Option<Area>>>,
        status: Arc<Mutex<SourceStatus>>,
    ) -> Self {
        Self {
            live,
            enricher,
            alerts,
            geofences,
            sources,
            settings,
            viewport,
            status,
            req_log: Mutex::new(VecDeque::new()),
            cooldown: Mutex::new(HashMap::new()),
        }
    }

    /// Order sources per the user's `source_order` preference.
    fn ordered_sources(&self) -> Vec<Arc<dyn AircraftSource>> {
        let order = self.settings.lock().source_order.clone();
        let mut ranked: Vec<_> = self.sources.iter().cloned().collect();
        ranked.sort_by_key(|s| {
            order
                .iter()
                .position(|n| n == s.name())
                .unwrap_or(usize::MAX)
        });
        ranked
    }

    fn note_requests(&self, n: usize) -> u32 {
        let now = now_ms();
        let mut log = self.req_log.lock();
        for _ in 0..n {
            log.push_back(now);
        }
        while log.front().map(|t| now - t > 60_000).unwrap_or(false) {
            log.pop_front();
        }
        log.len() as u32
    }

    pub async fn run(self: Arc<Self>, app: AppHandle) {
        loop {
            let interval = self.settings.lock().poll_interval_ms.max(1_000);
            let area = *self.viewport.lock();

            if let Some(area) = area {
                if area.intersects_region() {
                    self.poll_once(&app, area).await;
                }
            }

            tokio::time::sleep(Duration::from_millis(interval)).await;
        }
    }

    async fn poll_once(&self, app: &AppHandle, area: Area) {
        let queries = queries_for_area(&area);
        let sources = self.ordered_sources();
        let mut last_err: Option<String> = None;
        let now_before = now_ms();

        for source in sources {
            if let Some(until) = self.cooldown.lock().get(source.name()).copied() {
                if now_before < until {
                    last_err = Some(format!(
                        "{}: cooling down for {}s after rate limit",
                        source.name(),
                        (until - now_before) / 1000
                    ));
                    continue;
                }
            }

            match source.snapshot(&queries).await {
                Ok(raw) => {
                    let now = now_ms();
                    let mut list = normalize(raw, source.name(), now);
                    // Defense in depth: never surface aircraft outside the
                    // North America coverage box even if a query edge caught one.
                    list.retain(|ac| match (ac.lat, ac.lon) {
                        (Some(la), Some(lo)) => Area::NORTH_AMERICA.contains(la, lo),
                        _ => true,
                    });
                    for ac in &mut list {
                        self.enricher.fill_identity(ac);
                    }
                    let feed_total = list.len() as u64;
                    let diff = self.live.ingest(list, feed_total, now);

                    let mut alerts = self.alerts.evaluate(&diff);
                    alerts.extend(self.geofences.evaluate(&diff));
                    let _ = app.emit("aircraft-diff", &diff);
                    for ev in alerts {
                        let _ = app.emit("alert", &ev);
                    }

                    self.cooldown.lock().remove(source.name());

                    let rpm = self.note_requests(queries.len());
                    {
                        let mut st = self.status.lock();
                        if st.active_source != source.name() {
                            tracing::info!(
                                "feeding from {} ({} aircraft in view)",
                                source.name(),
                                diff.total
                            );
                        }
                        st.active_source = source.name().to_string();
                        st.healthy = true;
                        st.last_error = None;
                        st.last_success_at = Some(now);
                        st.requests_last_minute = rpm;
                    }
                    let _ = app.emit("source-status", &*self.status.lock());
                    return;
                }
                Err(e) => {
                    let msg = e.to_string();
                    last_err = Some(format!("{}: {msg}", source.name()));
                    if msg.contains("429") {
                        let until = now_ms() + RATE_LIMIT_COOLDOWN_MS;
                        self.cooldown
                            .lock()
                            .insert(source.name().to_string(), until);
                        tracing::warn!(
                            "source {} rate-limited; resting {}s",
                            source.name(),
                            RATE_LIMIT_COOLDOWN_MS / 1000
                        );
                    } else {
                        tracing::warn!("source {} failed: {msg}", source.name());
                    }
                }
            }
        }

        let rpm = self.note_requests(queries.len());
        {
            let mut st = self.status.lock();
            st.healthy = false;
            st.last_error = last_err;
            st.requests_last_minute = rpm;
        }
        let _ = app.emit("source-status", &*self.status.lock());
    }
}
