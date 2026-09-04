// Windowed horizon view: aircraft and sky bodies plotted as azimuth × elevation
// from the primary place, for pointing a camera or your eyes. Frontend-only —
// projection reuses viewFromPlace() from passes.ts; the window can be panned.

import { derived, writable } from "svelte/store";
import type { Aircraft } from "./api/types";
import { aircraft, primaryPlace, filters } from "./state";
import { passClock, viewFromPlace } from "./passes";
import { matchesFilters } from "./filters/filters";
import { deadReckon } from "./geo";
import { solarPosition, type SkyPosition } from "./sun";
import { moonPosition, illuminatedFraction } from "./moon";
import { altColor } from "./theme/colors";

export {
  HORIZON_FOV,
  wrap360,
  bearingDelta,
  elevationToFrac,
  bearingToX,
} from "./horizon/geometry";

/** How far ahead the motion streak projects, seconds. */
const STREAK_SEC = 45;

export const horizonOpen = writable(false);
export const horizonRangeNm = writable(40);
/** Bearing at the centre of the window (0–360). */
export const horizonCenterBearing = writable(0);

export interface HorizonTarget {
  hex: string;
  callsign: string;
  typeCode: string | null;
  bearingDeg: number;
  elevationDeg: number;
  distanceNm: number;
  altBaroFt: number | null;
  color: string;
  military: boolean;
  emergency: boolean;
  /** A second point ~45 s ahead for a motion streak; null when not projectable. */
  aheadBearingDeg: number | null;
  aheadElevationDeg: number | null;
}

function heading(a: Aircraft): number | null {
  return a.track ?? a.trueHeading ?? a.magHeading;
}

/** Aircraft in range of the primary place, projected onto the sky. */
export const horizonTargets = derived(
  [aircraft, primaryPlace, filters, horizonRangeNm],
  ([$aircraft, $place, $filters, $range]) => {
    if (!$place) return [] as HorizonTarget[];
    const out: HorizonTarget[] = [];
    for (const a of $aircraft.values()) {
      if (a.onGround || a.altBaro == null) continue;
      if (!matchesFilters(a, $filters)) continue;
      const v = viewFromPlace(a, $place);
      if (!v || v.distanceNm > $range) continue;

      let aheadBearingDeg: number | null = null;
      let aheadElevationDeg: number | null = null;
      const hdg = heading(a);
      if (a.groundSpeed != null && hdg != null && a.lat != null && a.lon != null) {
        const [lat2, lon2] = deadReckon(a.lat, a.lon, hdg, a.groundSpeed, STREAK_SEC);
        const v2 = viewFromPlace({ ...a, lat: lat2, lon: lon2 }, $place);
        if (v2) {
          aheadBearingDeg = v2.bearingDeg;
          aheadElevationDeg = v2.elevationDeg;
        }
      }

      out.push({
        hex: a.hex,
        callsign: (a.flight ?? a.registration ?? a.hex).trim(),
        typeCode: a.typeCode,
        bearingDeg: v.bearingDeg,
        elevationDeg: v.elevationDeg,
        distanceNm: v.distanceNm,
        altBaroFt: a.altBaro,
        color: altColor(a.altBaro, false),
        military: a.military,
        emergency: !!a.emergency && a.emergency !== "none",
        aheadBearingDeg,
        aheadElevationDeg,
      });
    }
    return out;
  },
);

export interface HorizonBodies {
  sun: SkyPosition;
  moon: SkyPosition;
  /** 0 = new, 1 = full. */
  moonIllum: number;
}

/** Sun and Moon positions for the primary place, refreshed on the slow clock. */
export const horizonBodies = derived(
  [primaryPlace, passClock],
  ([$place, $now]) => {
    if (!$place) return null;
    const at = new Date($now);
    const sun = solarPosition(at, $place.lat, $place.lon);
    const moon = moonPosition(at, $place.lat, $place.lon);
    return { sun, moon, moonIllum: illuminatedFraction(sun, moon) } as HorizonBodies;
  },
);
