<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import maplibregl from "maplibre-gl/dist/maplibre-gl-csp";
  import type { Map as MlMap, GeoJSONSource } from "maplibre-gl";
  import {
    NA_MAX_BOUNDS,
    NA_BOUNDS,
    INITIAL_CENTER,
    INITIAL_ZOOM,
    MIN_ZOOM,
    MAX_ZOOM,
    clipToRegion,
    type Bbox,
  } from "./region";
  import {
    BASEMAP_ATTRIBUTION,
    DATA_ATTRIBUTION,
    resolveStyleUrl,
    themeFor,
  } from "./style";
  import { registerAircraftIcons } from "./icons";
  import { addCoverageBoundary } from "./coverage";
  import { makeTransformRequest } from "./tileProxy";
  import { Overlays } from "./overlays";
  import {
    aircraft,
    aircraftGeoJson,
    selectedHex,
    hoveredHex,
    basemap,
    home,
    goHomeSignal,
    layers,
    rangeRingsNm,
    selectedAirport,
    followHex,
    flyTo,
    mapBounds,
    geofences,
    routeLine,
  } from "../state";
  import { setViewport, getTrail, getSettings } from "../api/backend";
  import type { TrailPoint, HomeLocation, MapLayers } from "../api/types";
  import { get } from "svelte/store";

  let container: HTMLDivElement;
  let map: MlMap | undefined;
  let mapError = "";
  let interactionsInstalled = false;
  let moveTimer: number | undefined;
  let trailTimer: number | undefined;
  let unsubGeo: (() => void) | undefined;
  let unsubSel: (() => void) | undefined;
  let unsubBasemap: (() => void) | undefined;
  let unsubHome: (() => void) | undefined;
  let unsubGoHome: (() => void) | undefined;
  let homeMarker: maplibregl.Marker | undefined;
  let activeBasemap = "";
  let themeApplied = "";
  let overlays: Overlays | undefined;
  let overlayTimer: number | undefined;
  let unsubLayers: (() => void) | undefined;
  let unsubRings: (() => void) | undefined;
  let unsubFences: (() => void) | undefined;
  let unsubFollow: (() => void) | undefined;
  let unsubFly: (() => void) | undefined;
  let unsubAircraft: (() => void) | undefined;
  let unsubHover: (() => void) | undefined;
  let unsubRoute: (() => void) | undefined;
  let curLayers: MapLayers = {
    airports: false,
    weather: false,
    airspace: false,
    rangeRings: false,
  };

  const EMPTY_FC = { type: "FeatureCollection", features: [] } as const;

  function pushViewport() {
    if (!map) return;
    const b = map.getBounds();
    const bbox: Bbox = {
      west: b.getWest(),
      south: b.getSouth(),
      east: b.getEast(),
      north: b.getNorth(),
    };
    mapBounds.set(bbox);
    const clipped = clipToRegion(bbox);
    if (clipped) void setViewport(clipped, map.getZoom());
  }

  function followAircraft() {
    const hex = get(followHex);
    if (!map || !hex) return;
    const a = get(aircraft).get(hex);
    if (!a || a.lat === null || a.lon === null) return;
    const c = map.getCenter();
    // Only recenter once the target drifts from centre, and gently.
    if (Math.hypot(c.lng - a.lon, c.lat - a.lat) < 0.002) return;
    map.easeTo({ center: [a.lon, a.lat], duration: 400 });
  }

  function updateHoverRing() {
    const src = map?.getSource("hover") as GeoJSONSource | undefined;
    if (!src) return;
    const hex = get(hoveredHex);
    const a = hex ? get(aircraft).get(hex) : null;
    if (a && a.lat !== null && a.lon !== null) {
      src.setData({
        type: "FeatureCollection",
        features: [
          { type: "Feature", properties: {}, geometry: { type: "Point", coordinates: [a.lon, a.lat] } },
        ],
      } as any);
    } else {
      src.setData(EMPTY_FC as any);
    }
  }

  function scheduleViewport() {
    if (moveTimer) clearTimeout(moveTimer);
    moveTimer = window.setTimeout(pushViewport, 400);
    if (overlayTimer) clearTimeout(overlayTimer);
    overlayTimer = window.setTimeout(refreshOverlays, 700);
  }

  function refreshOverlays() {
    if (!map || !overlays) return;
    if (!curLayers.airports && !curLayers.weather && !curLayers.airspace) return;
    const b = map.getBounds();
    void overlays.refresh(
      {
        west: b.getWest(),
        south: b.getSouth(),
        east: b.getEast(),
        north: b.getNorth(),
      },
      curLayers,
    );
  }

  function trailToGeoJson(points: TrailPoint[]) {
    // Split into segments so the line can be colored by altitude band.
    return {
      type: "FeatureCollection" as const,
      features:
        points.length < 2
          ? []
          : points.slice(1).map((p, i) => {
              const a = points[i];
              return {
                type: "Feature" as const,
                geometry: {
                  type: "LineString" as const,
                  coordinates: [
                    [a.lon, a.lat],
                    [p.lon, p.lat],
                  ],
                },
                properties: { alt: p.altBaro ?? 0 },
              };
            }),
    };
  }

  async function refreshTrail() {
    if (!map) return;
    const hex = get(selectedHex);
    const src = map.getSource("trail") as GeoJSONSource | undefined;
    if (!src) return;
    if (!hex) {
      src.setData(EMPTY_FC as any);
      return;
    }
    try {
      const points = await getTrail(hex);
      src.setData(trailToGeoJson(points) as any);
    } catch {
      /* backend not ready yet */
    }
  }

  /** First font stack the active basemap ships, so aircraft labels always render. */
  function styleFont(): string[] {
    try {
      for (const l of map!.getStyle().layers ?? []) {
        const f = (l as any).layout?.["text-font"];
        if (Array.isArray(f) && f.length) return f;
      }
    } catch {
      /* style not ready */
    }
    return ["Open Sans Regular", "Noto Sans Regular"];
  }

  // (Re)create the app's sources / layers / images. Idempotent — safe to call
  // on `load`, `style.load` and `styledata`, and again after a basemap swap
  // wipes everything the app added.
  function installLayers() {
    if (!map || !map.isStyleLoaded()) return;
    try {
      doInstallLayers(map);
    } catch (e) {
      console.error("[diag] installLayers failed:", (e as Error)?.stack ?? e);
    }
  }

  function doInstallLayers(map: MlMap) {
    registerAircraftIcons(map);

    if (!map.getSource("aircraft")) {
      map.addSource("aircraft", {
        type: "geojson",
        data: get(aircraftGeoJson) as any,
        promoteId: "hex",
      });
    }
    if (!map.getSource("trail")) {
      map.addSource("trail", { type: "geojson", data: EMPTY_FC as any });
    }
    if (!map.getSource("hover")) {
      map.addSource("hover", { type: "geojson", data: EMPTY_FC as any });
    }
    if (!map.getSource("route")) {
      map.addSource("route", { type: "geojson", data: EMPTY_FC as any });
    }

    try {
      addCoverageBoundary(map);
    } catch (e) {
      console.error("[diag] coverage boundary failed:", (e as Error)?.message ?? e);
    }
    try {
      overlays?.install();
      overlays?.setVisibility(curLayers);
      overlays?.setGeofences(get(geofences) as any);
    } catch (e) {
      console.error("[diag] overlays install failed:", (e as Error)?.message ?? e);
    }

    const font = styleFont();
    const dark = themeFor(activeBasemap).dark;
    const iconSize: any = [
      "interpolate",
      ["linear"],
      ["zoom"],
      3,
      ["*", 0.62, ["get", "sizeMul"]],
      8,
      ["*", 1.15, ["get", "sizeMul"]],
    ];

    if (!map.getLayer("route-remain")) {
      map.addLayer({
        id: "route-remain",
        type: "line",
        source: "route",
        filter: ["==", ["get", "leg"], "remain"],
        layout: { "line-cap": "round", "line-join": "round" },
        paint: {
          "line-width": 2,
          "line-color": "#7f8b99",
          "line-dasharray": [2, 2],
          "line-opacity": 0.7,
        },
      });
    }
    if (!map.getLayer("route-flown")) {
      map.addLayer({
        id: "route-flown",
        type: "line",
        source: "route",
        filter: ["==", ["get", "leg"], "flown"],
        layout: { "line-cap": "round", "line-join": "round" },
        paint: {
          "line-width": 2.5,
          "line-color": "#4c9be8",
          "line-opacity": 0.9,
        },
      });
    }

    if (!map.getLayer("trail-line")) {
      map.addLayer({
        id: "trail-line",
        type: "line",
        source: "trail",
        paint: {
          "line-width": 2,
          "line-color": [
            "interpolate",
            ["linear"],
            ["get", "alt"],
            0,
            "#7ad151",
            15000,
            "#f9c74f",
            30000,
            "#f3722c",
            42000,
            "#b5179e",
          ],
          "line-opacity": 0.85,
        },
      });
    }

    if (!map.getLayer("aircraft-halo")) {
      map.addLayer({
        id: "aircraft-halo",
        type: "circle",
        source: "aircraft",
        filter: ["any", ["get", "selected"], ["get", "emergency"]],
        paint: {
          "circle-radius": 14,
          "circle-color": ["case", ["get", "emergency"], "#ff3b30", "#4c9be8"],
          "circle-opacity": 0.22,
        },
      });
    }

    if (!map.getLayer("hover-ring")) {
      map.addLayer({
        id: "hover-ring",
        type: "circle",
        source: "hover",
        paint: {
          "circle-radius": 12,
          "circle-opacity": 0,
          "circle-stroke-color": "#ffffff",
          "circle-stroke-width": 2,
          "circle-stroke-opacity": 0.9,
        },
      });
    }

    // Soft drop shadow: a dark, blurred, offset copy of each icon.
    if (!map.getLayer("aircraft-shadow")) {
      map.addLayer({
        id: "aircraft-shadow",
        type: "symbol",
        source: "aircraft",
        layout: {
          "icon-image": ["get", "icon"],
          "icon-size": iconSize,
          "icon-rotate": ["get", "rotation"],
          "icon-rotation-alignment": "map",
          "icon-allow-overlap": true,
          "icon-ignore-placement": true,
        },
        paint: {
          // Keep the halo modest — a large `icon-halo-blur` on an SDF bleeds to
          // the sprite quad and renders as a fuzzy square instead of the shape.
          "icon-color": "#000000",
          "icon-opacity": 0.4,
          "icon-halo-color": "#000000",
          "icon-halo-width": 1.5,
          "icon-halo-blur": 2,
          "icon-translate": [2, 3],
          "icon-translate-anchor": "viewport",
        },
      });
    }

    const freshInstall = !map.getLayer("aircraft-symbol");
    if (freshInstall) {
      map.addLayer({
        id: "aircraft-symbol",
        type: "symbol",
        source: "aircraft",
        layout: {
          "icon-image": ["get", "icon"],
          "icon-size": iconSize,
          "icon-rotate": ["get", "rotation"],
          "icon-rotation-alignment": "map",
          "icon-allow-overlap": true,
          "icon-ignore-placement": true,
          "text-field": ["step", ["zoom"], "", 6, ["get", "callsign"]],
          "text-font": font,
          "text-size": 10,
          "text-offset": [0, 1.5],
          "text-anchor": "top",
          "text-optional": true,
        },
        paint: {
          "icon-color": ["get", "color"],
          // White outline pops on dark basemaps; dark outline defines the icon
          // on light basemaps.
          "icon-halo-color": dark ? "#f4f7fb" : "#10141a",
          "icon-halo-width": 1.55,
          "icon-halo-blur": 0.4,
          "text-color": dark ? "#e6edf3" : "#1c1c1c",
          "text-halo-color": dark ? "#0e1116" : "#ffffff",
          "text-halo-width": 1.6,
        },
      });
    }

    // Re-apply theme-dependent colors (the layer persists across style swaps).
    if (map.getLayer("aircraft-symbol") && themeApplied !== activeBasemap) {
      themeApplied = activeBasemap;
      map.setPaintProperty(
        "aircraft-symbol",
        "icon-halo-color",
        dark ? "#f4f7fb" : "#10141a",
      );
      map.setPaintProperty(
        "aircraft-symbol",
        "text-color",
        dark ? "#e6edf3" : "#1c1c1c",
      );
      map.setPaintProperty(
        "aircraft-symbol",
        "text-halo-color",
        dark ? "#0e1116" : "#ffffff",
      );
    }

    if (freshInstall) {
      void refreshTrail();
      renderRoute(get(routeLine));
    }
  }

  function renderRoute(
    r: { flown: [number, number][]; remain: [number, number][] } | null,
  ) {
    const src = map?.getSource("route") as GeoJSONSource | undefined;
    if (!src) return;
    const features: any[] = [];
    if (r && r.flown.length > 1) {
      features.push({
        type: "Feature",
        geometry: { type: "LineString", coordinates: r.flown },
        properties: { leg: "flown" },
      });
    }
    if (r && r.remain.length > 1) {
      features.push({
        type: "Feature",
        geometry: { type: "LineString", coordinates: r.remain },
        properties: { leg: "remain" },
      });
    }
    src.setData({ type: "FeatureCollection", features } as any);
  }

  // Event handlers that should be bound exactly once for the map's lifetime.
  function installInteractions() {
    if (!map || interactionsInstalled) return;
    interactionsInstalled = true;

    map.on("mouseenter", "aircraft-symbol", () => {
      map!.getCanvas().style.cursor = "pointer";
    });
    map.on("mouseleave", "aircraft-symbol", () => {
      map!.getCanvas().style.cursor = "";
      hoveredHex.set(null);
    });
    map.on("mousemove", "aircraft-symbol", (e) => {
      const f = e.features?.[0];
      hoveredHex.set(f ? (f.properties?.hex as string) : null);
    });
    map.on("click", "aircraft-symbol", (e) => {
      const f = e.features?.[0];
      if (f) {
        selectedAirport.set(null);
        selectedHex.set(f.properties?.hex as string);
      }
    });
    map.on("click", (e) => {
      const hits = map!.queryRenderedFeatures(e.point, {
        layers: ["aircraft-symbol"],
      });
      if (hits.length === 0) selectedHex.set(null);
    });
  }

  onMount(async () => {
    let cacheEnabled = false;
    let basemapKey = "darkMatter";
    let initialHome: HomeLocation | null = null;
    try {
      const s = await getSettings();
      cacheEnabled = s.tileCacheEnabled;
      basemapKey = s.basemap;
      initialHome = s.home ?? null;
      if (s.layers) layers.set(s.layers);
      if (s.rangeRingsNm?.length) rangeRingsNm.set(s.rangeRingsNm);
    } catch {
      /* backend still starting — use defaults */
    }
    activeBasemap = basemapKey;
    basemap.set(basemapKey);
    const styleUrl = resolveStyleUrl(basemapKey);
    const transformRequest = await makeTransformRequest(cacheEnabled);

    const start = initialHome
      ? homeCamera(initialHome)
      : { center: INITIAL_CENTER, zoom: INITIAL_ZOOM };

    map = new maplibregl.Map({
      container,
      style: styleUrl,
      center: start.center,
      zoom: start.zoom,
      minZoom: MIN_ZOOM,
      maxZoom: MAX_ZOOM,
      maxBounds: NA_MAX_BOUNDS,
      attributionControl: false,
      transformRequest,
    });
    overlays = new Overlays(map);

    map.on("error", (e) => {
      // Surfaced so a broken basemap / tile source is visible during testing.
      const msg = (e as any)?.error?.message ?? String((e as any)?.error ?? e);
      console.error("[maplibre]", msg);
      mapError = msg;
    });

    // The OpenFreeMap style references a few sprite icons that may be absent;
    // supply a blank 1px image so those symbol layers don't warn/skip.
    map.on("styleimagemissing", (e) => {
      const id = (e as any).id as string;
      if (map && !map.hasImage(id)) {
        map.addImage(id, { width: 1, height: 1, data: new Uint8Array(4) });
      }
    });

    map.addControl(
      new maplibregl.AttributionControl({
        compact: true,
        customAttribution: [DATA_ATTRIBUTION, BASEMAP_ATTRIBUTION],
      }),
    );
    map.addControl(new maplibregl.NavigationControl({ showCompass: true }), "top-left");
    map.addControl(new maplibregl.ScaleControl({ unit: "nautical" }), "bottom-left");

    // `load` fires on the initial style; `style.load` after a setStyle(); and
    // `styledata` covers edge cases where neither fires as expected. All three
    // funnel through the idempotent installer.
    const onStyleReady = () => {
      installLayers();
      installInteractions();
      pushViewport();
      refreshOverlays();
    };
    map.on("load", onStyleReady);
    map.on("style.load", onStyleReady);
    map.on("styledata", onStyleReady);

    unsubGeo = aircraftGeoJson.subscribe((fc) => {
      const src = map?.getSource("aircraft") as GeoJSONSource | undefined;
      src?.setData(fc as any);
    });
    unsubAircraft = aircraft.subscribe(() => {
      followAircraft();
      updateHoverRing();
    });
    unsubHover = hoveredHex.subscribe(() => updateHoverRing());
    unsubRoute = routeLine.subscribe((r) => renderRoute(r));

    unsubFollow = followHex.subscribe((hex) => {
      if (hex) {
        selectedHex.set(hex);
        followAircraft();
      }
    });
    unsubFly = flyTo.subscribe((t) => {
      if (!t || !map) return;
      followHex.set(null);
      map.easeTo({
        center: [t.lon, t.lat],
        zoom: t.zoom ?? Math.max(map.getZoom(), 8),
        duration: 600,
      });
      flyTo.set(null);
    });

    map.on("moveend", scheduleViewport);

    // Selected aircraft -> refresh its trail now and every few seconds.
    unsubSel = selectedHex.subscribe(() => void refreshTrail());
    trailTimer = window.setInterval(refreshTrail, 4000);

    unsubBasemap = basemap.subscribe((key) => {
      if (!map || key === activeBasemap) return;
      activeBasemap = key;
      mapError = "";
      map.setStyle(resolveStyleUrl(key));
    });

    // Reference overlays.
    const drawRings = () => {
      const rr = get(rangeRingsNm);
      overlays?.setRangeRings(get(home), rr, curLayers.rangeRings);
    };
    unsubLayers = layers.subscribe((l) => {
      curLayers = l;
      overlays?.setVisibility(l);
      drawRings();
      refreshOverlays();
    });
    unsubRings = rangeRingsNm.subscribe(drawRings);
    unsubFences = geofences.subscribe((f) => overlays?.setGeofences(f as any));

    // Home location: seed from settings (initial camera is already set above),
    // drop the marker, and react to later changes.
    home.set(initialHome);
    unsubHome = home.subscribe((h) => {
      renderHome(h);
      overlays?.setRangeRings(h, get(rangeRingsNm), curLayers.rangeRings);
    });
    unsubGoHome = goHomeSignal.subscribe((n) => {
      const h = get(home);
      if (n > 0 && h) recenterHome(h, true);
    });
  });

  function renderHome(h: HomeLocation | null) {
    if (!map) return;
    if (!h || !Number.isFinite(h.lon) || !Number.isFinite(h.lat)) {
      homeMarker?.remove();
      homeMarker = undefined;
      return;
    }
    if (!homeMarker) {
      const el = document.createElement("div");
      el.className = "home-marker";
      el.innerHTML =
        '<svg viewBox="0 0 24 24" width="26" height="26"><path d="M12 2 C7 2 3.5 6 3.5 10.5 C3.5 17 12 23 12 23 C12 23 20.5 17 20.5 10.5 C20.5 6 17 2 12 2 Z" fill="#4c9be8" stroke="#0b1220" stroke-width="1.5"/><circle cx="12" cy="10.5" r="3.4" fill="#0b1220"/></svg>';
      // setLngLat BEFORE addTo — a Marker added without a position throws every
      // render frame from MapLibre's projection helper and freezes the map.
      homeMarker = new maplibregl.Marker({ element: el, anchor: "bottom" })
        .setLngLat([h.lon, h.lat])
        .addTo(map);
    } else {
      homeMarker.setLngLat([h.lon, h.lat]);
    }
    homeMarker.getElement().title = `Home — ${h.label}`;
  }

  /** A valid, region-clamped center + zoom for a home location. */
  function homeCamera(h: HomeLocation): {
    center: [number, number];
    zoom: number;
  } {
    let lon = h.lon;
    let lat = h.lat;
    let zoom = 12;
    if (h.bbox && h.bbox.length === 4 && h.bbox.every((v) => Number.isFinite(v))) {
      const [w, s, e, n] = h.bbox;
      lon = (w + e) / 2;
      lat = (s + n) / 2;
      const span = Math.max(Math.abs(e - w), Math.abs(n - s));
      zoom =
        span > 20 ? 4.3 : span > 6 ? 5.8 : span > 1.5 ? 7.5 : span > 0.3 ? 9.5 : 11.5;
    }
    if (!Number.isFinite(lon) || !Number.isFinite(lat)) {
      return { center: INITIAL_CENTER, zoom: INITIAL_ZOOM };
    }
    return {
      center: [
        Math.min(Math.max(lon, NA_BOUNDS.west + 1), NA_BOUNDS.east - 1),
        Math.min(Math.max(lat, NA_BOUNDS.south + 1), NA_BOUNDS.north - 1),
      ],
      zoom: Math.min(Math.max(zoom, MIN_ZOOM + 0.2), MAX_ZOOM),
    };
  }

  let restoreBoundsTimer: number | undefined;

  function recenterHome(h: HomeLocation, animate: boolean) {
    if (!map) return;
    const { center, zoom } = homeCamera(h);
    const m = map;

    // Animating with `maxBounds` set can throw repeatedly from MapLibre's camera
    // constraint solver (undefined LngLat) and lock the render loop. Lift the
    // constraint for the duration of the move, then restore it.
    if (restoreBoundsTimer) clearTimeout(restoreBoundsTimer);
    m.stop();
    m.setMaxBounds(null);
    m.easeTo({ center, zoom, duration: animate ? 700 : 0 });
    restoreBoundsTimer = window.setTimeout(
      () => map?.setMaxBounds(NA_MAX_BOUNDS),
      animate ? 1000 : 120,
    );
  }

  onDestroy(() => {
    if (moveTimer) clearTimeout(moveTimer);
    if (trailTimer) clearInterval(trailTimer);
    if (overlayTimer) clearTimeout(overlayTimer);
    if (restoreBoundsTimer) clearTimeout(restoreBoundsTimer);
    unsubGeo?.();
    unsubSel?.();
    unsubBasemap?.();
    unsubHome?.();
    unsubGoHome?.();
    unsubLayers?.();
    unsubRings?.();
    unsubFences?.();
    unsubFollow?.();
    unsubFly?.();
    unsubAircraft?.();
    unsubHover?.();
    unsubRoute?.();
    homeMarker?.remove();
    map?.remove();
  });

  export function flyToAircraft(lon: number, lat: number) {
    map?.easeTo({ center: [lon, lat], zoom: Math.max(map.getZoom(), 8) });
  }

  export function currentBounds(): Bbox | null {
    if (!map) return null;
    const b = map.getBounds();
    return {
      west: b.getWest(),
      south: b.getSouth(),
      east: b.getEast(),
      north: b.getNorth(),
    };
  }
</script>

<div class="map" bind:this={container}></div>
{#if $followHex}
  <button class="follow-chip" on:click={() => followHex.set(null)}>
    ⊙ Following {($aircraft.get($followHex)?.flight ?? $followHex).trim()} — click to stop
  </button>
{/if}
{#if mapError}
  <div class="map-error" title={mapError}>Basemap error: {mapError}</div>
{/if}

<style>
  .map {
    position: absolute;
    inset: 0;
  }
  .map-error {
    position: absolute;
    top: 8px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 6;
    max-width: 70%;
    background: #5a1e1e;
    border: 1px solid var(--emergency);
    color: #ffd7d3;
    font-size: 11px;
    padding: 4px 10px;
    border-radius: 6px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  :global(.maplibregl-ctrl-attrib) {
    font-size: 10px;
  }
  .follow-chip {
    position: absolute;
    top: 8px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 6;
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 6px;
    padding: 4px 12px;
    font-size: 12px;
    font-weight: 600;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
  }
  :global(.home-marker) {
    cursor: default;
    filter: drop-shadow(0 2px 3px rgba(0, 0, 0, 0.55));
  }
  :global(.home-marker svg) {
    display: block;
  }
</style>
