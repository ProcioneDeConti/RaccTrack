//! Tauri command surface exposed to the frontend.

use std::collections::HashMap;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::alerts::{WatchEntry, WatchKind};
use crate::app::AppState;
use crate::config::AppSettings;
use crate::enrich::AircraftDetail;
use crate::geocode::GeoResult;
use crate::ingest::model::{Aircraft, AircraftResponse};
use crate::ingest::normalize;
use serde::Serialize;
use crate::region::Area;
use crate::state::{AircraftDiff, AircraftEvent, TrailPoint};
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
    let hex = hex.to_lowercase();
    let ac = match state.live.get(&hex) {
        Some(ac) => ac,
        // Not in the viewport feed (e.g. an NA-wide emergency-squawk hit) —
        // pull just this aircraft straight from a source by hex.
        None => fetch_aircraft_by_hex(state.inner(), &hex)
            .await
            .ok_or_else(|| format!("no live data for {hex} right now"))?,
    };
    let contact = state.settings.lock().contact.clone();
    let mut detail = state.enricher.detail(ac, &contact).await;
    if let Some(info) = state.live.airborne(&hex) {
        detail.airborne_since = Some(info.since_ms);
        detail.saw_departure = info.saw_departure;
    }
    Ok(detail)
}

/// On-demand single-aircraft fetch, trying each source until one knows the hex.
async fn fetch_aircraft_by_hex(state: &AppState, hex: &str) -> Option<Aircraft> {
    for source in &state.sources {
        match source.by_hex(hex).await {
            Ok(raw) => {
                if let Some(mut ac) = normalize(raw, source.name(), now_ms())
                    .into_iter()
                    .find(|a| a.hex == hex)
                {
                    state.enricher.fill_identity(&mut ac);
                    return Some(ac);
                }
            }
            Err(e) => tracing::debug!("by_hex {hex} via {}: {e}", source.name()),
        }
    }
    None
}

#[tauri::command]
pub fn get_trail(state: State<AppState>, hex: String) -> Vec<TrailPoint> {
    state.live.trail(&hex)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalReceiverProbe {
    pub aircraft: usize,
    pub with_position: usize,
}

/// One-off probe of a local dump1090/readsb `aircraft.json` URL, for the
/// Settings "test connection" button.
#[tauri::command]
pub async fn test_local_receiver(url: String) -> CmdResult<LocalReceiverProbe> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(err)?;
    let resp = client.get(url.trim()).send().await.map_err(err)?;
    if !resp.status().is_success() {
        return Err(format!("receiver returned HTTP {}", resp.status()));
    }
    let body: AircraftResponse = resp.json().await.map_err(err)?;
    let with_position = body
        .ac
        .iter()
        .filter(|a| a.lat.is_some() && a.lon.is_some())
        .count();
    Ok(LocalReceiverProbe {
        aircraft: body.ac.len(),
        with_position,
    })
}

/// Enumerate connected RTL-SDR dongles, for the Settings "detect device"
/// button — a plain probe, independent of whether direct RTL-SDR mode is on.
#[tauri::command]
pub fn list_rtlsdr_devices() -> CmdResult<Vec<String>> {
    crate::ingest::rtlsdr::list_devices().map_err(err)
}

/// Compute (or return the cached) estimated RTL-SDR reception polygon
/// around whichever saved place has `rtlsdrLocation` set. Errors if none is
/// set — the frontend should only call this once one is.
#[tauri::command]
pub async fn compute_coverage(
    state: State<'_, AppState>,
) -> CmdResult<crate::coverage::CoverageResult> {
    let (place, antenna_height_ft, target_alt_ft) = {
        let s = state.settings.lock();
        let place = s
            .places
            .iter()
            .find(|p| p.rtlsdr_location)
            .cloned()
            .ok_or_else(|| {
                "No place is marked as the RTL-SDR location — set one in Places & alerts.".to_string()
            })?;
        (place, s.coverage_antenna_height_ft, s.coverage_target_alt_ft)
    };
    state
        .coverage
        .compute(place.lat, place.lon, antenna_height_ft, target_alt_ft)
        .await
        .map_err(err)
}

/// Live progress of an in-flight `compute_coverage` — polled while the
/// Settings panel shows "Computing…" to drive a real progress bar/ETA, since
/// a compute at the current terrain resolution takes on the order of minutes
/// (paced deliberately, to stay under the elevation API's rate limit).
#[tauri::command]
pub fn coverage_progress(state: State<AppState>) -> crate::coverage::CoverageProgress {
    state.coverage.progress()
}

