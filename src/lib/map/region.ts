// North America coverage region. Kept in sync with the Rust mirror in
// `src-tauri/src/region.rs` — change both together.
//
// Covers Alaska, Canada, CONUS, Mexico, Central America and the Caribbean.
// ADS-B data outside this box is not reliably available from the free
// community feeds, so the map is hard-locked to it and polling never
// requests coordinates beyond it.

export const NA_BOUNDS = {
  west: -172,
  south: 7,
  east: -52,
  north: 72,
} as const;

// MapLibre LngLatBoundsLike: [[west, south], [east, north]]
export const NA_MAX_BOUNDS: [[number, number], [number, number]] = [
  [NA_BOUNDS.west, NA_BOUNDS.south],
  [NA_BOUNDS.east, NA_BOUNDS.north],
];

// Opening view: roughly centered on the continental US.
export const INITIAL_CENTER: [number, number] = [-96, 39];
export const INITIAL_ZOOM = 3.4;
export const MIN_ZOOM = 2.6;
export const MAX_ZOOM = 15;

export interface Bbox {
  west: number;
  south: number;
  east: number;
  north: number;
}

/** Clip an arbitrary bbox to the North America region. Returns null if disjoint. */
export function clipToRegion(b: Bbox): Bbox | null {
  const west = Math.max(b.west, NA_BOUNDS.west);
  const south = Math.max(b.south, NA_BOUNDS.south);
  const east = Math.min(b.east, NA_BOUNDS.east);
  const north = Math.min(b.north, NA_BOUNDS.north);
  if (west >= east || south >= north) return null;
  return { west, south, east, north };
}
