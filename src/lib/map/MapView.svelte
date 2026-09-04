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
  import Icon from "../ui/Icon.svelte";
  import RaccoonMark from "../ui/RaccoonMark.svelte";
  import { addCoverageBoundary } from "./coverage";
  import { latestRadar } from "./radar";
  import { makeTransformRequest } from "./tileProxy";
  import { Overlays } from "./overlays";
  import {
    aircraft,
    aircraftGeoJson,
    selectedHex,
    hoveredHex,
    basemap,
    places,
    primaryPlace,
    goHomeSignal,
    layers,
    rangeRingsNm,
    selectedAirport,
    followHex,
    flyTo,
    mapBounds,
    routeLine,
  } from "../state";
  import { setViewport, getTrail, getSettings, updateSettings } from "../api/backend";
  import type { TrailPoint, MapLayers, Place } from "../api/types";
  import { copyText } from "../ui/clipboard";
  import { get } from "svelte/store";
  import { ACCENT, EMERGENCY, ALT_GRADIENT } from "../theme/colors";

  // Aircraft label / outline colours, per basemap brightness. Declared once
  // here because they're applied both at layer creation and again after a
  // live `setStyle()` (the layer survives, its paint props don't).
  const LABEL = {
    dark: { outline: "#f4f7fb", text: "#e6edf3", halo: "#0e1116" },
    light: { outline: "#10141a", text: "#1c1c1c", halo: "#ffffff" },
  } as const;

  let container: HTMLDivElement;
  let map: MlMap | undefined;
  let mapError = "";
  /** Right-click context menu: screen position + the lat/lon under the cursor. */
  let ctxMenu: { x: number; y: number; lat: number; lon: number } | null = null;
  let ctxToast = "";
  let ctxToastTimer: number | undefined;
  let interactionsInstalled = false;
  let moveTimer: number | undefined;
  let trailTimer: number | undefined;
  let radarTimer: number | undefined;
  let unsubGeo: (() => void) | undefined;
  let unsubSel: (() => void) | undefined;
  let unsubBasemap: (() => void) | undefined;
  let unsubPlaces: (() => void) | undefined;
  let unsubGoHome: (() => void) | undefined;
  let placeMarkers = new Map<string, maplibregl.Marker>();
  let activeBasemap = "";
  let themeApplied = "";
  let overlays: Overlays | undefined;
  let overlayTimer: number | undefined;
  let styleSettleTimer: number | undefined;
  let unsubLayers: (() => void) | undefined;
  let unsubRings: (() => void) | undefined;
  let unsubFollow: (() => void) | undefined;
  let unsubFly: (() => void) | undefined;
  let unsubAircraft: (() => void) | undefined;
  let unsubHover: (() => void) | undefined;
  let unsubRoute: (() => void) | undefined;
  let curLayers: MapLayers = {
    airports: false,
    weather: false,
    radar: false,
    airspace: false,
    rangeRings: false,
  };

  const EMPTY_FC = { type: "FeatureCollection", features: [] } as const;

  const preventNativeMenu = (e: Event) => e.preventDefault();

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

  // Selection highlight via feature-state — no geometry rebuild on click. A
  // GeoJSON `setData` can drop feature state, so this is re-run after each feed
  // push and after a style swap.
  let appliedSel: string | null = null;
  function syncSelected() {
    if (!map?.getSource("aircraft")) return;
    const hex = get(selectedHex);
    if (appliedSel && appliedSel !== hex) {
      try {
        map.removeFeatureState({ source: "aircraft", id: appliedSel }, "selected");
      } catch {
        /* feature gone */
      }
    }
    if (hex) {
      try {
        map.setFeatureState({ source: "aircraft", id: hex }, { selected: true });
      } catch {
        /* feature not in view */
      }
    }
    appliedSel = hex;
  }

  async function refreshRadar() {
    const snap = await latestRadar();
    overlays?.setRadarFrame(snap?.tileUrl ?? null);
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
    // Fast path: this runs on every `styledata`; once everything's installed for
    // the current style there's nothing to do.
    if (
      map.getLayer("aircraft-symbol") &&
      map.getLayer("ov-airport-dot") &&
      map.getLayer("coverage-bounds-line") &&
      map.hasImage("ac-jet") &&
      themeApplied === activeBasemap
    ) {
      return;
    }

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
      // Visibility is applied in afterStyleSettled(), not here — calling
      // setLayoutProperty from an install that runs on `styledata` re-fires
      // `styledata` and spins.
      overlays?.install();
    } catch (e) {
      console.error("[diag] overlays install failed:", (e as Error)?.message ?? e);
    }

    const dark = themeFor(activeBasemap).dark;
    const lab = dark ? LABEL.dark : LABEL.light;
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
          "line-color": ACCENT,
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
            ...ALT_GRADIENT.flatMap(([ft, col]) => [ft, col]),
          ],
          "line-opacity": 0.85,
        },
      });
    }

    if (!map.getLayer("aircraft-halo")) {
      // Radius/opacity are expression-driven: `selected` comes from feature-state
      // (set on click, no geometry rebuild), `emergency` from the feature props.
      const selected: any = ["boolean", ["feature-state", "selected"], false];
      const emerg: any = ["boolean", ["get", "emergency"], false];
      map.addLayer({
        id: "aircraft-halo",
        type: "circle",
        source: "aircraft",
        paint: {
          "circle-radius": ["case", selected, 15, emerg, 13, 0],
          "circle-color": ["case", emerg, EMERGENCY, ACCENT],
          "circle-opacity": ["case", ["any", selected, emerg], 0.22, 0] as any,
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
          "text-font": styleFont(),
          "text-size": 10,
          "text-offset": [0, 1.5],
          "text-anchor": "top",
          "text-optional": true,
        },
        paint: {
          "icon-color": ["get", "color"],
          // White outline pops on dark basemaps; dark outline defines the icon
          // on light basemaps.
          "icon-halo-color": lab.outline,
          "icon-halo-width": 1.55,
          "icon-halo-blur": 0.4,
          "text-color": lab.text,
          "text-halo-color": lab.halo,
          "text-halo-width": 1.6,
        },
      });
    }

    // Re-apply theme-dependent colors (the layer persists across style swaps).
    if (map.getLayer("aircraft-symbol") && themeApplied !== activeBasemap) {
      themeApplied = activeBasemap;
      map.setPaintProperty("aircraft-symbol", "icon-halo-color", lab.outline);
      map.setPaintProperty("aircraft-symbol", "text-color", lab.text);
      map.setPaintProperty("aircraft-symbol", "text-halo-color", lab.halo);
      // Overlay labels (airports, range rings) flip with the basemap too.
      for (const id of ["ov-airport-label", "ov-rings-label"]) {
        if (!map.getLayer(id)) continue;
        map.setPaintProperty(id, "text-color", lab.text);
        map.setPaintProperty(id, "text-halo-color", lab.halo);
      }
      if (map.getLayer("ov-airport-dot"))
        map.setPaintProperty("ov-airport-dot", "circle-stroke-color", lab.halo);
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
      closeCtx();
      const hits = map!.queryRenderedFeatures(e.point, {
        layers: ["aircraft-symbol"],
      });
      if (hits.length === 0) selectedHex.set(null);
    });

    map.on("contextmenu", (e) => {
      const el = container;
      const MENU_W = 210;
      const MENU_H = 108;
      const x = Math.max(6, Math.min(e.point.x, el.clientWidth - MENU_W));
      const y = Math.max(6, Math.min(e.point.y, el.clientHeight - MENU_H));
      // Normalise longitude in case the map has wrapped past the antimeridian.
      const lon = ((e.lngLat.lng + 540) % 360) - 180;
      ctxMenu = { x, y, lat: e.lngLat.lat, lon };
    });
    map.on("movestart", closeCtx);
    map.on("dragstart", closeCtx);
  }

  function closeCtx() {
    ctxMenu = null;
  }

  function flashToast(msg: string) {
    ctxToast = msg;
    if (ctxToastTimer) clearTimeout(ctxToastTimer);
    ctxToastTimer = window.setTimeout(() => (ctxToast = ""), 2200);
  }

  async function copyCoords(lat: number, lon: number) {
    closeCtx();
    const ok = await copyText(`${lat.toFixed(5)}, ${lon.toFixed(5)}`);
    flashToast(ok ? "Coordinates copied" : "Couldn't copy coordinates");
  }

  async function addPlaceHere(lat: number, lon: number) {
    closeCtx();
    const cur = get(places);
    const p: Place = {
      id:
        crypto.randomUUID?.() ??
        `p${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`,
      label: `${lat.toFixed(5)}, ${lon.toFixed(5)}`,
      lat,
      lon,
      kind: "coordinates",
      bbox: null,
      primary: cur.length === 0,
      alert: { enabled: false, radiusNm: 10, ceilingFt: null, notableOnly: false },
    };
    const next = [...cur, p];
    try {
      const s = await updateSettings({ places: next });
      places.set(s.places ?? next);
      flashToast("Place added — rename it in Places & alerts");
    } catch {
      flashToast("Couldn't add place");
    }
  }

  onMount(async () => {
    let cacheEnabled = false;
    let basemapKey = "darkMatter";
    let initialPlaces: Place[] = [];
    try {
      const s = await getSettings();
      cacheEnabled = s.tileCacheEnabled;
      basemapKey = s.basemap;
      initialPlaces = s.places ?? [];
      if (s.layers) layers.set(s.layers);
      if (s.rangeRingsNm?.length) rangeRingsNm.set(s.rangeRingsNm);
    } catch {
      /* backend still starting — use defaults */
    }
    activeBasemap = basemapKey;
    basemap.set(basemapKey);
    const styleUrl = resolveStyleUrl(basemapKey);
    const transformRequest = await makeTransformRequest(cacheEnabled);

    const startPlace =
      initialPlaces.find((p) => p.primary) ?? initialPlaces[0] ?? null;
    const start = startPlace
      ? placeCamera(startPlace)
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

    // Suppress the webview's native context menu over the map so ours shows.
    container.addEventListener("contextmenu", preventNativeMenu);

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
    // Nav + scale stay on the left but are nudged clear of the panel rail in
    // app.css. A left-docked panel, when open, sits over them — acceptable
    // since the right side is reserved for the selection detail panel.
    map.addControl(new maplibregl.NavigationControl({ showCompass: true }), "top-left");
    map.addControl(new maplibregl.ScaleControl({ unit: "nautical" }), "bottom-left");

    // `load` fires on the initial style; `style.load` after a setStyle(); and
    // `styledata` fires many times per style change. Keeping the app's layers
    // present is cheap + idempotent and runs on every `styledata`; the heavy
    // work (viewport push, overlay refetch, visibility, selection) is coalesced
    // into one trailing pass so the `styledata` burst from addLayer/setStyle
    // doesn't run it a dozen times.
    const ensureLayers = () => {
      installLayers();
      installInteractions();
    };
    let settleRetries = 0;
    const afterStyleSettled = () => {
      if (!map) return;
      if (!map.isStyleLoaded() && settleRetries < 20) {
        settleRetries++;
        styleSettleTimer = window.setTimeout(afterStyleSettled, 90);
        return;
      }
      settleRetries = 0;
      ensureLayers();
      overlays?.setVisibility(curLayers);
      overlays?.setRangeRings(
        get(primaryPlace),
        get(rangeRingsNm),
        curLayers.rangeRings,
      );
      overlays?.setPlaceRings(get(places));
      syncSelected();
      pushViewport();
      refreshOverlays();
    };
    const onStyleEvent = () => {
      ensureLayers();
      settleRetries = 0;
      if (styleSettleTimer) clearTimeout(styleSettleTimer);
      styleSettleTimer = window.setTimeout(afterStyleSettled, 90);
    };
    map.on("load", onStyleEvent);
    map.on("style.load", onStyleEvent);
    map.on("styledata", onStyleEvent);

    unsubGeo = aircraftGeoJson.subscribe((fc) => {
      const src = map?.getSource("aircraft") as GeoJSONSource | undefined;
      if (!src) return;
      src.setData(fc as any);
      syncSelected();
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

    // Selected aircraft -> highlight it, refresh its trail now and periodically.
    unsubSel = selectedHex.subscribe(() => {
      syncSelected();
      void refreshTrail();
    });
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
      overlays?.setRangeRings(get(primaryPlace), rr, curLayers.rangeRings);
    };
    let radarWasOn = false;
    unsubLayers = layers.subscribe((l) => {
      curLayers = l;
      overlays?.setVisibility(l);
      drawRings();
      refreshOverlays();
      if (l.radar && !radarWasOn) void refreshRadar();
      radarWasOn = l.radar;
    });
    unsubRings = rangeRingsNm.subscribe(drawRings);
    radarTimer = window.setInterval(() => {
      if (curLayers.radar) void refreshRadar();
    }, 5 * 60 * 1000);

    // Saved places: seed from settings (initial camera is already set above),
    // drop the markers + alert rings, and react to later changes.
    places.set(initialPlaces);
    unsubPlaces = places.subscribe((ps) => {
      renderPlaces(ps);
      overlays?.setPlaceRings(ps);
      drawRings();
    });
    unsubGoHome = goHomeSignal.subscribe((n) => {
      const p = get(primaryPlace);
      if (n > 0 && p) recenterHome(p, true);
    });
  });

  function placePinSvg(primary: boolean): string {
    const fill = primary ? ACCENT : "#8b949e";
    return `<svg viewBox="0 0 24 24" width="${primary ? 26 : 22}" height="${primary ? 26 : 22}"><path d="M12 2 C7 2 3.5 6 3.5 10.5 C3.5 17 12 23 12 23 C12 23 20.5 17 20.5 10.5 C20.5 6 17 2 12 2 Z" fill="${fill}" stroke="#0b1220" stroke-width="1.5"/><circle cx="12" cy="10.5" r="3.4" fill="#0b1220"/></svg>`;
  }

  function renderPlaces(ps: Place[]) {
    if (!map) return;
    const wanted = new Set(ps.map((p) => p.id));
    for (const [id, m] of placeMarkers) {
      if (!wanted.has(id)) {
        m.remove();
        placeMarkers.delete(id);
      }
    }
    for (const p of ps) {
      if (!Number.isFinite(p.lon) || !Number.isFinite(p.lat)) continue;
      let m = placeMarkers.get(p.id);
      if (!m) {
        const el = document.createElement("div");
        el.className = "place-marker";
        // setLngLat BEFORE addTo — a Marker added without a position throws
        // every render frame and freezes the map.
        m = new maplibregl.Marker({ element: el, anchor: "bottom" })
          .setLngLat([p.lon, p.lat])
          .addTo(map);
        placeMarkers.set(p.id, m);
      } else {
        m.setLngLat([p.lon, p.lat]);
      }
      const el = m.getElement();
      el.innerHTML = placePinSvg(p.primary);
      el.title = p.alert.enabled
        ? `${p.label} · alert ${p.alert.radiusNm} nm`
        : p.label;
      el.onclick = (ev) => {
        ev.stopPropagation();
        flyTo.set({ lat: p.lat, lon: p.lon, zoom: Math.max(map!.getZoom(), 9) });
      };
    }
  }

  /** A valid, region-clamped center + zoom for a place. */
  function placeCamera(h: Place): {
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

  function recenterHome(h: Place, animate: boolean) {
    if (!map) return;
    const { center, zoom } = placeCamera(h);
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
    if (radarTimer) clearInterval(radarTimer);
    if (overlayTimer) clearTimeout(overlayTimer);
    if (styleSettleTimer) clearTimeout(styleSettleTimer);
    if (restoreBoundsTimer) clearTimeout(restoreBoundsTimer);
    if (ctxToastTimer) clearTimeout(ctxToastTimer);
    container?.removeEventListener("contextmenu", preventNativeMenu);
    unsubGeo?.();
    unsubSel?.();
    unsubBasemap?.();
    unsubPlaces?.();
    unsubGoHome?.();
    unsubLayers?.();
    unsubRings?.();
    unsubFollow?.();
    unsubFly?.();
    unsubAircraft?.();
    unsubHover?.();
    unsubRoute?.();
    for (const m of placeMarkers.values()) m.remove();
    placeMarkers.clear();
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

<svelte:window
  on:keydown={(e) => e.key === "Escape" && closeCtx()}
  on:click={() => ctxMenu && closeCtx()}
  on:blur={closeCtx}
/>

<div class="map" bind:this={container}></div>

{#if ctxMenu}
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div
    class="ctx-menu"
    style="left:{ctxMenu.x}px; top:{ctxMenu.y}px"
    on:contextmenu|preventDefault
  >
    <div class="ctx-coord">
      {ctxMenu.lat.toFixed(5)}, {ctxMenu.lon.toFixed(5)}
    </div>
    <button on:click={() => ctxMenu && copyCoords(ctxMenu.lat, ctxMenu.lon)}>
      <Icon name="crosshair" size={13} /> Copy coordinates
    </button>
    <button on:click={() => ctxMenu && addPlaceHere(ctxMenu.lat, ctxMenu.lon)}>
      <Icon name="map-pin" size={13} /> Add place here
    </button>
  </div>
{/if}

{#if ctxToast}
  <div class="ctx-toast">{ctxToast}</div>
{/if}

{#if !mapError && $aircraftGeoJson.features.length === 0}
  <div class="empty-mark">
    <RaccoonMark size={96} />
    <p>No aircraft in view — pan the map or wait for the next sweep.</p>
  </div>
{/if}

{#if $followHex}
  <button class="follow-chip" on:click={() => followHex.set(null)}>
    <Icon name="crosshair" size={13} />
    Following {($aircraft.get($followHex)?.flight ?? $followHex).trim()} — click to stop
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
  .empty-mark {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -55%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    pointer-events: none;
    z-index: 4;
    color: var(--text-dim);
    text-align: center;
  }
  .empty-mark :global(.rm) {
    opacity: 0.9;
    filter: drop-shadow(0 2px 8px rgba(0, 0, 0, 0.35));
  }
  .empty-mark p {
    margin: 0;
    font-size: 11px;
    opacity: 0.85;
    max-width: 240px;
  }
  .map-error {
    position: absolute;
    top: 8px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 6;
    max-width: 70%;
    background: var(--bg-panel);
    border: 1px solid var(--emergency);
    color: var(--emergency);
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
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: var(--radius-sm);
    padding: 4px 12px;
    font-size: var(--fs-md);
    font-weight: 600;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
  }
  .ctx-menu {
    position: absolute;
    z-index: 20;
    min-width: 190px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-panel);
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .ctx-coord {
    font-size: 10px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
    padding: 3px 8px 5px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 3px;
  }
  .ctx-menu button {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: var(--text);
    font-size: var(--fs-md);
    padding: 6px 8px;
    border-radius: 4px;
  }
  .ctx-menu button:hover {
    background: var(--bg-elev);
  }
  .ctx-toast {
    position: absolute;
    bottom: 34px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 20;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    color: var(--text);
    font-size: var(--fs-md);
    padding: 5px 12px;
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-panel);
  }
  :global(.place-marker) {
    cursor: pointer;
    filter: drop-shadow(0 2px 3px rgba(0, 0, 0, 0.55));
  }
  :global(.place-marker svg) {
    display: block;
  }
</style>
