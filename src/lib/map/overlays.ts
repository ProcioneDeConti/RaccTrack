// Map reference overlays: airports, airspace, weather (recolours airport dots),
// and range rings. One module owns all of their sources/layers and the
// viewport-driven data refresh so MapView just calls install / refresh / visibility.

import maplibregl from "maplibre-gl/dist/maplibre-gl-csp";
import type {
  Map as MlMap,
  GeoJSONSource,
  MapGeoJSONFeature,
} from "maplibre-gl";
import type { Bbox } from "./region";
import type { Airport, MapColors, MapLayers, Metar } from "../api/types";
import { airportsIn, metarsIn, airspaceIn } from "../api/backend";
import { selectedAirport, selectedHex } from "../state";
import {
  AIRSPACE_STYLE,
  AIRSPACE_FALLBACK,
  FLIGHT_CATEGORY_COLORS,
  FLIGHT_CATEGORY_FALLBACK,
  GEOFENCE_LINE_DEFAULT,
  GEOFENCE_FILL_DEFAULT,
  hexToRgba01,
} from "../theme/colors";
import { createFillLayer, type FillPolygon } from "./glFill";

const EMPTY = { type: "FeatureCollection", features: [] } as const;
const COVERAGE_LINE_COLOR = "#22d3ee";
const COVERAGE_FILL_RGBA = hexToRgba01(COVERAGE_LINE_COLOR, 0.14);
// 1x1 transparent PNG — the radar source needs *some* tile URL before the first
// frame is fetched.
const PLACEHOLDER_TILE =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+P+/HgAFhAJ/wlseKgAAAABJRU5ErkJggg==";

function airspacePaintColor(overrides: Record<string, string> = {}): any {
  const m: any[] = ["match", ["get", "category"]];
  for (const [k, v] of Object.entries(AIRSPACE_STYLE)) m.push(k, overrides[k] ?? v.color);
  m.push(AIRSPACE_FALLBACK);
  return m;
}

export interface PlaceGeofence {
  lat: number;
  lon: number;
  alert: {
    enabled: boolean;
    radiusNm: number;
    /** [lat, lon] vertices — an open ring, >= 3 points to take effect. */
    shape: [number, number][] | null;
  };
}

/** Closed [lon, lat] ring for a place's alert geometry — its drawn polygon
 *  when set, else a circle at its radius. Shared by the outline (line) and
 *  the fill (custom WebGL layer) so the two always agree exactly. */
function geofenceRing(p: PlaceGeofence): [number, number][] {
  if (p.alert.shape && p.alert.shape.length >= 3) {
    const pts = p.alert.shape.map(([lat, lon]) => [lon, lat] as [number, number]);
    pts.push(pts[0]);
    return pts;
  }
  return circle(p.lat, p.lon, p.alert.radiusNm);
}

function fltCatCircleColor(): any {
  const m: any[] = ["match", ["get", "fltCat"]];
  for (const [k, v] of Object.entries(FLIGHT_CATEGORY_COLORS)) m.push(k, v);
  m.push(FLIGHT_CATEGORY_FALLBACK);
  return m;
}

export class Overlays {
  private map: MlMap;
  private fltCat = new Map<string, string>();
  private lastAirports: Airport[] = [];
  private handlersBound = false;
  /** Last-applied `visibility` per layer id, so we don't re-set an unchanged
   *  value (each `setLayoutProperty` re-fires `styledata`). Cleared on install
   *  because a fresh style resets every layer to visible. */
  private visState: Record<string, boolean> = {};

  /** Per-kind record of the last successful fetch, so panning back over an
   *  area we already have doesn't refetch. Cleared on a real style swap. */
  private fetched: Record<string, { bbox: Bbox; zb: number; at: number }> = {};

  /** Current radar tile URL template — survives style swaps so a re-install
   *  re-creates the source with the right frame. */
  private radarUrl: string | null = null;

