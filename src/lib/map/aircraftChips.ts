// Aircraft info-chip (pill + callsign/altitude), its RTL-SDR wifi badge, and
// the leader line joining it to the plane — rendered via a deck.gl overlay
// instead of MapLibre symbol layers.
//
// Why: the MapLibre version needed four independent symbol layers (chip
// pill+text, badge, leader line, drop shadow) tied together only by
// happening to share a screen position. MapLibre's collision system
// evaluates each layer's placement independently, so there was no way to
// guarantee they'd show or hide as one unit — see git history for two
// separate bugs from that: an orphaned badge outliving a dropped chip, and
// (when the badge was made collision-managed to fix that) the badge
// colliding with its own chip and taking the chip down instead.
//
// deck.gl doesn't solve this automatically either — its CollisionFilter
// extension evaluates collision per rendered sub-layer, not atomically
// across a composite object (see visgl/deck.gl discussion #8488, the same
// bug category in a different framework) — so this doesn't lean on any
// framework's built-in collision system for "keep these pieces together."
// `computeVisible` below decides, once, in plain JS, which aircraft get a
// chip this frame, and every layer in `buildChipLayers` is fed that same
// pre-filtered array. That's a JS-level guarantee: there's no code path
// where a badge, line, or background can render without the others,
// because they're not independent decisions — they're the same one.
//
// The plane icon itself, its own drop shadow, the selection halo, and the
// hover ring are unaffected and stay as plain MapLibre layers (see
// MapView.svelte) — a lone icon or a plain circle never had this
// composability problem in the first place.

import { IconLayer, TextLayer } from "@deck.gl/layers";
import type { Layer } from "@deck.gl/core";
import type { Map as MlMap } from "maplibre-gl";
import type { AircraftFeature } from "../state";
import { FLIGHT_CATEGORY_COLORS } from "../theme/colors";

export const CHIP_MINZOOM = 6;

/** The MapLibre plane-icon layer id (see MapView.svelte) — the leader line
 *  is inserted just *before* it in the paint order (see `beforeId` below)
 *  so the plane visually paints over the near end of the line, matching a
 *  real leader line "emerging from under" its aircraft. Exported so the
 *  caller can check the layer actually exists before calling
 *  `buildChipLayers` — `beforeId` referencing a layer MapLibre doesn't have
 *  yet (e.g. mid-basemap-swap, before it's been re-added) throws. */
export const PLANE_LAYER_ID = "aircraft-symbol";

// --- badge + leader-line raster icons ---------------------------------------
//
// Both are "mask" icons in deck.gl's IconLayer sense: a plain white shape,
// tinted per-instance via `getColor` — the same mental model the old SDF
// icons used for MapLibre's `icon-color`, just without needing an actual
// signed-distance field (deck.gl antialiases mask icons directly, so a flat
// canvas raster is enough).

const BADGE_PX = 22;
// Backdrop behind the badge — a border-color ring plus a slightly smaller
// pill-color fill circle, the same two-tone treatment as the pill's own
// background+border, so the badge reads as part of the same chip instead of
// a bare icon floating against whatever's under it (map, another chip, the
// pill's own hard-edged border).
const BADGE_RING_PX = BADGE_PX + 5;
const BADGE_FILL_PX = BADGE_PX + 2;
const LEADER_W_PX = 40;
const LEADER_H_PX = 6;
const ATLAS_W = BADGE_PX + BADGE_RING_PX + BADGE_FILL_PX + LEADER_W_PX;
const ATLAS_H = Math.max(BADGE_RING_PX, LEADER_H_PX);

interface Atlas {
  // `IconLayer.iconAtlas` only accepts a URL string or a luma.gl `Texture`
  // at the type level in deck.gl v9 (no raw canvas/ImageData/ImageBitmap,
  // unlike older docs) — a data URL is the simplest way to hand it a
  // canvas we drew ourselves without pulling in luma.gl's texture-creation
  // API directly. Built once and cached, so it's only encoded once.
  image: string;
  mapping: Record<string, { x: number; y: number; width: number; height: number; mask: boolean }>;
}

