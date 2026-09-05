// Aircraft icons rendered to canvas at runtime as SDF images so MapLibre can
// tint them by altitude (`icon-color`) and outline them (`icon-halo-*`).
//
// Each path is drawn in a 64x64 box, nose pointing up (north / bearing 0),
// symmetric about x = 32.

import type { Map as MlMap } from "maplibre-gl";
import { alphaToSdf } from "./sdf";

export type IconKind =
  | "light"
  | "jet"
  | "heavy"
  | "heli"
  | "balloon"
  | "glider"
  | "ground"
  | "dot";

const PATHS: Record<IconKind, string> = {
  // Small GA aircraft — thin fuselage, straight mid-set wings, small tail.
  light:
    "M32 8 L34 27 L52 33 L52 37 L34 33 L34 48 L40 54 L40 57 L32 54 L24 57 L24 54 L30 48 L30 33 L12 37 L12 33 L30 27 Z",
  // Airliner / business jet — swept wings, longer body, swept tailplane.
  jet:
    "M32 5 L34 22 L58 42 L58 48 L34 37 L34 50 L43 59 L43 63 L32 57 L21 63 L21 59 L30 50 L30 37 L6 48 L6 42 L30 22 Z",
  // Wide-body / jumbo — broader wingspan, fatter fuselage, engine nacelle hints.
  heavy:
    "M32 3 L35 20 L45 27 L46 31 L44 31 L44 34 L62 45 L62 51 L44 43 L35 39 L35 52 L46 61 L46 64 L32 58 L18 64 L18 61 L29 52 L29 39 L20 43 L2 51 L2 45 L20 34 L20 31 L18 31 L19 27 L29 20 Z",
  // Helicopter — rotor disc (X of blades) over a compact body, tail boom + rotor.
  heli:
    "M12 30 L52 30 L52 34 L12 34 Z M30 12 L34 12 L34 52 L30 52 Z M24 24 L40 24 L40 40 L24 40 Z M31 40 L33 40 L33 55 L31 55 Z M25 53 L39 53 L39 57 L25 57 Z",
  // Airship / blimp — horizontal ellipse, tail fin, small gondola.
  balloon:
    "M32 18 C46 18 55 23 55 31 C55 39 46 44 32 44 C18 44 9 39 9 31 C9 23 18 18 32 18 Z M50 24 L60 20 L56 31 L60 42 L50 38 Z M28 43 L36 43 L36 48 L28 48 Z",
  // Glider — very long, thin, straight wings.
  glider:
    "M32 8 L33 26 L60 29 L60 32 L33 31 L33 49 L38 54 L38 57 L32 54 L26 57 L26 54 L31 49 L31 31 L4 32 L4 29 L31 26 Z",
  // Surface vehicle — rounded square.
  ground: "M22 22 L42 22 L42 42 L22 42 Z",
  // Unknown position source / no heading — plain dot.
  dot: "M21 32 A11 11 0 1 0 43 32 A11 11 0 1 0 21 32 Z",
};

const SIZE_MUL: Record<IconKind, number> = {
  light: 0.85,
  jet: 1.0,
  heavy: 1.28,
  heli: 0.95,
  balloon: 1.15,
  glider: 1.05,
  ground: 0.8,
  dot: 0.7,
};

export function sizeMulFor(kind: IconKind): number {
  return SIZE_MUL[kind] ?? 1;
}

// Paths are authored in a 64-unit box; render them into a larger canvas with a
// generous margin so the SDF — and thus the halo / outline / shadow blur — has
// room and isn't clipped at the edges.
const CANVAS = 96;
const MARGIN = 16;
const SDF_RADIUS = 16;
const SCALE = (CANVAS - 2 * MARGIN) / 64;

function shapeToSdf(path: string): ImageData {
  const c = document.createElement("canvas");
  c.width = CANVAS;
  c.height = CANVAS;
  const ctx = c.getContext("2d")!;
  ctx.translate(MARGIN, MARGIN);
  ctx.scale(SCALE, SCALE);
  ctx.fillStyle = "#fff";
  ctx.fill(new Path2D(path), "nonzero");

  const px = ctx.getImageData(0, 0, CANVAS, CANVAS).data;
  const alpha = new Uint8ClampedArray(CANVAS * CANVAS);
  for (let i = 0; i < alpha.length; i++) alpha[i] = px[i * 4 + 3];
  return alphaToSdf(alpha, CANVAS, CANVAS, SDF_RADIUS);
}

export function registerAircraftIcons(map: MlMap): void {
  for (const [name, path] of Object.entries(PATHS) as [IconKind, string][]) {
    const id = `ac-${name}`;
    if (map.hasImage(id)) continue;
    map.addImage(id, shapeToSdf(path), { sdf: true, pixelRatio: 2 });
  }
}

