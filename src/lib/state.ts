import { writable, derived, get } from "svelte/store";
import type { Aircraft, AircraftDiff, CoverageResult, SourceStatus } from "./api/types";
import { iconKindFor, sizeMulFor } from "./map/icons";
import { altColor, altColorOnLight, EMERGENCY } from "./theme/colors";
import { matchesFilters, type Filters, defaultFilters } from "./filters/filters";
import { altitude as fmtAltitude, units } from "./format";

/** Live aircraft keyed by hex. */
export const aircraft = writable<Map<string, Aircraft>>(new Map());
export const total = writable(0);
export const lastUpdate = writable(0);
export const selectedHex = writable<string | null>(null);
export const hoveredHex = writable<string | null>(null);
export const filters = writable<Filters>(defaultFilters());
export const sourceStatus = writable<SourceStatus | null>(null);
/** count of aircraft squawking an emergency code anywhere in North America */
export const emergencyCount = writable(0);
export const basemap = writable<string>("darkMatter");
/** App chrome theme — "auto" follows the basemap's own light/dark tiles. */
export const uiTheme = writable<"auto" | "light" | "dark">("auto");

import type { MapColors, MapLayers, Place } from "./api/types";
/** All saved locations. */
export const places = writable<Place[]>([]);
/** The primary place — drives the rail "go to" button + range rings. */
export const primaryPlace = derived(
  places,
  ($p) => $p.find((x) => x.primary) ?? $p[0] ?? null,
);
/** bumped to ask MapView to recenter on the primary place */
export const goHomeSignal = writable(0);

/** User color overrides for airspace categories + geofences. */
export const mapColors = writable<MapColors>({
  airspace: {},
  geofenceFill: null,
  geofenceLine: null,
});

/** Non-null while the user is hand-drawing a geofence polygon for this
 *  place on the map (see MapView's click-to-add-vertex handling). */
export const geofenceDraft = writable<{ placeId: string } | null>(null);

/** Shown on first launch (until acknowledged) and re-openable from About. */
export const disclaimerOpen = writable(false);

/** Latest computed RTL-SDR reception polygon, shared between Settings
 *  (triggers the computation) and MapView (renders it). */
export const coverageResult = writable<CoverageResult | null>(null);
/** Live mirror of `AppSettings.coverageEnabled` — whether to show it. */
export const coverageEnabled = writable(false);

export const layers = writable<MapLayers>({
  airports: false,
  weather: false,
  radar: false,
  airspace: false,
  rangeRings: false,
});
export const rangeRingsNm = writable<number[]>([25, 50, 100]);
/** airport ident whose info panel is open */
export const selectedAirport = writable<string | null>(null);
/** airport whose chart viewer is open: { ident, label } or null */
export const chartTarget = writable<{ ident: string; label: string } | null>(
  null,
);
/** hex the map is locked to and follows */
export const followHex = writable<string | null>(null);
/** hexes pinned to the bottom bar */
export const pinned = writable<string[]>([]);
/** bumped to ask MapView to fly to a lat/lon */
export const flyTo = writable<{ lat: number; lon: number; zoom?: number } | null>(
  null,
);
/**
 * Great-circle route of the selected aircraft, split into the leg already
 * flown and the leg remaining, for the map's two-tone route line.
 * `[lon, lat][]` segments; null when there's nothing to draw.
 */
export const routeLine = writable<{
  flown: [number, number][];
  remain: [number, number][];
} | null>(null);

/** current map viewport bounds (updated by MapView) */
export const mapBounds = writable<{
  west: number;
  south: number;
  east: number;
  north: number;
} | null>(null);

import { updateSettings } from "./api/backend";

export function togglePin(hex: string): void {
  pinned.update((list) => {
    const next = list.includes(hex)
      ? list.filter((h) => h !== hex)
      : [...list, hex].slice(-12);
    void updateSettings({ pinned: next });
    return next;
  });
}

export function applyDiff(diff: AircraftDiff): void {
  aircraft.update((m) => {
    for (const a of diff.added) m.set(a.hex, a);
    for (const a of diff.updated) m.set(a.hex, a);
    for (const hex of diff.removed) {
      m.delete(hex);
      iconMemo.delete(hex);
    }
    return m;
  });
  total.set(diff.total);
  lastUpdate.set(diff.generatedAt);
}

export function resetAircraft(list: Aircraft[], totalCount: number): void {
  const m = new Map<string, Aircraft>();
  for (const a of list) m.set(a.hex, a);
  aircraft.set(m);
  iconMemo.clear();
  total.set(totalCount);
  lastUpdate.set(Date.now());
}

// iconKindFor does Set lookups + regexes; its inputs (category/type are fixed
// per airframe, onGround/hasHeading flip rarely) change far less often than the
// 3 s position diff, so memoise the result per hex keyed by those inputs.
const iconMemo = new Map<
  string,
  { sig: string; icon: string; sizeMul: number }
