import { writable, derived, get } from "svelte/store";
import type { Aircraft, AircraftDiff, SourceStatus } from "./api/types";
import { altColor, iconKindFor, sizeMulFor } from "./map/icons";
import { matchesFilters, type Filters, defaultFilters } from "./filters/filters";

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

import type { HomeLocation, MapLayers } from "./api/types";
export const home = writable<HomeLocation | null>(null);
/** bumped to ask MapView to recenter on the home location */
export const goHomeSignal = writable(0);

export const layers = writable<MapLayers>({
  airports: false,
  weather: false,
  airspace: false,
  rangeRings: false,
});
export const rangeRingsNm = writable<number[]>([25, 50, 100]);
/** airport ident whose info panel is open */
export const selectedAirport = writable<string | null>(null);
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
    for (const hex of diff.removed) m.delete(hex);
    return m;
  });
  total.set(diff.total);
  lastUpdate.set(diff.generatedAt);
}

export function resetAircraft(list: Aircraft[], totalCount: number): void {
  const m = new Map<string, Aircraft>();
  for (const a of list) m.set(a.hex, a);
  aircraft.set(m);
  total.set(totalCount);
  lastUpdate.set(Date.now());
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
    callsign: string;
    altBaro: number | null;
    selected: boolean;
    military: boolean;
    emergency: boolean;
  };
}

/** GeoJSON feed for the aircraft symbol layer, filtered by the active filters. */
export const aircraftGeoJson = derived(
  [aircraft, filters, selectedHex],
  ([$aircraft, $filters, $selected]) => {
    const features: AircraftFeature[] = [];
    for (const a of $aircraft.values()) {
      if (a.lat === null || a.lon === null) continue;
      if (!matchesFilters(a, $filters)) continue;
      const emergency = !!a.emergency && a.emergency !== "none";
      const heading = a.track ?? a.trueHeading ?? a.magHeading;
      const kind = iconKindFor(
        a.category,
        a.typeCode,
        a.onGround,
        heading !== null && heading !== undefined,
      );
      features.push({
        type: "Feature",
        id: a.hex,
        geometry: { type: "Point", coordinates: [a.lon, a.lat] },
        properties: {
          hex: a.hex,
          icon: `ac-${kind}`,
          sizeMul: sizeMulFor(kind),
          rotation: heading ?? 0,
          color: emergency ? "#ff3b30" : altColor(a.altBaro, a.onGround),
          callsign: (a.flight ?? a.registration ?? a.hex).trim(),
          altBaro: a.altBaro,
          selected: a.hex === $selected,
          military: a.military,
          emergency,
        },
      });
    }
    return {
      type: "FeatureCollection" as const,
      features,
    };
  },
);

export function selectedAircraft(): Aircraft | null {
  const hex = get(selectedHex);
  if (!hex) return null;
  return get(aircraft).get(hex) ?? null;
}

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