/// Live progress of the direct-RTL-SDR worker thread — device open? real
/// messages decoded yet? — so Settings can show more than "no error so far".
#[tauri::command]
pub fn rtlsdr_status(state: State<AppState>) -> crate::ingest::rtlsdr::RtlSdrStatus {
    state.rtlsdr.status()
}

/// Tune the RTL-SDR to a VHF airband frequency and start playing the
/// AM-demodulated audio — see `atc.rs` for the single-dongle handoff with
/// ADS-B decoding this does when they'd otherwise contend for one device.
#[tauri::command]
pub async fn atc_tune(state: State<'_, AppState>, mhz: f64, device_index: u32) -> CmdResult<()> {
    state.atc.tune(mhz, device_index).await.map_err(err)
}

#[tauri::command]
pub async fn atc_stop(state: State<'_, AppState>) -> CmdResult<()> {
    state.atc.stop().await;
    Ok(())
}

#[tauri::command]
pub fn atc_status(state: State<AppState>) -> crate::atc::AtcStatus {
    state.atc.status()
}

/// Scan across several frequencies at once, parking on whichever one
/// currently has a transmission — same single/dual-dongle handling as `atc_tune`.
#[tauri::command]
pub async fn atc_scan(
    state: State<'_, AppState>,
    mhz: Vec<f64>,
    device_index: u32,
) -> CmdResult<()> {
    state.atc.scan(mhz, device_index).await.map_err(err)
}

/// Start recording the current ATC session to a WAV file in the OS
/// Downloads folder; returns its path. Errors if nothing is playing yet.
#[tauri::command]
pub fn atc_start_recording(app: AppHandle, state: State<AppState>) -> CmdResult<String> {
    use tauri::Manager;
    let dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().document_dir())
        .map_err(err)?;
    let name = format!(
        "racctrack-atc-{}.wav",
        crate::logbook::chrono_iso(now_ms()).replace(':', "-")
    );
    let path = state.atc.start_recording(dir.join(name)).map_err(err)?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn atc_stop_recording(state: State<AppState>) {
    state.atc.stop_recording();
}

/// Listen for ACARS on a single VHF frequency — see `acars.rs` for the
/// same single-dongle handoff `atc_tune` does.
#[tauri::command]
pub async fn acars_start(state: State<'_, AppState>, mhz: f64, device_index: u32) -> CmdResult<()> {
    state.acars.start(vec![mhz], device_index).await.map_err(err)
}

/// Scan across several ACARS frequencies, parking on whichever currently has
/// a burst — same dwell/hang handling as `atc_scan`.
#[tauri::command]
pub async fn acars_scan(
    state: State<'_, AppState>,
    mhz: Vec<f64>,
    device_index: u32,
) -> CmdResult<()> {
    state.acars.start(mhz, device_index).await.map_err(err)
}

#[tauri::command]
pub async fn acars_stop(state: State<'_, AppState>) -> CmdResult<()> {
    state.acars.stop().await;
    Ok(())
}

#[tauri::command]
pub fn acars_status(state: State<AppState>) -> crate::acars::AcarsStatus {
    state.acars.status()
}

#[tauri::command]
pub fn acars_messages(state: State<AppState>) -> Vec<crate::acars::AcarsMessage> {
    state.acars.messages()
}

#[tauri::command]
pub fn acars_clear_messages(state: State<AppState>) {
    state.acars.clear_messages();
}

/// Live progress of the direct-UAT (978MHz) worker thread — same
/// "enabled, but is the device actually open/decoding" distinction as
/// `rtlsdr_status`.
#[tauri::command]
pub fn uat_status(state: State<AppState>) -> crate::ingest::uat::UatStatus {
    state.uat.status()
}

#[derive(serde::Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(serde::Deserialize)]
struct GithubRelease {
    assets: Vec<GithubAsset>,
}