  /** Last place-alert ring FeatureCollection — survives style swaps. */
  private placeAlertData: unknown = null;
  /** Last draft (in-progress hand-drawn geofence) FeatureCollection. */
  private draftData: unknown = null;
  /** User color overrides — airspace category overrides + geofence colors. */
  private colorOverrides: MapColors = { airspace: {}, geofenceFill: null, geofenceLine: null };
  private lastPlaces: PlaceGeofence[] = [];
  /** Fills geofence interiors directly in WebGL — see glFill.ts for why this
   *  exists instead of a normal `fill` layer. */
  private fillLayer = createFillLayer("ov-place-alert-fill-gl");
  /** Separate instance for the RTL-SDR coverage polygon — same technique,
   *  independent lifecycle/color from place-alert geofences. */
  private coverageFillLayer = createFillLayer("ov-coverage-fill-gl");
  private coverageData: unknown = null;

  /** Draw a dashed ring — and a translucent fill — at each alert-enabled
   *  place's geometry (its drawn shape, or a circle at its radius). */
  setPlaceRings(places: PlaceGeofence[]) {
    this.lastPlaces = places;
    const active = places.filter(
      (p) => p.alert.enabled && Number.isFinite(p.lat) && Number.isFinite(p.lon),
    );
    const features = active.map((p) => ({
      type: "Feature" as const,
      geometry: { type: "LineString" as const, coordinates: geofenceRing(p) },
      properties: {},
    }));
    this.placeAlertData = { type: "FeatureCollection", features };
    (this.map.getSource("ov-place-alert") as GeoJSONSource | undefined)?.setData(
      this.placeAlertData as any,
    );

    const fillColor = hexToRgba01(this.colorOverrides.geofenceFill ?? GEOFENCE_FILL_DEFAULT, 0.22);
    const polygons: FillPolygon[] = active.map((p) => ({
      ring: geofenceRing(p).slice(0, -1),
      color: fillColor,
    }));
    this.fillLayer.setPolygons(polygons);
    this.map.triggerRepaint();
  }

  /** Estimated RTL-SDR reception polygon — an irregular terrain-aware ring
   *  (not a plain circle), same outline+WebGL-fill technique as place-alert
   *  geofences but its own layer/color. `null` (or too few points) clears it. */
  setCoverage(
    result: {
      receiverLat: number;
      receiverLon: number;
      points: { bearingDeg: number; distanceNm: number }[];
    } | null,
  ) {
    const src = this.map.getSource("ov-coverage") as GeoJSONSource | undefined;
    if (!result || result.points.length < 3) {
      this.coverageData = EMPTY;
      src?.setData(EMPTY as any);
      this.coverageFillLayer.setPolygons([]);
      this.map.triggerRepaint();
      return;
    }

    const ring: [number, number][] = result.points.map((p) =>
      destination(result.receiverLat, result.receiverLon, p.distanceNm, p.bearingDeg),
    );
    const closedRing = [...ring, ring[0]];

    this.coverageData = {
      type: "FeatureCollection",
      features: [
        {
          type: "Feature",
          geometry: { type: "LineString", coordinates: closedRing },
          properties: {},
        },
      ],
    };
    src?.setData(this.coverageData as any);
    this.coverageFillLayer.setPolygons([{ ring, color: COVERAGE_FILL_RGBA }]);
    this.map.triggerRepaint();
  }

  /** Live preview while the user is hand-drawing a geofence — outline +
   *  vertex dots only (plain GeoJSON; a handful of points, no fill needed
   *  mid-draw, and no risk of the tile-seam issue at that scale). */
  setDraft(points: [number, number][]) {
    const coords = points.map(([lat, lon]) => [lon, lat]);
    const features: any[] = [];
    if (coords.length >= 2) {
      features.push({
        type: "Feature",
        geometry: { type: "LineString", coordinates: coords },
        properties: {},
      });
    }
    for (const c of coords) {
      features.push({ type: "Feature", geometry: { type: "Point", coordinates: c }, properties: {} });
    }
    this.draftData = { type: "FeatureCollection", features };
    (this.map.getSource("ov-geofence-draft") as GeoJSONSource | undefined)?.setData(
      this.draftData as any,
    );
  }

