//! Shared application state managed by Tauri.

use std::sync::Arc;

use parking_lot::Mutex;

use arc_swap::ArcSwap;

use crate::airspace::Airspace;
use crate::alerts::Alerts;
use crate::charts::Charts;
use crate::config::AppSettings;
use crate::db::Db;
use crate::enrich::airports::Airports;
use crate::enrich::Enricher;
use crate::geocode::Geocoder;
use crate::poller::SourceStatus;
use crate::region::Area;
use crate::state::LiveState;
use crate::tiles::TileCache;
use crate::weather::Weather;

pub struct AppState {
    pub live: Arc<LiveState>,
    pub enricher: Arc<Enricher>,
    pub alerts: Arc<Alerts>,
    pub tiles: Arc<TileCache>,
    pub geocoder: Arc<Geocoder>,
    pub weather: Arc<Weather>,
    pub airspace: Arc<Airspace>,
    pub charts: Arc<Charts>,
    pub airports: Arc<ArcSwap<Airports>>,
    pub db: Arc<Db>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub viewport: Arc<Mutex<Option<Area>>>,
    pub status: Arc<Mutex<SourceStatus>>,
}