/// Download the latest official Zadig release and launch it (elevated), for
/// Settings' "Fix USB driver" button — RTL-SDR dongles need WinUSB bound via
/// Zadig before `rs_rtl` can claim them. Windows-only; driver installation
/// on Windows can't go through a plain spawn, it needs ShellExecute's
/// "runas" verb (that's what `runas` does), which is why this can't just be
/// `open_external`ed. Zadig itself is a separate downloaded binary we launch
/// as its own process — not linked into this app — so its GPL-3.0 license
/// doesn't affect RaccTrack's.
#[tauri::command]
pub async fn fix_usb_driver(app: AppHandle) -> CmdResult<()> {
    if !cfg!(target_os = "windows") {
        return Err("USB driver installation is only needed on Windows.".into());
    }

    let client = reqwest::Client::builder()
        .user_agent(crate::USER_AGENT)
        .build()
        .map_err(err)?;

    let release: GithubRelease = client
        .get("https://api.github.com/repos/pbatard/libwdi/releases/latest")
        .send()
        .await
        .map_err(err)?
        .json()
        .await
        .map_err(err)?;

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.starts_with("zadig-") && a.name.ends_with(".exe"))
        .ok_or_else(|| "couldn't find a Zadig download in the latest release".to_string())?;

    let bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(err)?
        .bytes()
        .await
        .map_err(err)?;

    let dir = app.path().app_data_dir().map_err(err)?;
    std::fs::create_dir_all(&dir).map_err(err)?;
    let exe_path = dir.join("zadig.exe");
    std::fs::write(&exe_path, &bytes).map_err(err)?;

    tokio::task::spawn_blocking(move || runas::Command::new(&exe_path).status())
        .await
        .map_err(err)?
        .map_err(err)?;

    Ok(())
}

// --- flight-event history ---

#[tauri::command]
pub fn aircraft_history(state: State<AppState>, hex: String) -> CmdResult<Vec<AircraftEvent>> {
    state
        .history
        .for_hex(&hex.to_lowercase(), 200)
        .map_err(err)
}

#[tauri::command]
pub fn recent_events(
    state: State<AppState>,
    limit: Option<i64>,
) -> CmdResult<Vec<AircraftEvent>> {
    state
        .history
        .recent(limit.unwrap_or(300).clamp(1, 2000))
        .map_err(err)
}

#[tauri::command]
pub fn clear_history(state: State<AppState>) -> CmdResult<()> {
    state.history.clear().map_err(err)
}

// --- spotter logbook ---

#[tauri::command]
pub fn logbook(
    state: State<AppState>,
    sort: String,
    search: String,
    limit: Option<i64>,
) -> CmdResult<Vec<crate::logbook::Sighting>> {
    state
        .logbook
        .list(&sort, &search, limit.unwrap_or(1000).clamp(1, 20_000))
        .map_err(err)
}

#[tauri::command]
pub fn sighting(
    state: State<AppState>,
    hex: String,
) -> CmdResult<Option<crate::logbook::Sighting>> {
    state.logbook.get(&hex.to_lowercase()).map_err(err)
}

#[tauri::command]
pub fn set_sighting_note(state: State<AppState>, hex: String, note: String) -> CmdResult<()> {
    state.logbook.set_note(&hex.to_lowercase(), &note).map_err(err)
}

#[tauri::command]
pub fn delete_sighting(state: State<AppState>, hex: String) -> CmdResult<()> {
    state.logbook.delete(&hex.to_lowercase()).map_err(err)
}

#[tauri::command]
pub fn clear_logbook(state: State<AppState>) -> CmdResult<()> {
    state.logbook.clear().map_err(err)
}

#[tauri::command]
pub fn logbook_count(state: State<AppState>) -> CmdResult<i64> {
    state.logbook.count().map_err(err)
}

/// Write the whole logbook to a CSV in the OS Downloads folder; returns its path.
#[tauri::command]
pub fn export_logbook(app: AppHandle, state: State<AppState>) -> CmdResult<String> {
    use tauri::Manager;
    let csv = state.logbook.export_csv().map_err(err)?;
    let dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().document_dir())
        .map_err(err)?;
    let name = format!(
        "racctrack-logbook-{}.csv",
        &crate::logbook::chrono_iso(now_ms())[..10]
    );
    let path = dir.join(name);
    std::fs::write(&path, csv).map_err(err)?;
    Ok(path.to_string_lossy().into_owned())
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

// --- airport charts (FAA d-TPP) ---

#[tauri::command]
pub async fn airport_charts(
    state: State<'_, AppState>,
    airport: String,
) -> CmdResult<crate::charts::ChartSet> {
    state.charts.charts_for(&airport).await.map_err(err)
}

#[tauri::command]
pub async fn chart_pdf(
    state: State<'_, AppState>,
    url: String,
) -> CmdResult<tauri::ipc::Response> {
    let bytes = state.charts.pdf(&url).await.map_err(err)?;
    Ok(tauri::ipc::Response::new(bytes))
}

// --- aircraft datalink (airframes.io) ---

#[tauri::command]
pub async fn datalink_for(
    state: State<'_, AppState>,
    hex: String,
) -> CmdResult<Vec<crate::datalink::DlMessage>> {
    state.datalink.for_hex(&hex).await.map_err(err)
}

#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> CmdResult<()> {
    if !url.starts_with("https://") {
        return Err("only https URLs may be opened".into());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(url, None::<&str>).map_err(err)
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
