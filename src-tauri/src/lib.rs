mod airspace;
mod alerts;
mod app;
mod commands;
mod config;
mod db;
mod emergency_watch;
mod enrich;
mod geocode;
mod ingest;
mod notable;
mod poller;
mod region;
mod state;
mod tiles;
mod util;
mod weather;

use std::sync::Arc;

use arc_swap::ArcSwap;
use parking_lot::Mutex;
use tauri::{Manager, RunEvent};

use crate::alerts::Alerts;
use crate::app::AppState;
use crate::config::AppSettings;
use crate::db::Db;
use crate::enrich::{
    actypes::AcTypes, aircraft_db::AircraftDb, airlines::Airlines, airports::Airports,
    photos::PhotoLookup, routes::RouteLookup, Enricher,
};
use crate::airspace::Airspace;
use crate::emergency_watch::EmergencyWatch;
use crate::geocode::Geocoder;
use crate::ingest::{AircraftSource, HttpV2Source};
use crate::poller::{Poller, SourceStatus};
use crate::state::LiveState;
use crate::tiles::TileCache;
use crate::weather::Weather;

const USER_AGENT: &str = concat!(
    "RaccTrack-ADSB/",
    env!("CARGO_PKG_VERSION"),
    " (personal, non-commercial use)"
);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,racctrack_lib=debug".into()),
        )
        .init();

    let http = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .gzip(true)
        .build()
        .expect("build reqwest client");

    tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_notification::init())
        .register_asynchronous_uri_scheme_protocol("ofmtiles", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().trim_start_matches('/').to_string();
            tracing::debug!("ofmtiles scheme request: {}", request.uri());
            tauri::async_runtime::spawn(async move {
                let Some(state) = app.try_state::<AppState>() else {
                    responder.respond(
                        tauri::http::Response::builder()
                            .status(503)
                            .body(b"starting".to_vec())
                            .unwrap(),
                    );
                    return;
                };
                let resp = match state.tiles.serve(&path).await {
                    Ok(tb) => tauri::http::Response::builder()
                        .status(200)
                        .header("content-type", tb.content_type)
                        .header("access-control-allow-origin", "*")
                        .header("cache-control", "public, max-age=86400")
                        .body(tb.data)
                        .unwrap(),
                    Err(e) => tauri::http::Response::builder()
                        .status(502)
                        .header("access-control-allow-origin", "*")
                        .body(e.to_string().into_bytes())
                        .unwrap(),
                };
                responder.respond(resp);
            });
        })
        .setup(move |app| {
            let handle = app.handle().clone();

            // --- persistence ---
            let data_dir = handle
                .path()
                .app_data_dir()
                .expect("resolve app data dir");
            std::fs::create_dir_all(&data_dir).ok();
            let db = Arc::new(Db::open(&data_dir.join("racctrack.sqlite"))?);

            let settings = Arc::new(Mutex::new(AppSettings::load(&db)));

            // --- enrichment (bundled DBs load in the background) ---
            let aircraft_db = Arc::new(ArcSwap::from_pointee(AircraftDb::empty()));
            let airports = Arc::new(ArcSwap::from_pointee(Airports::empty()));
            let airlines = Arc::new(ArcSwap::from_pointee(Airlines::empty()));
            let actypes = Arc::new(ArcSwap::from_pointee(AcTypes::empty()));
            spawn_reference_loaders(
                handle.clone(),
                aircraft_db.clone(),
                airports.clone(),
                airlines.clone(),
                actypes.clone(),
            );

            let enricher = Arc::new(Enricher::new(
                aircraft_db.clone(),
                airports.clone(),
                airlines.clone(),
                actypes.clone(),
                RouteLookup::new(db.clone(), http.clone()),
                PhotoLookup::new(db.clone(), http.clone()),
            ));

            let alerts = Arc::new(Alerts::new(db.clone()));
            let geocoder = Arc::new(Geocoder::new(db.clone(), http.clone()));
            let weather = Arc::new(Weather::new(db.clone(), http.clone()));
            let airspace = Arc::new(Airspace::new(db.clone(), http.clone()));
            let tiles = Arc::new(TileCache::new(
                db.clone(),
                http.clone(),
                settings.lock().tile_cache_max_mb,
            ));

            let live = Arc::new(LiveState::new());
            let viewport = Arc::new(Mutex::new(None));
            let status = Arc::new(Mutex::new(SourceStatus::default()));

            let sources: Vec<Arc<dyn AircraftSource>> = vec![
                Arc::new(HttpV2Source::adsb_lol(http.clone())),
                Arc::new(HttpV2Source::adsb_fi(http.clone())),
            ];

            app.manage(AppState {
                live: live.clone(),
                enricher: enricher.clone(),
                alerts: alerts.clone(),
                tiles: tiles.clone(),
                geocoder: geocoder.clone(),
                weather: weather.clone(),
                airspace: airspace.clone(),
                airports: airports.clone(),
                db: db.clone(),
                settings: settings.clone(),
                viewport: viewport.clone(),
                status: status.clone(),
            });

            let ewatch = Arc::new(EmergencyWatch::new(http.clone(), settings.clone()));
            tauri::async_runtime::spawn(ewatch.run(handle.clone()));

            let poller = Arc::new(Poller::new(
                live, enricher, alerts, sources, settings, viewport, status,
            ));
            tauri::async_runtime::spawn(poller.run(handle));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::set_viewport,
            commands::get_snapshot,
            commands::get_aircraft_detail,
            commands::get_trail,
            commands::get_all_trails,
            commands::get_source_status,
            commands::log_frontend,
            commands::geocode,
            commands::list_watch,
            commands::add_watch,
            commands::remove_watch,
            commands::set_watch_enabled,
            commands::get_settings,
            commands::update_settings,
            commands::tile_cache_stats,
            commands::clear_tile_cache,
            commands::download_tile_area,
            commands::airports_in,
            commands::airport_info,
            commands::find_airport,
            commands::metars_in,
            commands::station_wx,
            commands::airspace_in,
            commands::list_presets,
        ])
        .build(tauri::generate_context!())
        .expect("build tauri app")
        .run(|_app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                tracing::info!("shutting down");
            }
        });
}

