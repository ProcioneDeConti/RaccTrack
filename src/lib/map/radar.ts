// Weather radar (NEXRAD + global mosaic) from RainViewer's free, key-less API.
// https://www.rainviewer.com/api.html

const MAPS_URL = "https://api.rainviewer.com/public/weather-maps.json";

interface Frame {
  time: number;
  path: string;
}

export interface RadarSnapshot {
  /** Tile URL template for MapLibre (`{z}/{x}/{y}`). */
  tileUrl: string;
  /** Frame time, epoch seconds. */
  time: number;
}

/** Fetch the latest available radar frame. Returns null on any failure — radar
 *  is a nice-to-have overlay, not load-bearing. */
export async function latestRadar(): Promise<RadarSnapshot | null> {
  try {
    const r = await fetch(MAPS_URL, { cache: "no-cache" });
    if (!r.ok) return null;
    const j = (await r.json()) as {
      host: string;
      radar?: { past?: Frame[]; nowcast?: Frame[] };
    };
    const past = j.radar?.past ?? [];
    const frame = past[past.length - 1];
    if (!j.host || !frame) return null;
    // 256px tiles · colour scheme 4 (Weather-Channel style) · smooth, no snow
    return {
      tileUrl: `${j.host}${frame.path}/256/{z}/{x}/{y}/4/1_0.png`,
      time: frame.time,
    };
  } catch {
    return null;
  }
}
