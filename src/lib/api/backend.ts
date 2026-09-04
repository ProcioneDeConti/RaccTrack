import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AircraftDiff,
  AircraftDetail,
  AircraftEvent,
  AlertEvent,
  AppSettings,
  GeoResult,
  LocalReceiverProbe,
  Sighting,
  SourceStatus,
  TrailPoint,
  WatchEntry,
  WatchKind,
} from "./types";
import type { Bbox } from "../map/region";

/** Tell the backend which region the map is currently showing. */
export function setViewport(bbox: Bbox, zoom: number): Promise<void> {
  return invoke("set_viewport", { bbox, zoom });
}

export function getSnapshot(): Promise<AircraftDiff> {
  return invoke("get_snapshot");
}

export function getAircraftDetail(hex: string): Promise<AircraftDetail> {
  return invoke("get_aircraft_detail", { hex });
}

export function getTrail(hex: string): Promise<TrailPoint[]> {
  return invoke("get_trail", { hex });
}

export function aircraftHistory(hex: string): Promise<AircraftEvent[]> {
  return invoke("aircraft_history", { hex });
}

export function recentEvents(limit?: number): Promise<AircraftEvent[]> {
  return invoke("recent_events", { limit });
}

export function clearHistory(): Promise<void> {
  return invoke("clear_history");
}

export function logbook(
  sort: "last" | "first" | "count" | "reg",
  search: string,
  limit?: number,
): Promise<Sighting[]> {
  return invoke("logbook", { sort, search, limit });
}

export function sighting(hex: string): Promise<Sighting | null> {
  return invoke("sighting", { hex });
}

export function setSightingNote(hex: string, note: string): Promise<void> {
  return invoke("set_sighting_note", { hex, note });
}

export function deleteSighting(hex: string): Promise<void> {
  return invoke("delete_sighting", { hex });
}

export function clearLogbook(): Promise<void> {
  return invoke("clear_logbook");
}

export function logbookCount(): Promise<number> {
  return invoke("logbook_count");
}

export function exportLogbook(): Promise<string> {
  return invoke("export_logbook");
}

export function testLocalReceiver(url: string): Promise<LocalReceiverProbe> {
  return invoke("test_local_receiver", { url });
}

export function getAllTrails(): Promise<Record<string, TrailPoint[]>> {
  return invoke("get_all_trails");
}

export function getSourceStatus(): Promise<SourceStatus> {
  return invoke("get_source_status");
}

export function geocode(query: string): Promise<GeoResult[]> {
  return invoke("geocode", { query });
}

// --- overlays: airports / weather / airspace ---

import type {
  Airport,
  AirportInfo,
  Metar,
  StationWx,
} from "./types";

export function airportsIn(bbox: Bbox, limit?: number): Promise<Airport[]> {
  return invoke("airports_in", { bbox, limit });
}

export function airportInfo(code: string): Promise<AirportInfo | null> {
  return invoke("airport_info", { code });
}

export function findAirport(query: string): Promise<Airport[]> {
  return invoke("find_airport", { query });
}

export function metarsIn(bbox: Bbox): Promise<Metar[]> {
  return invoke("metars_in", { bbox });
}

export function stationWx(icao: string): Promise<StationWx> {
  return invoke("station_wx", { icao });
}

// deno-lint-ignore no-explicit-any
export function airspaceIn(bbox: Bbox): Promise<any> {
  return invoke("airspace_in", { bbox });
}

// --- airport charts (FAA d-TPP) ---

import type { ChartSet } from "./types";

export function airportCharts(airport: string): Promise<ChartSet> {
  return invoke("airport_charts", { airport });
}

/** Chart PDF bytes (cached in SQLite backend-side). */
export function chartPdf(url: string): Promise<ArrayBuffer> {
  return invoke("chart_pdf", { url });
}

export function openExternal(url: string): Promise<void> {
  return invoke("open_external", { url });
}

// --- aircraft datalink (airframes.io) ---

import type { DlMessage } from "./types";

export function datalinkFor(hex: string): Promise<DlMessage[]> {
  return invoke("datalink_for", { hex });
}

// --- presets ---

import type { Preset } from "./types";

export function listPresets(): Promise<Preset[]> {
  return invoke("list_presets");
}

// --- watchlist ---

export function listWatch(): Promise<WatchEntry[]> {
  return invoke("list_watch");
}

export function addWatch(
  kind: WatchKind,
  value: string,
  label: string | null,
): Promise<WatchEntry> {
  return invoke("add_watch", { kind, value, label });
}

export function removeWatch(id: number): Promise<void> {
  return invoke("remove_watch", { id });
}

export function setWatchEnabled(id: number, enabled: boolean): Promise<void> {
  return invoke("set_watch_enabled", { id, enabled });
}

// --- settings ---

export function getSettings(): Promise<AppSettings> {
  return invoke("get_settings");
}

export function updateSettings(
  patch: Partial<AppSettings>,
): Promise<AppSettings> {
  return invoke("update_settings", { patch });
}

// --- tile cache ---

export interface TileCacheStats {
  tiles: number;
  bytes: number;
}

export function tileCacheStats(): Promise<TileCacheStats> {
  return invoke("tile_cache_stats");
}

export function clearTileCache(): Promise<void> {
  return invoke("clear_tile_cache");
}

export interface DownloadProgress {
  done: number;
  total: number;
  finished: boolean;
}

export function downloadTileArea(
  bbox: Bbox,
  minZoom: number,
  maxZoom: number,
): Promise<void> {
  return invoke("download_tile_area", { bbox, minZoom, maxZoom });
}

// --- events ---

export function onDiff(cb: (d: AircraftDiff) => void): Promise<UnlistenFn> {
  return listen<AircraftDiff>("aircraft-diff", (e) => cb(e.payload));
}

export function onAlert(cb: (a: AlertEvent) => void): Promise<UnlistenFn> {
  return listen<AlertEvent>("alert", (e) => cb(e.payload));
}

export function onAircraftEvent(
  cb: (e: AircraftEvent) => void,
): Promise<UnlistenFn> {
  return listen<AircraftEvent>("aircraft-event", (e) => cb(e.payload));
}

export function onSourceStatus(
  cb: (s: SourceStatus) => void,
): Promise<UnlistenFn> {
  return listen<SourceStatus>("source-status", (e) => cb(e.payload));
}

export function onEmergencyCount(cb: (n: number) => void): Promise<UnlistenFn> {
  return listen<number>("emergency-count", (e) => cb(e.payload));
}

export function onDownloadProgress(
  cb: (p: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgress>("tile-download-progress", (e) =>
    cb(e.payload),
  );
}
