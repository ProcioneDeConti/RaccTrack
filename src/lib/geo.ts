// Small geodesy helpers (nautical miles, degrees).

const R_NM = 3440.065; // Earth radius in nautical miles
const D2R = Math.PI / 180;

/** Great-circle distance in nautical miles. */
export function distanceNm(
  lat1: number,
  lon1: number,
  lat2: number,
  lon2: number,
): number {
  const dLat = (lat2 - lat1) * D2R;
  const dLon = (lon2 - lon1) * D2R;
  const a =
    Math.sin(dLat / 2) ** 2 +
    Math.cos(lat1 * D2R) * Math.cos(lat2 * D2R) * Math.sin(dLon / 2) ** 2;
  return 2 * R_NM * Math.asin(Math.min(1, Math.sqrt(a)));
}

/** Initial bearing (degrees, 0–360) from point 1 to point 2. */
export function bearing(
  lat1: number,
  lon1: number,
  lat2: number,
  lon2: number,
): number {
  const y = Math.sin((lon2 - lon1) * D2R) * Math.cos(lat2 * D2R);
  const x =
    Math.cos(lat1 * D2R) * Math.sin(lat2 * D2R) -
    Math.sin(lat1 * D2R) * Math.cos(lat2 * D2R) * Math.cos((lon2 - lon1) * D2R);
  return (Math.atan2(y, x) / D2R + 360) % 360;
}

/**
 * Local east/north offset (nautical miles) of a point from a reference,
 * equirectangular approximation — good to well under 1 nm over the ~150 nm
 * ranges the pass predictor works at, and lets motion be treated as linear.
 */
export function enuOffsetNm(
  latRef: number,
  lonRef: number,
  lat: number,
  lon: number,
): { e: number; n: number } {
  return {
    e: (lon - lonRef) * Math.cos(latRef * D2R) * 60,
    n: (lat - latRef) * 60,
  };
}

/** Smallest absolute difference between two bearings, 0–180 degrees. */
export function angleDelta(a: number, b: number): number {
  return Math.abs(((a - b + 540) % 360) - 180);
}

/** Compass point for a bearing, e.g. 200 -> "SSW". */
export function compass(deg: number): string {
  const pts = [
    "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE",
    "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW",
  ];
  return pts[Math.round(deg / 22.5) % 16];
}

export function fmtDistanceNm(nm: number): string {
  return nm < 10 ? `${nm.toFixed(1)} nm` : `${Math.round(nm)} nm`;
}

/**
 * Project point P onto the great circle from A to B.
 *  - `along`: signed distance (nm) from A to P's foot on the A→B track.
 *    Negative when P is "behind" A; can exceed |A→B| when P is past B.
 *  - `cross`: absolute perpendicular offset (nm) of P from the track.
 */
export function projectOntoTrack(
  latA: number,
  lonA: number,
  latB: number,
  lonB: number,
  latP: number,
  lonP: number,
): { along: number; cross: number } {
  const d13 = distanceNm(latA, lonA, latP, lonP) / R_NM; // angular
  if (d13 === 0) return { along: 0, cross: 0 };
  const t13 = bearing(latA, lonA, latP, lonP) * D2R;
  const t12 = bearing(latA, lonA, latB, lonB) * D2R;
  const dxt = Math.asin(
    Math.max(-1, Math.min(1, Math.sin(d13) * Math.sin(t13 - t12))),
  );
  const dat = Math.acos(
    Math.max(-1, Math.min(1, Math.cos(d13) / Math.cos(dxt))),
  );
  const sign = Math.cos(t13 - t12) < 0 ? -1 : 1;
  return { along: sign * dat * R_NM, cross: Math.abs(dxt) * R_NM };
}

/** Human duration from decimal hours, e.g. 1.2 -> "1h 12m". */
export function fmtDuration(hours: number): string {
  if (!isFinite(hours) || hours < 0) return "—";
  const total = Math.round(hours * 60);
  const h = Math.floor(total / 60);
  const m = total % 60;
  return h > 0 ? `${h}h ${m.toString().padStart(2, "0")}m` : `${m}m`;
}

/**
 * Point at fraction `f` (0–1) along the great circle from 1 to 2.
 * Returns `[lon, lat]` (GeoJSON order).
 */
export function gcInterpolate(
  lat1: number,
  lon1: number,
  lat2: number,
  lon2: number,
  f: number,
): [number, number] {
  const φ1 = lat1 * D2R,
    λ1 = lon1 * D2R,
    φ2 = lat2 * D2R,
    λ2 = lon2 * D2R;
  const dφ = φ2 - φ1,
    dλ = λ2 - λ1;
  const a =
    Math.sin(dφ / 2) ** 2 +
    Math.cos(φ1) * Math.cos(φ2) * Math.sin(dλ / 2) ** 2;
  const δ = 2 * Math.asin(Math.min(1, Math.sqrt(a)));
  if (δ === 0) return [lon1, lat1];
  const A = Math.sin((1 - f) * δ) / Math.sin(δ);
  const B = Math.sin(f * δ) / Math.sin(δ);
  const x =
    A * Math.cos(φ1) * Math.cos(λ1) + B * Math.cos(φ2) * Math.cos(λ2);
  const y =
    A * Math.cos(φ1) * Math.sin(λ1) + B * Math.cos(φ2) * Math.sin(λ2);
  const z = A * Math.sin(φ1) + B * Math.sin(φ2);
  const φ = Math.atan2(z, Math.sqrt(x * x + y * y));
  const λ = Math.atan2(y, x);
  return [λ / D2R, φ / D2R];
}

/** A great-circle polyline `[lon, lat][]` from 1 to 2 with `steps` segments. */
export function gcPath(
  lat1: number,
  lon1: number,
  lat2: number,
  lon2: number,
  steps = 64,
): [number, number][] {
  const out: [number, number][] = [];
  for (let i = 0; i <= steps; i++) {
    out.push(gcInterpolate(lat1, lon1, lat2, lon2, i / steps));
  }
  return out;
}