  /** Apply user color overrides (airspace categories + geofence) and
   *  re-render anything they affect. */
  setColors(colors: MapColors) {
    this.colorOverrides = colors;
    if (this.map.getLayer("ov-airspace-fill")) {
      this.map.setPaintProperty(
        "ov-airspace-fill",
        "fill-color",
        airspacePaintColor(colors.airspace),
      );
    }
    if (this.map.getLayer("ov-airspace-line")) {
      this.map.setPaintProperty(
        "ov-airspace-line",
        "line-color",
        airspacePaintColor(colors.airspace),
      );
    }
    if (this.map.getLayer("ov-place-alert-line")) {
      this.map.setPaintProperty(
        "ov-place-alert-line",
        "line-color",
        colors.geofenceLine ?? GEOFENCE_LINE_DEFAULT,
      );
    }
    this.setPlaceRings(this.lastPlaces);
  }

  /** Point the radar layer at a new frame (or null before the first fetch). */
  setRadarFrame(url: string | null) {
    this.radarUrl = url;
    const src = this.map.getSource("ov-radar") as any;
    if (src?.setTiles) src.setTiles([url ?? PLACEHOLDER_TILE]);
  }

  private zoomBucket(): number {
    const z = this.map.getZoom();
    return z < 6 ? 0 : z < 9 ? 1 : 2;
  }

  private haveFresh(kind: string, bbox: Bbox, ttlMs: number): boolean {
    const c = this.fetched[kind];
    return (
      !!c &&
      c.zb === this.zoomBucket() &&
      Date.now() - c.at < ttlMs &&
      bbox.west >= c.bbox.west &&
      bbox.south >= c.bbox.south &&
      bbox.east <= c.bbox.east &&
      bbox.north <= c.bbox.north
    );
  }

  private noteFetch(kind: string, bbox: Bbox) {
    this.fetched[kind] = { bbox, zb: this.zoomBucket(), at: Date.now() };
  }

  constructor(map: MlMap) {
    this.map = map;
  }

