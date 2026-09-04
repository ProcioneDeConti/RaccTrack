//! Tauri command surface exposed to the frontend.

use std::collections::HashMap;

use tauri::{AppHandle, Emitter, State};

use crate::alerts::{WatchEntry, WatchKind};
use crate::app::AppState;
use crate::config::AppSettings;
use crate::enrich::AircraftDetail;
use crate::geocode::GeoResult;
use crate::region::Area;
use crate::state::{AircraftDiff, TrailPoint};
use crate::tiles::{DownloadProgress, TileCacheStats};
use crate::poller::SourceStatus;
use crate::util::now_ms;

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
pub fn set_viewport(state: State<AppState>, bbox: Area, zoom: f64) {
    let _ = zoom;
    *state.viewport.lock() = Some(bbox);
}

#[tauri::command]
pub fn get_snapshot(state: State<AppState>) -> AircraftDiff {
    let list = state.live.snapshot();
    AircraftDiff {
        total: state.live.total(),
        generated_at: now_ms(),
        added: list,
        ..Default::default()
    }
}

#[tauri::command]
pub async fn get_aircraft_detail(
    state: State<'_, AppState>,
    hex: String,
) -> CmdResult<AircraftDetail> {
    let ac = state
        .live
        .get(&hex)
        .ok_or_else(|| format!("aircraft {hex} not in view"))?;
    let contact = state.settings.lock().contact.clone();
    Ok(state.enricher.detail(ac, &contact).await)
}

#[tauri::command]
pub fn get_trail(state: State<AppState>, hex: String) -> Vec<TrailPoint> {
    state.live.trail(&hex)
}

#[tauri::command]
pub fn get_all_trails(state: State<AppState>) -> HashMap<String, Vec<TrailPoint>> {
    state.live.all_trails()
}

#[tauri::command]
pub fn get_source_status(state: State<AppState>) -> SourceStatus {
    state.status.lock().clone()
}

/// Frontend -> backend log bridge, so webview console errors land in the same
/// log stream during development.
#[tauri::command]
pub fn log_frontend(level: String, message: String) {
    let message = message.chars().take(2000).collect::<String>();
    match level.as_str() {
        "error" => tracing::error!(target: "frontend", "{message}"),
        "warn" => tracing::warn!(target: "frontend", "{message}"),
        _ => tracing::info!(target: "frontend", "{message}"),
    }
}

#[tauri::command]
pub async fn geocode(state: State<'_, AppState>, query: String) -> CmdResult<Vec<GeoResult>> {
    state.geocoder.search(&query).await.map_err(err)
}

// --- airports / weather / airspace overlays ---

#[tauri::command]
pub fn airports_in(
    state: State<AppState>,
    bbox: Area,
    limit: Option<usize>,
) -> Vec<crate::enrich::airports::Airport> {
    let b = bbox.clamped();
    state
        .airports
        .load()
        .list_in(b.west, b.south, b.east, b.north, limit.unwrap_or(600))
}

#[tauri::command]
pub fn airport_info(
    state: State<AppState>,
    code: String,
) -> Option<crate::enrich::airports::AirportInfo> {
    state.airports.load().info(&code)
}

#[tauri::command]
pub fn find_airport(
    state: State<AppState>,
    query: String,
) -> Vec<crate::enrich::airports::Airport> {
    state.airports.load().find(&query)
}

#[tauri::command]
pub async fn metars_in(
    state: State<'_, AppState>,
    bbox: Area,
) -> CmdResult<Vec<crate::weather::Metar>> {
    state.weather.metars_in(bbox).await.map_err(err)
}

#[tauri::command]
pub async fn station_wx(
    state: State<'_, AppState>,
    icao: String,
) -> CmdResult<crate::weather::StationWx> {
    state.weather.station(&icao).await.map_err(err)
}

#[tauri::command]
pub async fn airspace_in(
    state: State<'_, AppState>,
    bbox: Area,
) -> CmdResult<serde_json::Value> {
    state.airspace.in_area(bbox).await.map_err(err)
}

// --- notable presets ---

#[tauri::command]
pub fn list_presets() -> Vec<crate::notable::Preset> {
    crate::notable::presets()
}

// --- watchlist ---

#[tauri::command]
pub fn list_watch(state: State<AppState>) -> CmdResult<Vec<WatchEntry>> {
    state.alerts.list().map_err(err)
}

#[tauri::command]
pub fn add_watch(
    state: State<AppState>,
    kind: WatchKind,
    value: String,
    label: Option<String>,
) -> CmdResult<WatchEntry> {
    state.alerts.add(kind, &value, label.as_deref()).map_err(err)
}

#[tauri::command]
pub fn remove_watch(state: State<AppState>, id: i64) -> CmdResult<()> {
    state.alerts.remove(id).map_err(err)
}

#[tauri::command]
pub fn set_watch_enabled(state: State<AppState>, id: i64, enabled: bool) -> CmdResult<()> {
    state.alerts.set_enabled(id, enabled).map_err(err)
}

// --- settings ---

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> AppSettings {
    state.settings.lock().clone()
}

#[tauri::command]
pub fn update_settings(
    state: State<AppState>,
    patch: serde_json::Value,
) -> CmdResult<AppSettings> {
    let mut s = state.settings.lock();
    s.apply_patch(patch);
    s.save(&state.db).map_err(err)?;
    state.tiles.set_max_mb(s.tile_cache_max_mb);
    Ok(s.clone())
}

// --- tile cache ---

#[tauri::command]
pub fn tile_cache_stats(state: State<AppState>) -> CmdResult<TileCacheStats> {
    state.tiles.stats().map_err(err)
}

#[tauri::command]
pub fn clear_tile_cache(state: State<AppState>) -> CmdResult<()> {
    state.tiles.clear().map_err(err)
}

#[tauri::command]
pub fn download_tile_area(
    app: AppHandle,
    state: State<AppState>,
    bbox: Area,
    min_zoom: u8,
    max_zoom: u8,
) {
    let tiles = state.tiles.clone();
    let min_zoom = min_zoom.min(max_zoom);
    let max_zoom = max_zoom.clamp(min_zoom, 12);
    tauri::async_runtime::spawn(async move {
        let app2 = app.clone();
        let res = tiles
            .download_area(bbox, min_zoom, max_zoom, move |p: DownloadProgress| {
                let _ = app2.emit("tile-download-progress", &p);
            })
            .await;
        if let Err(e) = res {
            tracing::warn!("tile area download failed: {e}");
            let _ = app.emit(
                "tile-download-progress",
                &DownloadProgress {
                    done: 0,
                    total: 0,
                    finished: true,
                },
            );
        }
    });
}
