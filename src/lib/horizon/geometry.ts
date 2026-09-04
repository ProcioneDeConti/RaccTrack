// Pure screen-mapping math for the horizon view. Kept free of store/Tauri
// imports so it's unit-testable in isolation.

/** Degrees of azimuth visible in the window at once. */
export const HORIZON_FOV = 120;

export const wrap360 = (deg: number) => ((deg % 360) + 360) % 360;

/** Signed shortest delta `a - b`, in (-180, 180]. */
export function bearingDelta(a: number, b: number): number {
  return ((a - b + 540) % 360) - 180;
}

/** Compress elevation 0–90° into a 0–1 fraction — low angles get more room. */
export function elevationToFrac(elevDeg: number): number {
  const e = Math.max(0, Math.min(90, elevDeg));
  return Math.pow(e / 90, 0.55);
}

/**
 * Screen x for a bearing given the window centre and pixel width, or null when
 * it falls outside the visible field of view.
 */
export function bearingToX(
  bearingDeg: number,
  centerDeg: number,
  width: number,
  fov: number = HORIZON_FOV,
): number | null {
  const delta = bearingDelta(bearingDeg, centerDeg);
  if (Math.abs(delta) > fov / 2 + 3) return null;
  return width / 2 + (delta / fov) * width;
}
