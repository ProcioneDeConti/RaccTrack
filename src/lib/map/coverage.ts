// Hazard-tape outline of the ADS-B coverage region, drawn as a real map layer
// so it sits on the geographic boundary (the North America box we clip all
// queries to) rather than the window edge.

import type { Map as MlMap } from "maplibre-gl";
import { NA_BOUNDS } from "./region";
import { CAUTION_GOLD, CAUTION_GREY } from "../theme/colors";

const STRIPE = 16;

/** Diagonal gold/grey stripe tile for `line-pattern`. */
function stripeImage(): ImageData {
  const c = document.createElement("canvas");
  c.width = STRIPE;
  c.height = STRIPE;
  const ctx = c.getContext("2d")!;
  ctx.fillStyle = CAUTION_GOLD;
  ctx.fillRect(0, 0, STRIPE, STRIPE);
  ctx.strokeStyle = CAUTION_GREY;
  ctx.lineWidth = STRIPE / 2;
  ctx.beginPath();
  ctx.moveTo(-STRIPE, STRIPE);
  ctx.lineTo(STRIPE, -STRIPE);
  ctx.moveTo(0, STRIPE * 2);
  ctx.lineTo(STRIPE * 2, 0);
  ctx.stroke();
  return ctx.getImageData(0, 0, STRIPE, STRIPE);
}

/** Rectangle ring, densified so constant-latitude edges curve correctly. */
function coverageRing(): [number, number][] {
  const { west, south, east, north } = NA_BOUNDS;
  const step = 2;
  const ring: [number, number][] = [];
  for (let lon = west; lon < east; lon += step) ring.push([lon, south]);
  for (let lat = south; lat < north; lat += step) ring.push([east, lat]);
  for (let lon = east; lon > west; lon -= step) ring.push([lon, north]);
  for (let lat = north; lat > south; lat -= step) ring.push([west, lat]);
  ring.push([west, south]);
  return ring;
}

export function addCoverageBoundary(map: MlMap): void {
  if (!map.hasImage("caution-stripe")) {
    map.addImage("caution-stripe", stripeImage(), { pixelRatio: 2 });
  }

  if (!map.getSource("coverage-bounds")) {
    map.addSource("coverage-bounds", {
      type: "geojson",
      data: {
        type: "Feature",
        properties: {},
        geometry: { type: "LineString", coordinates: coverageRing() },
      },
    });
  }

  if (!map.getLayer("coverage-bounds-line")) {
    map.addLayer({
      id: "coverage-bounds-line",
      type: "line",
      source: "coverage-bounds",
      layout: { "line-cap": "butt", "line-join": "round" },
      paint: {
        "line-pattern": "caution-stripe",
        "line-width": ["interpolate", ["linear"], ["zoom"], 2, 6, 6, 14],
      },
    });
  }
}
