// Chart-style navaid symbols drawn to canvas at runtime and registered as
// plain (non-SDF) raster images, so the navaid overlay's `symbol` layer can
// pick one per feature with an `icon-image` match on the navaid kind.
//
// Deliberately not SDF (unlike the aircraft icons in icons.ts) — these aren't
// tinted or haloed per-feature; each is baked with its own colour plus a dark
// casing so it reads on both light and dark basemaps.

import type { Map as MlMap } from "maplibre-gl";

export type NavaidIcon = "vor" | "vordme" | "vortac" | "ndb" | "dme";

/** Map a navaid `kind` (from the backend) to one of the drawn icons. */
export function navaidIconFor(kind: string): NavaidIcon {
  switch (kind) {
    case "VOR":
      return "vor";
    case "VOR-DME":
      return "vordme";
    case "VORTAC":
    case "TACAN":
      return "vortac";
    case "NDB":
    case "NDB-DME":
      return "ndb";
    default:
      return "dme";
  }
}

const VOR_COLOR = "#4c9be8";
const NDB_COLOR = "#c77dff";
const CASING = "#0b1220";

const S = 40; // canvas px; registered at pixelRatio 2 → crisp near ~20px
const C = S / 2;

function ctx2d(): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const cv = document.createElement("canvas");
  cv.width = S;
  cv.height = S;
  const ctx = cv.getContext("2d")!;
  ctx.lineJoin = "round";
  ctx.lineCap = "round";
  return [cv, ctx];
}

function hexagonPath(ctx: CanvasRenderingContext2D, r: number) {
  ctx.beginPath();
  for (let i = 0; i < 6; i++) {
    const a = (Math.PI / 180) * (60 * i - 90);
    const x = C + r * Math.cos(a);
    const y = C + r * Math.sin(a);
    i ? ctx.lineTo(x, y) : ctx.moveTo(x, y);
  }
  ctx.closePath();
}

/** Casing pass then colour pass, so the shape has a dark outline on any basemap. */
function strokeTwice(ctx: CanvasRenderingContext2D, color: string) {
  ctx.strokeStyle = CASING;
  ctx.lineWidth = 4.5;
  ctx.stroke();
  ctx.strokeStyle = color;
  ctx.lineWidth = 2.4;
  ctx.stroke();
}

function centerDot(ctx: CanvasRenderingContext2D) {
  ctx.beginPath();
  ctx.arc(C, C, 2, 0, Math.PI * 2);
  ctx.fillStyle = CASING;
  ctx.fill();
}

function drawVor(ctx: CanvasRenderingContext2D) {
  hexagonPath(ctx, 11);
  strokeTwice(ctx, VOR_COLOR);
  centerDot(ctx);
}

function drawVorDme(ctx: CanvasRenderingContext2D) {
  // Hexagon inside a square outline (the sectional VOR-DME depiction).
  ctx.beginPath();
  ctx.rect(C - 13, C - 13, 26, 26);
  strokeTwice(ctx, VOR_COLOR);
  hexagonPath(ctx, 9);
  strokeTwice(ctx, VOR_COLOR);
  centerDot(ctx);
}

function drawVortac(ctx: CanvasRenderingContext2D) {
  // Hexagon with three filled TACAN lobes on alternating faces.
  hexagonPath(ctx, 11);
  strokeTwice(ctx, VOR_COLOR);
  ctx.fillStyle = VOR_COLOR;
  for (const k of [0, 2, 4]) {
    const a = (Math.PI / 180) * (60 * k - 60);
    const bx = C + 11 * Math.cos(a);
    const by = C + 11 * Math.sin(a);
    ctx.save();
    ctx.translate(bx, by);
    ctx.rotate(a);
    ctx.beginPath();
    ctx.moveTo(-4, 0);
    ctx.lineTo(4, 0);
    ctx.lineTo(2.5, 5);
    ctx.lineTo(-2.5, 5);
    ctx.closePath();
    ctx.fill();
    ctx.restore();
  }
  centerDot(ctx);
}

function drawDme(ctx: CanvasRenderingContext2D) {
  ctx.beginPath();
  ctx.rect(C - 10, C - 10, 20, 20);
  strokeTwice(ctx, VOR_COLOR);
  centerDot(ctx);
}

function drawNdb(ctx: CanvasRenderingContext2D) {
  // Filled dot ringed by a dashed circle (the sectional "dot pattern" NDB).
  ctx.setLineDash([1.5, 2.5]);
  ctx.beginPath();
  ctx.arc(C, C, 11, 0, Math.PI * 2);
  strokeTwice(ctx, NDB_COLOR);
  ctx.setLineDash([]);
  ctx.beginPath();
  ctx.arc(C, C, 3.4, 0, Math.PI * 2);
  ctx.fillStyle = NDB_COLOR;
  ctx.fill();
  ctx.lineWidth = 1;
  ctx.strokeStyle = CASING;
  ctx.stroke();
}

const DRAW: Record<NavaidIcon, (ctx: CanvasRenderingContext2D) => void> = {
  vor: drawVor,
  vordme: drawVorDme,
  vortac: drawVortac,
  dme: drawDme,
  ndb: drawNdb,
};

export function registerNavaidIcons(map: MlMap): void {
  for (const [name, draw] of Object.entries(DRAW) as [
    NavaidIcon,
    (ctx: CanvasRenderingContext2D) => void,
  ][]) {
    const id = `nav-${name}`;
    if (map.hasImage(id)) continue;
    const [, ctx] = ctx2d();
    draw(ctx);
    map.addImage(id, ctx.getImageData(0, 0, S, S), { pixelRatio: 2 });
  }
}
