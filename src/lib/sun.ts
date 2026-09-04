// Solar position + a photographer's read on the light. Low-precision formulae
// from the Astronomical Almanac (elevation good to ~0.01°, azimuth to ~0.1°) —
// plenty for "will this pass be back-lit". No dependency, same as how the Lucide
// icon geometry is vendored rather than imported.

import { angleDelta } from "./geo";

const D2R = Math.PI / 180;
const R2D = 180 / Math.PI;

/** Apparent altitude/azimuth of the Sun for a place and instant. */
export function solarPosition(
  date: Date,
  latDeg: number,
  lonDeg: number,
): { azimuth: number; elevation: number } {
  // Days since the J2000.0 epoch (2000-01-01 12:00 UT).
  const n = date.getTime() / 86_400_000 + 2440587.5 - 2451545.0;

  let meanLon = (280.46 + 0.9856474 * n) % 360;
  if (meanLon < 0) meanLon += 360;
  const meanAnom = ((357.528 + 0.9856003 * n) % 360) * D2R;

  const eclipticLon =
    (meanLon +
      1.915 * Math.sin(meanAnom) +
      0.02 * Math.sin(2 * meanAnom)) *
    D2R;
  const obliquity = (23.439 - 0.0000004 * n) * D2R;

  const rightAsc = Math.atan2(
    Math.cos(obliquity) * Math.sin(eclipticLon),
    Math.cos(eclipticLon),
  );
  const decl = Math.asin(Math.sin(obliquity) * Math.sin(eclipticLon));

  // Greenwich mean sidereal time -> local hour angle.
  let gmstHours = (18.697374558 + 24.06570982441908 * n) % 24;
  if (gmstHours < 0) gmstHours += 24;
  const localSidereal = (gmstHours * 15 + lonDeg) * D2R;
  let hourAngle = localSidereal - rightAsc;
  hourAngle = ((hourAngle + Math.PI) % (2 * Math.PI)) - Math.PI;

  const lat = latDeg * D2R;
  const elevation = Math.asin(
    Math.sin(lat) * Math.sin(decl) +
      Math.cos(lat) * Math.cos(decl) * Math.cos(hourAngle),
  );
  // Azimuth measured from South, positive toward West -> shift to from-North.
  const azSouth = Math.atan2(
    Math.sin(hourAngle),
    Math.cos(hourAngle) * Math.sin(lat) - Math.tan(decl) * Math.cos(lat),
  );
  const azimuth = (azSouth * R2D + 180) % 360;

  return { azimuth, elevation: elevation * R2D };
}

export type LightPhase = "day" | "golden" | "blue" | "night";

/** Rough daylight phase from the Sun's elevation (degrees). */
export function lightPhase(sunElevationDeg: number): LightPhase {
  if (sunElevationDeg >= 6) return "day";
  if (sunElevationDeg >= -0.833) return "golden"; // disc up, low and warm
  if (sunElevationDeg >= -6) return "blue"; // civil twilight
  return "night";
}

export type LitSide = "front" | "side" | "back" | "n/a";

/**
 * Where the light falls on a subject seen at `targetBearing`, for someone
 * standing at the place looking toward it. "front" = sun behind the
 * photographer (subject well lit); "back" = shooting into the sun.
 */
export function litSide(
  sunAzimuth: number,
  targetBearing: number,
  sunElevationDeg: number,
): LitSide {
  if (sunElevationDeg < -0.833) return "n/a";
  const d = angleDelta(sunAzimuth, targetBearing);
  if (d < 60) return "back";
  if (d > 120) return "front";
  return "side";
}
