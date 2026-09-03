import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AircraftDiff,
  AircraftDetail,
  AlertEvent,
  AppSettings,
  GeoResult,
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

export function getAllTrails(): Promise<Record<string, TrailPoint[]>> {
  return invoke("get_all_trails");
}

export function getSourceStatus(): Promise<SourceStatus> {
  return invoke("get_source_status");
}

export function geocode(query: string): Promise<GeoResult[]> {
  return invoke("geocode", { query });
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

export function onSourceStatus(
  cb: (s: SourceStatus) => void,
): Promise<UnlistenFn> {
  return listen<SourceStatus>("source-status", (e) => cb(e.payload));
}

export function onDownloadProgress(
  cb: (p: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgress>("tile-download-progress", (e) =>
    cb(e.payload),
  );
}