  /** Idempotent — safe to call on every style.load. */
  install(beforeId?: string) {
    const m = this.map;
    // A style swap wipes every layer + source data; only then do the caches go
    // stale (fresh layers default to visible; sources are re-added empty).
    if (!m.getLayer("ov-airport-dot")) {
      this.visState = {};
      this.fetched = {};
    }
    if (!m.getSource("ov-airspace")) {
      m.addSource("ov-airspace", { type: "geojson", data: EMPTY as any });
    }
    if (!m.getSource("ov-airports")) {
      m.addSource("ov-airports", { type: "geojson", data: EMPTY as any });
    }
    if (!m.getSource("ov-rings")) {
      m.addSource("ov-rings", { type: "geojson", data: EMPTY as any });
    }
    if (!m.getSource("ov-place-alert")) {
      m.addSource("ov-place-alert", {
        type: "geojson",
        data: (this.placeAlertData ?? EMPTY) as any,
      });
    }
    if (!m.getSource("ov-geofence-draft")) {
      m.addSource("ov-geofence-draft", {
        type: "geojson",
        data: (this.draftData ?? EMPTY) as any,
      });
    }
    if (!m.getSource("ov-coverage")) {
      m.addSource("ov-coverage", {
        type: "geojson",
        data: (this.coverageData ?? EMPTY) as any,
      });
    }
    if (!m.getSource("ov-radar")) {
      m.addSource("ov-radar", {
        type: "raster",
        tiles: [this.radarUrl ?? PLACEHOLDER_TILE],
        tileSize: 256,
        attribution:
          '<a href="https://www.rainviewer.com/" target="_blank">RainViewer</a>',
      });
    }

    const add = (layer: any) => {
      if (!m.getLayer(layer.id)) m.addLayer(layer, beforeId);
    };

    // Radar sits right on top of the basemap, under every other overlay.
    add({
      id: "ov-radar",
      type: "raster",
      source: "ov-radar",
      paint: { "raster-opacity": 0.55, "raster-fade-duration": 0 },
    });

    add({
      id: "ov-airspace-fill",
      type: "fill",
      source: "ov-airspace",
      paint: { "fill-color": airspacePaintColor(this.colorOverrides.airspace), "fill-opacity": 0.08 },
    });
    add({
      id: "ov-airspace-line",
      type: "line",
      source: "ov-airspace",
      paint: {
        "line-color": airspacePaintColor(this.colorOverrides.airspace),
        "line-width": 1.3,
        "line-opacity": 0.85,
      },
    });

    add({
      id: "ov-rings-casing",
      type: "line",
      source: "ov-rings",
      layout: { "line-cap": "round", "line-join": "round" },
      paint: {
        "line-color": "#0d1117",
        "line-width": 3.5,
        "line-opacity": 0.5,
      },
    });
    add({
      id: "ov-rings-line",
      type: "line",
      source: "ov-rings",
      layout: { "line-cap": "round", "line-join": "round" },
      paint: {
        "line-color": "#9ec5ff",
        "line-width": 1.6,
        "line-dasharray": [3, 3],
        "line-opacity": 0.95,
      },
    });

    // Fill first so the dashed outline stays crisp on top of it.
    if (!m.getLayer(this.fillLayer.id)) m.addLayer(this.fillLayer, beforeId);
    add({
      id: "ov-place-alert-line",
      type: "line",
      source: "ov-place-alert",
      layout: { "line-cap": "round", "line-join": "round" },
      paint: {
        "line-color": this.colorOverrides.geofenceLine ?? GEOFENCE_LINE_DEFAULT,
        "line-width": 1.6,
        "line-dasharray": [2, 2],
        "line-opacity": 0.9,
      },
    });
    add({
      id: "ov-geofence-draft-line",
      type: "line",
      source: "ov-geofence-draft",
      layout: { "line-cap": "round", "line-join": "round" },
      paint: { "line-color": "#4c9be8", "line-width": 1.8, "line-dasharray": [1, 1.5] },
    });
    add({
      id: "ov-geofence-draft-point",
      type: "circle",
      source: "ov-geofence-draft",
      filter: ["==", ["geometry-type"], "Point"],
      paint: {
        "circle-radius": 4,
        "circle-color": "#4c9be8",
        "circle-stroke-color": "#0d1117",
        "circle-stroke-width": 1.2,
      },
    });

    // Fill first so the dashed outline stays crisp on top of it (same
    // ordering reasoning as the place-alert geofence fill above).
    if (!m.getLayer(this.coverageFillLayer.id)) {
      m.addLayer(this.coverageFillLayer, beforeId);
    }
    add({
      id: "ov-coverage-line",
      type: "line",
      source: "ov-coverage",
      layout: { "line-cap": "round", "line-join": "round" },
      paint: {
        "line-color": COVERAGE_LINE_COLOR,
        "line-width": 1.6,
        "line-dasharray": [4, 2],
        "line-opacity": 0.85,
      },
    });

    add({
      id: "ov-rings-label",
      type: "symbol",
      source: "ov-rings",
      filter: ["==", ["geometry-type"], "Point"],
      layout: {
        "text-field": ["get", "label"],
        "text-size": 10,
        "text-font": ["Noto Sans Regular", "Open Sans Regular"],
      },
      paint: {
        "text-color": "#8ab4f8",
        "text-halo-color": "#0d1117",
        "text-halo-width": 1.2,
      },
    });

    add({
      id: "ov-airport-dot",
      type: "circle",
      source: "ov-airports",
      paint: {
        "circle-radius": [
          "match",
          ["get", "size"],
          "large",
          5,
          "medium",
          3.5,
          2,
        ],
        "circle-color": fltCatCircleColor(),
        "circle-stroke-color": "#0d1117",
        "circle-stroke-width": 1,
      },
    });
    add({
      id: "ov-airport-label",
      type: "symbol",
      source: "ov-airports",
      minzoom: 7,
      layout: {
        "text-field": ["get", "code"],
        "text-size": 10,
        "text-offset": [0, 0.9],
        "text-anchor": "top",
        "text-font": ["Noto Sans Regular", "Open Sans Regular"],
        "text-optional": true,
      },
      paint: {
        "text-color": "#c9d1d9",
        "text-halo-color": "#0d1117",
        "text-halo-width": 1.2,
      },
    });

    // Event listeners must bind exactly once — install() re-runs on every
    // styledata, and re-binding here fires the airport click N times per click.
    if (!this.handlersBound) {
      this.handlersBound = true;
      m.on("click", "ov-airport-dot", (e) => {
        const f = e.features?.[0];
        if (f) {
          selectedHex.set(null);
          selectedAirport.set(f.properties?.ident as string);
        }
      });
      m.on("mouseenter", "ov-airport-dot", () => {
        m.getCanvas().style.cursor = "pointer";
      });
      m.on("mouseleave", "ov-airport-dot", () => {
        m.getCanvas().style.cursor = "";
      });
      m.on("click", "ov-airspace-fill", (e) => {
        const f = e.features?.[0];
        if (f) this.airspacePopup(e.lngLat, f);
      });
    }
  }

