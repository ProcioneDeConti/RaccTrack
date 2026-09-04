// Pass predictions: which tracked aircraft will fly near a place soon, when,
// how close, and in what light. Straight-line constant-velocity projection in a
// local tangent plane — great-circle curvature over the ~12 min / ~120 nm
// horizon is under a nautical mile, negligible for "point your camera there".

import { readable, writable } from "svelte/store";
import type { Aircraft } from "./api/types";
import { bearing, distanceNm, enuOffsetNm } from "./geo";
import {
  lightPhase,
  litSide,
  solarPosition,
  type LightPhase,
  type LitSide,
} from "./sun";

const FT_PER_NM = 6076.12;
/** Ignore anything slower than this — parked, taxiing, or a bad vector. */
const MIN_GROUND_SPEED_KT = 30;

/** How far ahead to project, minutes. Predictions decay fast past a few min. */
export const passHorizonMin = writable(12);
/** Only list passes whose closest approach is within this many nm. */
export const passRadiusNm = writable(15);

/** A ticking clock, live only while something subscribes (e.g. the panel). */
export const passClock = readable(Date.now(), (set) => {
  const id = setInterval(() => set(Date.now()), 15_000);
  return () => clearInterval(id);
});

export interface PredictedPass {
  hex: string;
  callsign: string;
  typeCode: string | null;
  /** Epoch ms of closest approach. */
  etaMs: number;
  /** Seconds until closest approach. */
  inSec: number;
  minDistanceNm: number;
  /** Compass bearing from the place to the aircraft at closest approach. */
  bearingDeg: number;
  /** Viewing angle above the horizon at closest approach, degrees. */
  elevationDeg: number;
  altBaroFt: number | null;
  military: boolean;
  emergency: boolean;
  light: { phase: LightPhase; sunElevationDeg: number; lit: LitSide };
}

function heading(ac: Aircraft): number | null {
  return ac.track ?? ac.trueHeading ?? ac.magHeading;
}

function viewingElevation(altFt: number | null, horizNm: number): number {
  if (altFt == null) return 0;
  if (horizNm <= 0) return 90;
  return (Math.atan2(altFt / FT_PER_NM, horizNm) * 180) / Math.PI;
}

/**
 * Predict this aircraft's closest approach to `place`, or null when it isn't a
 * usable upcoming pass (on the ground, too slow, no heading, already past its
 * closest point, beyond the time horizon, or outside the radius).
 */
export function predictPass(
  ac: Aircraft,
  place: { lat: number; lon: number },
  now: number,
  horizonMin: number,
  radiusNm: number,
): PredictedPass | null {
  if (ac.lat == null || ac.lon == null || ac.onGround) return null;
  const gs = ac.groundSpeed;
  const hdg = heading(ac);
  if (gs == null || gs < MIN_GROUND_SPEED_KT || hdg == null) return null;

  const { e, n } = enuOffsetNm(place.lat, place.lon, ac.lat, ac.lon);
  const speed = gs / 60; // nm per minute
  const vE = speed * Math.sin((hdg * Math.PI) / 180);
  const vN = speed * Math.cos((hdg * Math.PI) / 180);
  const v2 = vE * vE + vN * vN;
  if (v2 === 0) return null;

  // Time of closest approach of a point moving linearly toward the origin.
  const tMin = -(e * vE + n * vN) / v2;
  if (tMin <= 0 || tMin > horizonMin) return null;

  const cE = e + vE * tMin;
  const cN = n + vN * tMin;
  const horiz = Math.hypot(cE, cN);
  if (horiz > radiusNm) return null;

  const etaMs = now + tMin * 60_000;
  const bearingDeg = (Math.atan2(cE, cN) * 180) / Math.PI;
  const brg = (bearingDeg + 360) % 360;
  const sun = solarPosition(new Date(etaMs), place.lat, place.lon);

  return {
    hex: ac.hex,
    callsign: (ac.flight ?? ac.registration ?? ac.hex).trim(),
    typeCode: ac.typeCode,
    etaMs,
    inSec: tMin * 60,
    minDistanceNm: horiz,
    bearingDeg: brg,
    elevationDeg: viewingElevation(ac.altBaro, horiz),
    altBaroFt: ac.altBaro,
    military: ac.military,
    emergency: !!ac.emergency && ac.emergency !== "none",
    light: {
      phase: lightPhase(sun.elevation),
      sunElevationDeg: sun.elevation,
      lit: litSide(sun.azimuth, brg, sun.elevation),
    },
  };
}

export interface ViewAngle {
  bearingDeg: number;
  distanceNm: number;
  elevationDeg: number;
  /** True when the aircraft is currently closing on the place. */
  closing: boolean;
}

/** Live pointing info from a place to one aircraft, right now. */
export function viewFromPlace(
  ac: Aircraft,
  place: { lat: number; lon: number },
): ViewAngle | null {
  if (ac.lat == null || ac.lon == null) return null;
  const d = distanceNm(place.lat, place.lon, ac.lat, ac.lon);
  const brg = bearing(place.lat, place.lon, ac.lat, ac.lon);

  let closing = false;
  const gs = ac.groundSpeed;
  const hdg = heading(ac);
  if (gs != null && hdg != null) {
    const { e, n } = enuOffsetNm(place.lat, place.lon, ac.lat, ac.lon);
    const vE = Math.sin((hdg * Math.PI) / 180);
    const vN = Math.cos((hdg * Math.PI) / 180);
    closing = e * vE + n * vN < 0; // radial velocity component points inward
  }

  return {
    bearingDeg: brg,
    distanceNm: d,
    elevationDeg: viewingElevation(ac.altBaro, d),
    closing,
  };
}