// The info chip (pill + callsign/altitude), its RTL-SDR wifi badge, and the
// leader line joining it to the plane used to be SDF images registered here
// too (`CHIP_PILL_ID`, `CHIP_LEADER_ID`, `RTLSDR_BADGE_ID`) — they're now
// rendered by a deck.gl overlay instead (see `aircraftChips.ts`), which
// builds its own small raster icon atlas rather than reusing this file's
// MapLibre-specific SDF pipeline. The DOM-side list/detail-panel UI uses its
// own separate Lucide-style `Icon` component for the wifi glyph, unrelated
// to either of these.

// --- classification -------------------------------------------------------

const HELI_TYPES = new Set([
  "R22", "R44", "R66", "B06", "B06T", "B105", "B47G", "B407", "B412", "B429",
  "B430", "B505", "EC20", "EC25", "EC30", "EC35", "EC45", "EC55", "EC75", "H500",
  "H60", "H64", "S76", "S76C", "S92", "S61", "S64", "S65C", "A109", "A119",
  "A139", "A169", "A189", "AS32", "AS3B", "AS50", "AS55", "AS65", "GAZL", "LYNX",
  "EH10", "NH90", "PUMA", "MI8", "MI17", "MI24", "MI26", "K126", "EN28", "H47",
  "CH47", "V22", "EXPL", "EXEC", "HUCO", "R900", "DYH2", "DYH3", "SCTA", "SW4",
]);

const HEAVY_TYPES = new Set([
  "A124", "A225", "A332", "A333", "A337", "A338", "A339", "A342", "A343", "A345",
  "A346", "A359", "A35K", "A388", "A3ST", "B742", "B743", "B744", "B748", "B74D",
  "B74R", "B74S", "B762", "B763", "B764", "B772", "B77L", "B773", "B77W", "B788",
  "B789", "B78X", "MD11", "DC10", "IL76", "IL96", "A306", "A30B", "A310", "C5M",
  "C17", "KC10", "B52", "AN22", "AN12", "A400",
]);

const GLIDER_TYPES = new Set([
  "GLID", "DG40", "DG80", "DG1T", "DISC", "DUOD", "LS4", "LS6", "LS8", "LS10",
  "ARCE", "VENT", "JS1", "JS3", "NIMB", "STDC", "SF25", "SZ45", "PK15", "ASK21",
  "AS21", "AS22", "AS25", "AS26", "AS28", "AS29", "AS31", "TWIN",
]);

const BALLOON_TYPES = new Set([
  "SHIP", "BALL", "Z07T", "Z100", "GEPD", "THUN", "UHBP",
]);

/** Light-GA type-code prefixes (used only when there's no emitter category). */
function looksLight(t: string): boolean {
  return (
    /^C1[0-9]{2}$/.test(t) || // C150..C182 etc
    /^C2[0-9]{2}$/.test(t) ||
    /^C3[0-9]{2}$/.test(t) ||
    /^C4[0-9]{2}$/.test(t) ||
    /^PA[0-9]{2}$/.test(t) ||
    /^P2[0-9]{2}$/.test(t) ||
    /^P3[0-9]{2}$/.test(t) ||
    /^SR2[0-9]/.test(t) ||
    /^DA[0-9]{2}$/.test(t) ||
    /^BE[0-9]/.test(t) ||
    /^M20[A-Z]$/.test(t) ||
    /^RV[0-9]/.test(t) ||
    /^AA[0-9]/.test(t) ||
    /^GLAS|COL[0-9]|LNC|TBM[0-9]|PC12|EPIC|VELO|EXP[0-9]/.test(t)
  );
}

/**
 * Pick an icon for an aircraft. `category` is the ADS-B emitter category
 * (A1..A7, B1..B7, C1..C3); `typeCode` is the ICAO type designator.
 */
export function iconKindFor(
  category: string | null,
  typeCode: string | null,
  onGround: boolean,
  hasHeading: boolean,
): IconKind {
  if (!hasHeading && !onGround) return "dot";

  const t = (typeCode ?? "").toUpperCase();

  if (t && HELI_TYPES.has(t)) return "heli";
  if (t && HEAVY_TYPES.has(t)) return "heavy";
  if (t && GLIDER_TYPES.has(t)) return "glider";
  if (t && BALLOON_TYPES.has(t)) return "balloon";

  switch (category) {
    case "A7":
      return "heli";
    case "A5": // Heavy (> 300 000 lb)
      return "heavy";
    case "A4": // High-vortex large (B757)
    case "A3": // Large
    case "A2": // Small (regional / bizjet)
    case "A6": // High performance
      return "jet";
    case "A1": // Light
      return "light";
    case "B1": // Glider / sailplane
      return "glider";
    case "B2": // Lighter-than-air
      return "balloon";
    case "B4": // Ultralight
      return "light";
    case "B6": // UAV
      return "dot";
    case "C1":
    case "C2":
    case "C3":
      return "ground";
  }

  if (onGround && !category) return t && looksLight(t) ? "light" : "ground";
  if (t && looksLight(t)) return "light";
  return t ? "jet" : "light";
}
