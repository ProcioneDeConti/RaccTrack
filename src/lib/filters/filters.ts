import type { Aircraft } from "../api/types";

export interface Filters {
  altMin: number; // feet
  altMax: number; // feet
  militaryOnly: boolean;
  emergencyOnly: boolean;
  hideGround: boolean;
  requirePosition: boolean;
  types: string[]; // ICAO type codes; empty = all
}

export const ALT_CEILING = 60000;

export function defaultFilters(): Filters {
  return {
    altMin: 0,
    altMax: ALT_CEILING,
    militaryOnly: false,
    emergencyOnly: false,
    hideGround: false,
    requirePosition: true,
    types: [],
  };
}

export function isDefault(f: Filters): boolean {
  const d = defaultFilters();
  return (
    f.altMin === d.altMin &&
    f.altMax === d.altMax &&
    f.militaryOnly === d.militaryOnly &&
    f.emergencyOnly === d.emergencyOnly &&
    f.hideGround === d.hideGround &&
    f.requirePosition === d.requirePosition &&
    f.types.length === 0
  );
}

export function matchesFilters(a: Aircraft, f: Filters): boolean {
  if (f.requirePosition && (a.lat === null || a.lon === null)) return false;
  if (f.militaryOnly && !a.military) return false;
  if (f.emergencyOnly && (!a.emergency || a.emergency === "none")) return false;
  if (f.hideGround && a.onGround) return false;

  // Altitude: treat on-ground / unknown as 0 ft for range purposes, but only
  // exclude when the user has narrowed the range away from the ground.
  const alt = a.onGround ? 0 : (a.altBaro ?? a.altGeom ?? 0);
  if (alt < f.altMin || alt > f.altMax) return false;

  if (f.types.length > 0) {
    if (!a.typeCode || !f.types.includes(a.typeCode.toUpperCase())) return false;
  }
  return true;
}