  setVisibility(layers: MapLayers) {
    const set = (id: string, on: boolean) => {
      if (this.visState[id] === on) return;
      if (this.map.getLayer(id)) {
        this.map.setLayoutProperty(id, "visibility", on ? "visible" : "none");
        this.visState[id] = on;
      }
    };
    set("ov-radar", layers.radar);
    set("ov-airspace-fill", layers.airspace);
    set("ov-airspace-line", layers.airspace);
    set("ov-airport-dot", layers.airports);
    set("ov-airport-label", layers.airports);
    set("ov-rings-casing", layers.rangeRings);
    set("ov-rings-line", layers.rangeRings);
    set("ov-rings-label", layers.rangeRings);
    // Fences follow the airspace toggle? No — always visible when they exist.
  }

  async refresh(bbox: Bbox, layers: MapLayers) {
    const zoom = this.map.getZoom();

    if (layers.weather && zoom >= 5.5 && !this.haveFresh("weather", bbox, 120_000)) {
      try {
        const metars = await metarsIn(bbox);
        this.fltCat.clear();
        for (const m of metars) {
          if (m.flightCategory) this.fltCat.set(m.icao.toUpperCase(), m.flightCategory);
        }
        this.applyAirportData();
        this.noteFetch("weather", bbox);
      } catch {
        /* keep last */
      }
    }
    if (layers.airports && !this.haveFresh("airports", bbox, 300_000)) {
      try {
        this.lastAirports = await airportsIn(bbox, zoom < 6 ? 150 : 800);
        this.applyAirportData();
        this.noteFetch("airports", bbox);
      } catch {
        /* keep last */
      }
    }
    if (
      layers.airspace &&
      zoom >= 6.5 &&
      !this.haveFresh("airspace", bbox, 600_000)
    ) {
      try {
        const fc = await airspaceIn(bbox);
        (this.map.getSource("ov-airspace") as GeoJSONSource | undefined)?.setData(
          fc,
        );
        this.noteFetch("airspace", bbox);
      } catch {
        /* keep last */
      }
    } else if (layers.airspace && zoom < 6.5) {
      (this.map.getSource("ov-airspace") as GeoJSONSource | undefined)?.setData(
        EMPTY as any,
      );
    }
  }

  private applyAirportData() {
    const weatherOn = this.fltCat.size > 0;
    const features = this.lastAirports.map((a) => ({
      type: "Feature" as const,
      geometry: { type: "Point" as const, coordinates: [a.lon, a.lat] },
      properties: {
        ident: a.ident,
        code: a.icao ?? a.ident,
        size:
          a.kind === "large_airport"
            ? "large"
            : a.kind === "medium_airport"
              ? "medium"
              : "small",
        fltCat: weatherOn ? (this.fltCat.get((a.icao ?? a.ident).toUpperCase()) ?? "") : "",
      },
    }));
    (this.map.getSource("ov-airports") as GeoJSONSource | undefined)?.setData({
      type: "FeatureCollection",
      features,
    } as any);
  }

