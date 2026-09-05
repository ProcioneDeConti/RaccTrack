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
use crate::history::History;
use crate::ingest::model::Aircraft;
use crate::ingest::{normalize, queries_for_area, AircraftSource};
use crate::region::Area;
use crate::state::{AircraftEvent, EventKind, LiveState};
use crate::util::now_ms;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    /// Every source that actually contributed aircraft this poll — can be
    /// more than one now (e.g. a direct RTL-SDR feed *and* adsb.fi merged
    /// together), not just whichever one "won".
    pub active_sources: Vec<String>,
    pub healthy: bool,
    pub last_error: Option<String>,
    pub last_success_at: Option<i64>,
    pub requests_last_minute: u32,
}

impl Default for SourceStatus {
    fn default() -> Self {
        Self {
            active_sources: Vec::new(),
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
    pub history: Arc<History>,
    pub logbook: Arc<crate::logbook::Logbook>,
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
        history: Arc<History>,
        logbook: Arc<crate::logbook::Logbook>,
        sources: Vec<Arc<dyn AircraftSource>>,
        settings: Arc<Mutex<AppSettings>>,
        viewport: Arc<Mutex<Option<Area>>>,
        status: Arc<Mutex<SourceStatus>>,
    ) -> Self {
        Self {
            live,
            enricher,
            alerts,
            history,
            logbook,
            sources,
            settings,
            viewport,
            status,
            req_log: Mutex::new(VecDeque::new()),
            cooldown: Mutex::new(HashMap::new()),
        }
    }

    /// Order sources per the user's `source_order` preference. A direct
    /// RTL-SDR dongle, when enabled, outranks even the local-receiver feed
    /// (both are dropped entirely when their toggle is off). The community
    /// aggregators are dropped entirely too when `online_sources_enabled`
    /// is off, regardless of `source_order` — a hard "local only" mode.
    ///
    /// This ranking is also the merge priority: `poll_once` always queries
    /// every "locally owned" source (RTL-SDR, local receiver — no rate
    /// limit, no reason to skip either), then walks the remaining community
    /// sources as a fallback chain (only one queried per poll — they're
    /// redundant with each other, no benefit to hitting both). Results are
    /// merged by hex, first (= highest-ranked) source wins per aircraft.
    fn ordered_sources(&self) -> Vec<Arc<dyn AircraftSource>> {
        let (order, local_on, rtlsdr_on, online_on) = {
            let s = self.settings.lock();
            (
                s.source_order.clone(),
                s.local_receiver_enabled,
                s.rtlsdr_enabled,
                s.online_sources_enabled,
            )
        };
        let is_local = |s: &Arc<dyn AircraftSource>| {
            s.name() == crate::ingest::local::NAME
        };
        let is_rtlsdr = |s: &Arc<dyn AircraftSource>| {
            s.name() == crate::ingest::rtlsdr::NAME
        };
        let mut ranked: Vec<_> = self
            .sources
            .iter()
            .filter(|s| {
                (!is_local(s) || local_on)
                    && (!is_rtlsdr(s) || rtlsdr_on)
                    && (is_local(s) || is_rtlsdr(s) || online_on)
            })
            .cloned()
            .collect();
        ranked.sort_by_key(|s| {
            if is_rtlsdr(s) {
                return 0;
            }
            if is_local(s) {
                return 1;
            }
            order
                .iter()
                .position(|n| n == s.name())
                .map(|p| p + 2)
                .unwrap_or(usize::MAX)
        });
        ranked
    }

    /// RTL-SDR / local-receiver are "ours" — free, local, always worth
    /// asking. Everything else is a shared community aggregator, only one
    /// of which gets queried per poll (a fallback chain among themselves).
    fn is_locally_owned(name: &str) -> bool {
        name == crate::ingest::local::NAME || name == crate::ingest::rtlsdr::NAME
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

    /// Queries every enabled source and merges the results by hex (first —
    /// i.e. highest-ranked, see `ordered_sources` — source to report a given
    /// aircraft wins for it), rather than stopping at the first source that
    /// responds. Locally-owned sources (RTL-SDR, local receiver) are always
    /// all queried; community aggregators remain a fallback chain among
    /// themselves (only one queried per poll — they're redundant with each
    /// other, and hitting both would just double the external request load
    /// for no benefit).
    async fn poll_once(&self, app: &AppHandle, area: Area) {
        let queries = queries_for_area(&area);
        let sources = self.ordered_sources();
        let now_before = now_ms();

        let mut merged: HashMap<String, Aircraft> = HashMap::new();
        let mut active_names: Vec<String> = Vec::new();
        let mut last_err: Option<String> = None;
        // Only community-source attempts count against the rate-limit-aware
        // req/min stat — RTL-SDR/local-receiver aren't rate-limited services.
        let mut community_requests = 0usize;

        for source in sources {
            let is_local_source = Self::is_locally_owned(source.name());

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
            if !is_local_source {
                community_requests += queries.len();
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
                    for ac in list {
                        merged.entry(ac.hex.clone()).or_insert(ac);
                    }
                    active_names.push(source.name().to_string());
                    self.cooldown.lock().remove(source.name());

                    if !is_local_source {
                        // A community source answered — that fallback chain
                        // is satisfied for this poll.
                        break;
                    }
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
                    // Local sources failing doesn't stop us trying the next
                    // (independent) one; a community source failing falls
                    // through to the next one in the fallback chain. Either
                    // way: just continue the loop.
                }
            }
        }

        let rpm = self.note_requests(community_requests);

        if active_names.is_empty() {
            let mut st = self.status.lock();
            st.healthy = false;
            st.last_error = last_err;
            st.requests_last_minute = rpm;
            drop(st);
            let _ = app.emit("source-status", &*self.status.lock());
            return;
        }

        let now = now_ms();
        let list: Vec<Aircraft> = merged.into_values().collect();
        let feed_total = list.len() as u64;
        let mut diff = self.live.ingest(list, feed_total, now);

        if self.settings.lock().logbook_enabled && !diff.added.is_empty() {
            self.logbook.record(&diff.added, now);
        }

        let places = self.settings.lock().places.clone();
        let alerts = self.alerts.evaluate(&diff, &places);

        if self.settings.lock().history_enabled {
            let mut events = std::mem::take(&mut diff.events);
            for a in alerts.iter().filter(|a| !a.emergency) {
                // Emergency AlertEvents are deliberately not turned into a
                // second history row here — `diff.events` (from
                // `state::detect_events`, above) already recorded an
                // `EventKind::Emergency`/`EmergencyClear` entry for the same
                // transition, correctly categorized for the Events panel's
                // "Emergency" filter and paired with its own clear event,
                // which this alert-derived entry duplicated without either
                // of those (it landed under "Watchlist" instead, with no
                // "cleared" counterpart). The live toast notification is
                // unaffected — that's the separate `app.emit("alert", ..)`
                // loop below, which still runs for every alert.
                events.push(AircraftEvent {
                    hex: a.hex.clone(),
                    at: a.at,
                    kind: EventKind::Alert,
                    flight: None,
                    from: None,
                    to: Some(a.reason.clone()),
                    lat: None,
                    lon: None,
                });
            }
            if !events.is_empty() {
                self.history.record(&events);
                for ev in &events {
                    let _ = app.emit("aircraft-event", ev);
                }
            }
        }

        let _ = app.emit("aircraft-diff", &diff);
        for ev in alerts {
            let _ = app.emit("alert", &ev);
        }

        {
            let mut st = self.status.lock();
            if st.active_sources != active_names {
                tracing::info!(
                    "feeding from {} ({} aircraft in view)",
                    active_names.join(" + "),
                    diff.total
                );
            }
            st.active_sources = active_names;
            st.healthy = true;
            st.last_error = None;
            st.last_success_at = Some(now);
            st.requests_last_minute = rpm;
        }
        let _ = app.emit("source-status", &*self.status.lock());
    }
}