let atlasCache: Atlas | null = null;

function buildAtlas(): Atlas {
  if (atlasCache) return atlasCache;
  const c = document.createElement("canvas");
  c.width = ATLAS_W;
  c.height = ATLAS_H;
  const ctx = c.getContext("2d")!;
  ctx.fillStyle = "#fff";
  ctx.strokeStyle = "#fff";
  ctx.lineCap = "round";

  // WiFi-style badge: dot + two signal arcs (same shape as the old SDF one).
  const cx = BADGE_PX / 2;
  const cy = BADGE_PX * 0.62;
  ctx.beginPath();
  ctx.arc(cx, cy, BADGE_PX * 0.09, 0, Math.PI * 2);
  ctx.fill();
  ctx.lineWidth = BADGE_PX * 0.12;
  const up = 1.5 * Math.PI;
  const spread = 0.65;
  for (const r of [BADGE_PX * 0.22, BADGE_PX * 0.38]) {
    ctx.beginPath();
    ctx.arc(cx, cy, r, up - spread, up + spread);
    ctx.stroke();
  }

  // Backdrop circles for the badge — plain filled discs, packed after it.
  const ringX = BADGE_PX;
  ctx.beginPath();
  ctx.arc(ringX + BADGE_RING_PX / 2, BADGE_RING_PX / 2, BADGE_RING_PX / 2, 0, Math.PI * 2);
  ctx.fill();
  const fillX = ringX + BADGE_RING_PX;
  ctx.beginPath();
  ctx.arc(fillX + BADGE_FILL_PX / 2, BADGE_FILL_PX / 2, BADGE_FILL_PX / 2, 0, Math.PI * 2);
  ctx.fill();

  // Leader line: a horizontal stroke, packed after the circles.
  const leaderX = fillX + BADGE_FILL_PX;
  ctx.lineWidth = LEADER_H_PX * 0.6;
  ctx.beginPath();
  ctx.moveTo(leaderX + 2, LEADER_H_PX / 2);
  ctx.lineTo(leaderX + LEADER_W_PX - 2, LEADER_H_PX / 2);
  ctx.stroke();

  atlasCache = {
    image: c.toDataURL(),
    mapping: {
      badge: { x: 0, y: 0, width: BADGE_PX, height: BADGE_PX, mask: true },
      badgeRing: { x: ringX, y: 0, width: BADGE_RING_PX, height: BADGE_RING_PX, mask: true },
      badgeFill: { x: fillX, y: 0, width: BADGE_FILL_PX, height: BADGE_FILL_PX, mask: true },
      leader: { x: leaderX, y: 0, width: LEADER_W_PX, height: LEADER_H_PX, mask: true },
    },
  };
  return atlasCache;
}

function hexToRgb255(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
}

// --- shared text metrics -----------------------------------------------------
//
// Both the declutter pass and the badge's placement (it sits at the pill's
// *actual* top-right corner) need to know how wide a chip's text will
// render — measured once here and reused, rather than letting the
// background's own auto-fit and this module's idea of "how wide" drift
// apart (that mismatch was the direct cause of text bleeding past the pill:
// the two-line chip used to be two separate TextLayers, one background
// sized to *only* the callsign, with the — potentially longer — altitude
// line drawn by a second, backgroundless layer assumed to fit underneath
// it). It's now a single TextLayer with both lines as one string, so the
// background always auto-fits the actual longer line; this is what lets
// the badge agree with it on where the right edge actually is.

const TEXT_SIZE_PX = 11;
const CHIP_LEFT_PX = 30; // screen px right of the plane's anchor, to the pill's left edge
const CHIP_PAD_X = 10;
// Shifts the *visible* callsign/altitude text a few px closer to the pill's
// left edge than CHIP_PAD_X alone would — doesn't affect the (invisible)
// background-sizing layer, so the pill itself is unchanged, just the text
// sitting a little tighter inside it.
const TEXT_NUDGE_X = -4;
const CHIP_APPROX_H = 34; // two lines at TEXT_SIZE_PX plus padding — approximate, see computeVisible
// Used both to measure text width here *and* as every TextLayer's own
// `fontFamily` below — the badge's position depends on this module's own
// estimate of the pill's rendered width agreeing with deck.gl's actual
// glyph rendering closely enough; measuring against a plain "sans-serif"
// while rendering "Open Sans" was exactly what put the badge's estimated
// right edge to the left of the real one, so it landed on top of the text
// instead of past it.
const FONT_FAMILY = '"Open Sans", "Noto Sans", sans-serif';