  setRangeRings(
    home: { lat: number; lon: number } | null,
    radiiNm: number[],
    show: boolean,
  ) {
    const src = this.map.getSource("ov-rings") as GeoJSONSource | undefined;
    if (!src) return;
    if (!home || !show || radiiNm.length === 0) {
      src.setData(EMPTY as any);
      return;
    }
    const features: any[] = [];
    for (const nm of radiiNm) {
      features.push({
        type: "Feature",
        geometry: { type: "LineString", coordinates: circle(home.lat, home.lon, nm) },
        properties: {},
      });
      const [lon, lat] = destination(home.lat, home.lon, nm, 0);
      features.push({
        type: "Feature",
        geometry: { type: "Point", coordinates: [lon, lat] },
        properties: { label: `${nm} nm` },
      });
    }
    src.setData({ type: "FeatureCollection", features } as any);
  }

  private airspacePopup(lngLat: any, f: MapGeoJSONFeature) {
    const p = f.properties ?? {};
    const cat = String(p.category ?? "");
    const color = AIRSPACE_STYLE[cat]?.color ?? AIRSPACE_FALLBACK;
    const pretty = cat.replace(/^CLASS_/, "Class ").replace(/_/g, " ");
    const alt =
      p.lower || p.upper ? `${p.lower ?? "SFC"} – ${p.upper ?? "?"}` : "";
    new maplibregl.Popup({ closeButton: true, maxWidth: "260px" })
      .setLngLat(lngLat)
      .setHTML(
        `<div>
           <div style="font-weight:700;margin-bottom:2px">${escapeHtml(p.name ?? pretty ?? "Airspace")}</div>
           <div style="opacity:.75">
             <span style="display:inline-block;width:8px;height:8px;border-radius:2px;background:${color};margin-right:5px"></span>${escapeHtml(pretty)}${alt ? " · " + escapeHtml(alt) : ""}
           </div>
           ${p.times ? `<div style="opacity:.6;margin-top:3px;font-size:11px">${escapeHtml(p.times)}</div>` : ""}
         </div>`,
      )
      .addTo(this.map);
  }
}

function escapeHtml(s: string): string {
  return String(s).replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ]!,
  );
}

const NM_TO_M = 1852;
const R_EARTH = 6371000;

function destination(
  lat: number,
  lon: number,
  distNm: number,
  bearingDeg: number,
): [number, number] {
  const d = (distNm * NM_TO_M) / R_EARTH;
  const brg = (bearingDeg * Math.PI) / 180;
  const lat1 = (lat * Math.PI) / 180;
  const lon1 = (lon * Math.PI) / 180;
  const lat2 = Math.asin(
    Math.sin(lat1) * Math.cos(d) + Math.cos(lat1) * Math.sin(d) * Math.cos(brg),
  );
  const lon2 =
    lon1 +
    Math.atan2(
      Math.sin(brg) * Math.sin(d) * Math.cos(lat1),
      Math.cos(d) - Math.sin(lat1) * Math.sin(lat2),
    );
  return [(lon2 * 180) / Math.PI, (lat2 * 180) / Math.PI];
}

function circle(lat: number, lon: number, radiusNm: number): [number, number][] {
  const steps = 128;
  const pts: [number, number][] = [];
  // Counter-clockwise (bearing decreasing from north). GeoJSON wants exterior
  // rings CCW; geojson-vt clips a wrong-wound ring inside-out at tile edges,
  // which fills a whole boundary tile instead of the disc.
  for (let i = 0; i < steps; i++) {
    pts.push(destination(lat, lon, radiusNm, -(i * 360) / steps));
  }
  // Close the ring with a vertex identical to the first — MapLibre leaves a
  // notch at the seam when a polygon's first/last points only *nearly* match.
  pts.push([pts[0][0], pts[0][1]]);
  return pts;
}
