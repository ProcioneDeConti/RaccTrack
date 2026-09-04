//! Shared application state managed by Tauri.

use std::sync::Arc;

use parking_lot::Mutex;

use arc_swap::ArcSwap;

use crate::airspace::Airspace;
use crate::alerts::Alerts;
use crate::charts::Charts;
use crate::config::AppSettings;
use crate::datalink::Datalink;
use crate::db::Db;
use crate::enrich::airports::Airports;
use crate::enrich::Enricher;
use crate::geocode::Geocoder;
use crate::ingest::AircraftSource;
use crate::poller::SourceStatus;
use crate::region::Area;
use crate::state::LiveState;
use crate::tiles::TileCache;
use crate::weather::Weather;

pub struct AppState {
    pub live: Arc<LiveState>,
    /// Ingestion sources, shared with the poller — also used for on-demand
    /// single-aircraft (`by_hex`) lookups from the detail command.
    pub sources: Vec<Arc<dyn AircraftSource>>,
    pub enricher: Arc<Enricher>,
    pub alerts: Arc<Alerts>,
    pub tiles: Arc<TileCache>,
    pub geocoder: Arc<Geocoder>,
    pub weather: Arc<Weather>,
    pub airspace: Arc<Airspace>,
    pub charts: Arc<Charts>,
    pub datalink: Arc<Datalink>,
    pub airports: Arc<ArcSwap<Airports>>,
    pub db: Arc<Db>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub viewport: Arc<Mutex<Option<Area>>>,
    pub status: Arc<Mutex<SourceStatus>>,
}