let measureCtx: CanvasRenderingContext2D | null = null;
function textWidthPx(s: string): number {
  if (!s) return 0;
  if (!measureCtx) measureCtx = document.createElement("canvas").getContext("2d");
  if (!measureCtx) return s.length * TEXT_SIZE_PX * 0.6;
  measureCtx.font = `${TEXT_SIZE_PX}px ${FONT_FAMILY}`;
  return measureCtx.measureText(s).width;
}

function chipText(f: AircraftFeature): string {
  return f.properties.altLabel ? `${f.properties.callsign}\n${f.properties.altLabel}` : f.properties.callsign;
}

/** Pixel width of a chip's pill background, including padding — the
 *  longer of its two lines plus the same padding passed to `backgroundPadding`. */
function chipWidth(f: AircraftFeature): number {
  return (
    Math.max(textWidthPx(f.properties.callsign), textWidthPx(f.properties.altLabel)) +
    CHIP_PAD_X * 2
  );
}

// --- declutter ---------------------------------------------------------------
//
// A plain greedy "does this overlap a higher-priority chip already placed"
// pass — the same principle the old `symbol-sort-key`-based MapLibre
// collision used (higher-altitude aircraft wins), just run by hand instead
// of by the renderer, since that's what lets every layer below share one
// unambiguous answer. Approximate on purpose: it only needs to be "close
// enough" to avoid obviously-wrong overlaps, not pixel-perfect.

interface ChipRect {
  hex: string;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  priority: number;
}

function computeVisible(features: AircraftFeature[], map: MlMap): Set<string> {
  if (map.getZoom() < CHIP_MINZOOM) return new Set();
  const rects: ChipRect[] = [];
  for (const f of features) {
    let screen;
    try {
      screen = map.project(f.geometry.coordinates);
    } catch {
      continue; // off-world / not yet projectable
    }
    const x0 = screen.x + CHIP_LEFT_PX;
    const y0 = screen.y - CHIP_APPROX_H / 2;
    rects.push({
      hex: f.properties.hex,
      x0,
      y0,
      x1: x0 + chipWidth(f),
      y1: y0 + CHIP_APPROX_H,
      priority: f.properties.altBaro ?? -1,
    });
  }
  rects.sort((a, b) => b.priority - a.priority);
  const placed: ChipRect[] = [];
  const visible = new Set<string>();
  for (const r of rects) {
    const overlaps = placed.some((p) => r.x0 < p.x1 && r.x1 > p.x0 && r.y0 < p.y1 && r.y1 > p.y0);
    if (!overlaps) {
      placed.push(r);
      visible.add(r.hex);
    }
  }
  return visible;
}

// --- layers -------------------------------------------------------------

/** Fixed regardless of theme — a darker charcoal reads fine on both light
 *  and dark basemaps, and doing more visual work (once the sole separator
 *  from the map now that the chip has no drop shadow) is exactly why it's
 *  darker than the old theme-derived border. Used for the pill's own
 *  border and the leader line, so the two read as one cohesive shape. */
const CHIP_BORDER_RGB: [number, number, number] = [42, 46, 52];

export interface ChipLayerOpts {
  features: AircraftFeature[];
  map: MlMap;
  dark: boolean;
  chipBg: string;
  textColor: string;
}

