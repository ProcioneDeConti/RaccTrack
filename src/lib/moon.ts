// Low-precision topocentric Moon position (Paul Schlyter's method, with the
// main perturbation terms) and a rough illuminated fraction. Accurate to well
// under a degree — plenty for a "which way is the Moon" HUD. No dependency,
// mirrors sun.ts.

import type { SkyPosition } from "./sun";

const D2R = Math.PI / 180;
const R2D = 180 / Math.PI;

const rev = (x: number) => x - Math.floor(x / 360) * 360;
const sind = (d: number) => Math.sin(d * D2R);
const cosd = (d: number) => Math.cos(d * D2R);

/** Apparent altitude/azimuth of the Moon for a place and instant. */
export function moonPosition(
  date: Date,
  latDeg: number,
  lonDeg: number,
): SkyPosition {
  // Schlyter's day number: days since 1999-12-31 00:00 UT.
  const d =
    date.getTime() / 86_400_000 + 2440587.5 - 2451543.5;
  const utHours = ((date.getTime() / 3_600_000) % 24 + 24) % 24;
  const ecl = 23.4393 - 3.563e-7 * d;

  // Moon's orbital elements.
  const N = rev(125.1228 - 0.0529538083 * d);
  const i = 5.1454;
  const w = rev(318.0634 + 0.1643573223 * d);
  const a = 60.2666; // Earth radii
  const e = 0.054900;
  const M = rev(115.3654 + 13.0649929509 * d);

  // Eccentric anomaly (iterate once — eccentricity is small).
  let E = M + R2D * e * sind(M) * (1 + e * cosd(M));
  for (let k = 0; k < 5; k++) {
    E = E - (E - R2D * e * sind(E) - M) / (1 - e * cosd(E));
  }

  // Position in the orbital plane -> distance and true anomaly.
  const xv = a * (cosd(E) - e);
  const yv = a * Math.sqrt(1 - e * e) * sind(E);
  const r = Math.hypot(xv, yv);
  const v = rev(R2D * Math.atan2(yv, xv));

  // Geocentric ecliptic rectangular coordinates.
  let xh =
    r *
    (cosd(N) * cosd(v + w) - sind(N) * sind(v + w) * cosd(i));
  let yh =
    r *
    (sind(N) * cosd(v + w) + cosd(N) * sind(v + w) * cosd(i));
  let zh = r * sind(v + w) * sind(i);

  let lon = rev(R2D * Math.atan2(yh, xh));
  let lat = R2D * Math.atan2(zh, Math.hypot(xh, yh));

  // Perturbations — need the Sun's and Moon's mean longitudes.
  const ws = 282.9404 + 4.70935e-5 * d;
  const Ms = rev(356.047 + 0.9856002585 * d);
  const Ls = rev(ws + Ms);
  const Lm = rev(N + w + M);
  const Mm = M;
  const Dm = rev(Lm - Ls); // mean elongation
  const F = rev(Lm - N); // argument of latitude

  lon +=
    -1.274 * sind(Mm - 2 * Dm) +
    0.658 * sind(2 * Dm) +
    -0.186 * sind(Ms) +
    -0.059 * sind(2 * Mm - 2 * Dm) +
    -0.057 * sind(Mm - 2 * Dm + Ms) +
    0.053 * sind(Mm + 2 * Dm) +
    0.046 * sind(2 * Dm - Ms) +
    0.041 * sind(Mm - Ms) +
    -0.035 * sind(Dm) +
    -0.031 * sind(Mm + Ms) +
    -0.015 * sind(2 * F - 2 * Dm) +
    0.011 * sind(Mm - 4 * Dm);
  lat +=
    -0.173 * sind(F - 2 * Dm) +
    -0.055 * sind(Mm - F - 2 * Dm) +
    -0.046 * sind(Mm + F - 2 * Dm) +
    0.033 * sind(F + 2 * Dm) +
    0.017 * sind(2 * Mm + F);

  // Ecliptic -> equatorial (geocentric).
  const xg = r * cosd(lon) * cosd(lat);
  const yg = r * sind(lon) * cosd(lat);
  const zg = r * sind(lat);
  const xe = xg;
  const ye = yg * cosd(ecl) - zg * sind(ecl);
  const ze = yg * sind(ecl) + zg * cosd(ecl);

  const ra = rev(R2D * Math.atan2(ye, xe));
  const dec = R2D * Math.atan2(ze, Math.hypot(xe, ye));

  // Local sidereal time -> hour angle.
  const gmst0 = rev(Ls + 180) / 15; // hours
  const lst = gmst0 + utHours + lonDeg / 15;
  const ha = rev(lst * 15 - ra);

  // Hour angle + declination -> altitude/azimuth.
  const x = cosd(ha) * cosd(dec);
  const y = sind(ha) * cosd(dec);
  const z = sind(dec);
  const xhor = x * sind(latDeg) - z * cosd(latDeg);
  const yhor = y;
  const zhor = x * cosd(latDeg) + z * sind(latDeg);

  const azimuth = rev(R2D * Math.atan2(yhor, xhor) + 180);
  let elevation = R2D * Math.asin(zhor);
  // Topocentric parallax correction (~1°).
  elevation -= R2D * Math.asin(1 / r) * Math.cos(elevation * D2R);

  return { azimuth, elevation };
}

/** Angular separation between two sky positions, degrees. */
export function angularSeparation(a: SkyPosition, b: SkyPosition): number {
  const av = unit(a);
  const bv = unit(b);
  const dot = av[0] * bv[0] + av[1] * bv[1] + av[2] * bv[2];
  return R2D * Math.acos(Math.max(-1, Math.min(1, dot)));
}

function unit(p: SkyPosition): [number, number, number] {
  const el = p.elevation * D2R;
  const az = p.azimuth * D2R;
  return [Math.cos(el) * Math.cos(az), Math.cos(el) * Math.sin(az), Math.sin(el)];
}

/**
 * Rough illuminated fraction of the Moon's disc (0 = new, 1 = full), from the
 * Sun–Moon elongation as seen by the observer.
 */
export function illuminatedFraction(sun: SkyPosition, moon: SkyPosition): number {
  const elongation = angularSeparation(sun, moon) * D2R;
  return (1 - Math.cos(elongation)) / 2;
}