>();

function iconFor(
  a: Aircraft,
  hasHeading: boolean,
): { icon: string; sizeMul: number } {
  const sig = `${a.category ?? ""}|${a.typeCode ?? ""}|${a.onGround ? 1 : 0}|${
    hasHeading ? 1 : 0
  }`;
  const hit = iconMemo.get(a.hex);
  if (hit && hit.sig === sig) return hit;
  const kind = iconKindFor(a.category, a.typeCode, a.onGround, hasHeading);
  const entry = { sig, icon: `ac-${kind}`, sizeMul: sizeMulFor(kind) };
  iconMemo.set(a.hex, entry);
  return entry;
}

export interface AircraftFeature {
  type: "Feature";
  id: string;
  geometry: { type: "Point"; coordinates: [number, number] };
  properties: {
    hex: string;
    icon: string;
    sizeMul: number;
    rotation: number;
    color: string;
    /** Same altitude-band color as `color`, darkened for legible text on the
     *  map label chip's light-background variant. */
    altTextColorOnLight: string;
    callsign: string;
    altBaro: number | null;
    /** Map-chip altitude text — "0ft/m - GROUND" while grounded, "" only
     *  when airborne with no altitude data at all. */
    altLabel: string;
    military: boolean;
    emergency: boolean;
    /** Received straight off the user's own RTL-SDR dongle, not a community
     *  feed / local-receiver relay. */
    direct: boolean;
  };
}

/**
 * GeoJSON feed for the aircraft symbol layer, filtered by the active filters.
 * Selection is *not* an input here — it's applied on the map via
 * `setFeatureState` so clicking an aircraft doesn't rebuild every feature.
 */
export const aircraftGeoJson = derived(
  [aircraft, filters, units],
  ([$aircraft, $filters]) => {
    const features: AircraftFeature[] = [];
    for (const a of $aircraft.values()) {
      if (a.lat === null || a.lon === null) continue;
      if (!matchesFilters(a, $filters)) continue;
      const emergency = !!a.emergency && a.emergency !== "none";
      const heading = a.track ?? a.trueHeading ?? a.magHeading;
      const { icon, sizeMul } = iconFor(
        a,
        heading !== null && heading !== undefined,
      );
      features.push({
        type: "Feature",
        id: a.hex,
        geometry: { type: "Point", coordinates: [a.lon, a.lat] },
        properties: {
          hex: a.hex,
          icon,
          sizeMul,
          rotation: heading ?? 0,
          color: emergency ? EMERGENCY : altColor(a.altBaro, a.onGround),
          altTextColorOnLight: emergency ? EMERGENCY : altColorOnLight(a.altBaro, a.onGround),
          callsign: (a.flight ?? a.registration ?? a.hex).trim(),
          altBaro: a.altBaro,
          altLabel: a.onGround
            ? "GROUND"
            : a.altBaro === null
              ? ""
              : fmtAltitude(a.altBaro),
          military: a.military,
          emergency,
          direct: a.source === "rtl-sdr",
        },
      });
    }
    return {
      type: "FeatureCollection" as const,
      features,
    };
  },
);

/** Count of aircraft actually drawn on the map (positioned + filter-matched).
 *  Derived from the geojson feed so the status bar doesn't re-scan every diff. */
export const shownCount = derived(aircraftGeoJson, ($g) => $g.features.length);

export function selectedAircraft(): Aircraft | null {
  const hex = get(selectedHex);
  if (!hex) return null;
  return get(aircraft).get(hex) ?? null;
}

import {
  predictPass,
  passClock,
  passHorizonMin,
  passRadiusNm,
  type PredictedPass,
} from "./passes";

/** Upcoming passes over the primary place, soonest first. */
export const upcomingPasses = derived(
  [aircraft, primaryPlace, passClock, passHorizonMin, passRadiusNm],
  ([$aircraft, $place, $now, $horizon, $radius]) => {
    if (!$place) return [] as PredictedPass[];
    const out: PredictedPass[] = [];
    for (const ac of $aircraft.values()) {
      const p = predictPass(ac, $place, $now, $horizon, $radius);
      if (p) out.push(p);
    }
    out.sort((a, b) => a.etaMs - b.etaMs);
    return out;
  },
);

/** Aircraft with a position inside the current viewport, matching active filters. */
export const visibleAircraft = derived(
  [aircraft, mapBounds, filters],
  ([$aircraft, $bounds, $filters]) => {
    const out: Aircraft[] = [];
    for (const a of $aircraft.values()) {
      if (a.lat === null || a.lon === null) continue;
      if (!matchesFilters(a, $filters)) continue;
      if (
        $bounds &&
        (a.lon < $bounds.west ||
          a.lon > $bounds.east ||
          a.lat < $bounds.south ||
          a.lat > $bounds.north)
      )
        continue;
      out.push(a);
    }
    return out;
  },
);