export function buildChipLayers(opts: ChipLayerOpts): Layer[] {
  const { features, map, dark, chipBg, textColor } = opts;
  const visible = computeVisible(features, map);
  const shown = features.filter((f) => visible.has(f.properties.hex));
  const directShown = shown.filter((f) => f.properties.direct);
  const atlas = buildAtlas();

  const bgRgb = hexToRgb255(chipBg);
  const textRgb = hexToRgb255(textColor);
  const badgeRgb = hexToRgb255(FLIGHT_CATEGORY_COLORS.VFR);

  // Spans from just *under* the plane's own icon footprint to just under the
  // pill's rounded left corner, so the two overlap slightly at both ends —
  // `beforeId` inserts this into the MapLibre stack immediately before the
  // plane layer, so the plane paints over the near end (the line reads as
  // emerging from underneath it) while the pill still paints over the far
  // end (added later below, so it's on top by default).
  const leaderSpanStart = -8;
  const leaderSpanEnd = CHIP_LEFT_PX + 4;
  const leaderLayer = new IconLayer<AircraftFeature>({
    id: "chip-leader",
    data: shown,
    pickable: false,
    // `beforeId` isn't part of IconLayer's own declared props — it's a
    // `@deck.gl/mapbox`-specific insertion-point prop (`LayerOverlayProps`,
    // consumed by `MapboxOverlay` at runtime, not in the base Layer type),
    // hence the cast.
    ...({ beforeId: PLANE_LAYER_ID } as object),
    iconAtlas: atlas.image,
    iconMapping: atlas.mapping,
    getIcon: () => "leader",
    getPosition: (f) => f.geometry.coordinates,
    // `getSize` is a *pixel height* target, not an arbitrary scale factor —
    // IconLayer's default `sizeBasis: 'height'` computes
    // `renderedHeight = getSize`, `renderedWidth = getSize * (nativeWidth /
    // nativeHeight)`. Native width/height are set to the exact final pixel
    // size we want, so getSize is just that size — no scaling.
    getSize: LEADER_H_PX,
    sizeUnits: "pixels",
    getPixelOffset: [(leaderSpanStart + leaderSpanEnd) / 2, 0],
    getColor: [...CHIP_BORDER_RGB, 220],
  });

  // The background/border box is its own layer, sized from *both* lines
  // combined (via `chipText`) but with fully transparent text — this is
  // what fixed the text-bleed bug (a background only auto-fits its own
  // text, so a box sized for just the callsign bled once the altitude line
  // — often longer, e.g. "26,000 ft" vs "N5AZ" — was drawn separately
  // underneath it). The two lines are then drawn *visibly* by two further
  // backgroundless layers below, positioned to land inside this box, so
  // each can keep its own color (dropping that per the first attempt at
  // this fix was never intended to be permanent).
  const chipBgLayer = new TextLayer<AircraftFeature>({
    id: "chip-bg",
    data: shown,
    pickable: false,
    getPosition: (f) => f.geometry.coordinates,
    getText: chipText,
    getPixelOffset: [CHIP_LEFT_PX + CHIP_PAD_X, 1],
    getSize: TEXT_SIZE_PX,
    sizeUnits: "pixels",
    lineHeight: 1.3,
    getColor: [0, 0, 0, 0],
    getTextAnchor: "start",
    getAlignmentBaseline: "center",
    background: true,
    getBackgroundColor: [...bgRgb, 235],
    getBorderColor: [...CHIP_BORDER_RGB, 255],
    getBorderWidth: 2,
    backgroundPadding: [CHIP_PAD_X, 6],
    backgroundBorderRadius: 6,
    fontFamily: FONT_FAMILY,
  });

  const chipCallsignLayer = new TextLayer<AircraftFeature>({
    id: "chip-callsign",
    data: shown,
    pickable: false,
    getPosition: (f) => f.geometry.coordinates,
    getText: (f) => f.properties.callsign,
    getPixelOffset: [CHIP_LEFT_PX + CHIP_PAD_X + TEXT_NUDGE_X, -7],
    getSize: TEXT_SIZE_PX,
    sizeUnits: "pixels",
    getColor: [...textRgb, 255],
    getTextAnchor: "start",
    getAlignmentBaseline: "center",
    fontFamily: FONT_FAMILY,
  });

  // Colored by altitude band — dark chip: the same tint as the plane icon's
  // fill; light chip: a darkened variant legible as plain text (see
  // `altColorOnLight`).
  const chipAltLayer = new TextLayer<AircraftFeature>({
    id: "chip-alt",
    data: shown.filter((f) => f.properties.altLabel),
    pickable: false,
    getPosition: (f) => f.geometry.coordinates,
    getText: (f) => f.properties.altLabel,
    getPixelOffset: [CHIP_LEFT_PX + CHIP_PAD_X + TEXT_NUDGE_X, 9],
    getSize: TEXT_SIZE_PX,
    sizeUnits: "pixels",
    getColor: (f) => [
      ...hexToRgb255(dark ? f.properties.color : f.properties.altTextColorOnLight),
      255,
    ],
    getTextAnchor: "start",
    getAlignmentBaseline: "center",
    fontFamily: FONT_FAMILY,
    updateTriggers: {
      getColor: [dark],
    },
  });

  // Tucked into the pill's own top-right corner — which, since the pill's
  // width now depends on its text (see `chipBgLayer` above), is a different
  // screen position per aircraft. `getPixelOffset` is a per-feature
  // accessor here (not a constant), computed from the same `chipWidth` the
  // declutter pass already uses, so the badge always agrees with where the
  // pill's edge actually is instead of assuming a fixed width that doesn't
  // match a resized pill (that mismatch was the "badge on the wrong side"
  // bug — its offset was tuned for a fixed-width pill this layout doesn't
  // have). `chipWidth`'s own `textWidthPx` is a `measureText` estimate, not
  // deck.gl's real glyph metrics — it can undershoot slightly, and an
  // undershoot here means the badge lands *inside* the pill on top of the
  // text instead of past its edge (which is exactly what happened when the
  // estimate used a different font than the real one — see `FONT_FAMILY`).
  // Biasing a few px past the estimated edge is deliberate insurance
  // against a residual undershoot: the badge poking slightly outside the
  // corner reads fine, overlapping the text does not.
  // Shared by the badge and its two backdrop circles below, so all three
  // can't drift out of pixel-agreement with each other the way the badge
  // and the pill itself once did.
  const badgeOffset = (f: AircraftFeature): [number, number] => [
    CHIP_LEFT_PX + chipWidth(f) - BADGE_PX / 2 + 9 + 3,
    -13 - 3,
  ];

  const badgeRingLayer = new IconLayer<AircraftFeature>({
    id: "chip-badge-ring",
    data: directShown,
    pickable: false,
    iconAtlas: atlas.image,
    iconMapping: atlas.mapping,
    getIcon: () => "badgeRing",
    getPosition: (f) => f.geometry.coordinates,
    getPixelOffset: badgeOffset,
    getSize: BADGE_RING_PX,
    sizeUnits: "pixels",
    getColor: [...CHIP_BORDER_RGB, 255],
  });

  const badgeFillLayer = new IconLayer<AircraftFeature>({
    id: "chip-badge-fill",
    data: directShown,
    pickable: false,
    iconAtlas: atlas.image,
    iconMapping: atlas.mapping,
    getIcon: () => "badgeFill",
    getPosition: (f) => f.geometry.coordinates,
    getPixelOffset: badgeOffset,
    getSize: BADGE_FILL_PX,
    sizeUnits: "pixels",
    getColor: [...bgRgb, 255],
  });

  const badgeLayer = new IconLayer<AircraftFeature>({
    id: "chip-badge",
    data: directShown,
    pickable: false,
    iconAtlas: atlas.image,
    iconMapping: atlas.mapping,
    getIcon: () => "badge",
    getPosition: (f) => f.geometry.coordinates,
    getPixelOffset: badgeOffset,
    getSize: BADGE_PX,
    sizeUnits: "pixels",
    getColor: [...badgeRgb, 255],
  });

  // Paint order: the leader line (though `beforeId` actually controls its
  // place in the combined MapLibre+deck.gl stack), then the pill+text, then
  // the badge's backdrop (ring, then fill), then the badge itself on top.
  return [
    leaderLayer,
    chipBgLayer,
    chipCallsignLayer,
    chipAltLayer,
    badgeRingLayer,
    badgeFillLayer,
    badgeLayer,
  ];
}