fn spawn_reference_loaders(
    handle: tauri::AppHandle,
    aircraft_db: Arc<ArcSwap<AircraftDb>>,
    airports: Arc<ArcSwap<Airports>>,
    airlines: Arc<ArcSwap<Airlines>>,
    actypes: Arc<ArcSwap<AcTypes>>,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let resolve = |rel: &str| -> Option<std::path::PathBuf> {
            handle
                .path()
                .resolve(rel, tauri::path::BaseDirectory::Resource)
                .ok()
        };

        if let Some(path) = resolve("assets/aircraft.csv.gz") {
            match std::fs::read(&path).map_err(anyhow::Error::from).and_then(|b| {
                AircraftDb::from_gz_bytes(&b)
            }) {
                Ok(db) => {
                    tracing::info!("aircraft db loaded: {} entries", db.len());
                    aircraft_db.store(Arc::new(db));
                }
                Err(e) => tracing::warn!("aircraft db load failed ({}): {e}", path.display()),
            }
        } else {
            tracing::warn!("aircraft.csv.gz resource not found");
        }

        let read = |rel: &str| resolve(rel).and_then(|p| std::fs::read(&p).ok());
        match (
            read("assets/airports.csv"),
            read("assets/runways.csv"),
            read("assets/airport-frequencies.csv"),
        ) {
            (Some(ap), rw, fq) => {
                match Airports::load(
                    &ap,
                    &rw.unwrap_or_default(),
                    &fq.unwrap_or_default(),
                ) {
                    Ok(a) => {
                        tracing::info!("airports loaded: {} airports", a.len());
                        airports.store(Arc::new(a));
                    }
                    Err(e) => tracing::warn!("airports load failed: {e}"),
                }
            }
            _ => tracing::warn!("airports.csv resource not found"),
        }

        match resolve("assets/airlines.dat").and_then(|p| std::fs::read_to_string(&p).ok()) {
            Some(text) => match Airlines::from_dat(&text) {
                Ok(a) => {
                    tracing::info!("airlines loaded: {} entries", a.len());
                    airlines.store(Arc::new(a));
                }
                Err(e) => tracing::warn!("airlines load failed: {e}"),
            },
            None => tracing::warn!("airlines.dat resource not found"),
        }

        match resolve("assets/actypes.json").and_then(|p| std::fs::read(&p).ok()) {
            Some(bytes) => match AcTypes::from_json(&bytes) {
                Ok(t) => {
                    tracing::info!("aircraft types loaded: {} entries", t.len());
                    actypes.store(Arc::new(t));
                }
                Err(e) => tracing::warn!("actypes load failed: {e}"),
            },
            None => tracing::warn!("actypes.json resource not found"),
        }
    });
}
