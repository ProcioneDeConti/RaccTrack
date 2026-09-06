//! Shared application state managed by Tauri.

use std::sync::Arc;

use parking_lot::Mutex;

use arc_swap::ArcSwap;

use crate::acars::AcarsListener;
use crate::airspace::Airspace;
use crate::atc::AtcListener;
use crate::coverage::Coverage;
use crate::alerts::Alerts;
use crate::charts::Charts;
use crate::config::AppSettings;
use crate::datalink::Datalink;
use crate::db::Db;
use crate::enrich::airports::Airports;
use crate::enrich::navaids::Navaids;
use crate::enrich::Enricher;
use crate::geocode::Geocoder;
use crate::history::History;
use crate::ingest::{AircraftSource, RtlSdrSource};
use crate::logbook::Logbook;
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
    /// Same instance as the `RtlSdrSource` entry in `sources`, kept as its
    /// concrete type too so the Settings status command can reach its
    /// `status()` (not part of the trait — every other source is a plain
    /// request/response HTTP fetch with nothing analogous to report).
    pub rtlsdr: Arc<RtlSdrSource>,
    /// ATC voice audio (airband AM off the same RTL-SDR hardware) — see
    /// `atc.rs` for how it shares/hands off a dongle with `rtlsdr` above.
    pub atc: Arc<AtcListener>,
    /// ACARS message decoding — see `acars/mod.rs`; shares a dongle with
    /// `rtlsdr`/`atc` the same way `atc` does.
    pub acars: Arc<AcarsListener>,
    /// Direct UAT (978MHz) reception — see `ingest::uat`.
    pub uat: Arc<crate::ingest::uat::UatSource>,
    /// RTL-SDR VOR bearing/ident decoder — see `nav::vor`.
    pub vor: Arc<crate::nav::vor::VorListener>,
    pub enricher: Arc<Enricher>,
    pub alerts: Arc<Alerts>,
    pub history: Arc<History>,
    pub logbook: Arc<Logbook>,
    pub tiles: Arc<TileCache>,
    pub geocoder: Arc<Geocoder>,
    pub weather: Arc<Weather>,
    pub airspace: Arc<Airspace>,
    pub coverage: Arc<Coverage>,
    pub charts: Arc<Charts>,
    pub datalink: Arc<Datalink>,
    pub airports: Arc<ArcSwap<Airports>>,
    /// Bundled navaid reference (VOR/DME/NDB…) for the map overlay.
    pub navaids: Arc<ArcSwap<Navaids>>,
    pub db: Arc<Db>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub viewport: Arc<Mutex<Option<Area>>>,
    pub status: Arc<Mutex<SourceStatus>>,
}
