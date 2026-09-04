import { describe, it, expect } from "vitest";
import { predictPass, viewFromPlace } from "./passes";
import type { Aircraft } from "./api/types";

const PLACE = { lat: 40, lon: -74 };
const NOW = 1_700_000_000_000;

function ac(partial: Partial<Aircraft>): Aircraft {
  return {
    hex: "abc123",
    flight: "TEST123",
    registration: null,
    typeCode: null,
    description: null,
    category: null,
    lat: null,
    lon: null,
    altBaro: null,
    altGeom: null,
    onGround: false,
    groundSpeed: null,
    ias: null,
    tas: null,
    mach: null,
    track: null,
    magHeading: null,
    trueHeading: null,
    baroRate: null,
    geomRate: null,
    squawk: null,
    emergency: null,
    navAltitude: null,
    navHeading: null,
    navQnh: null,
    rssi: null,
    messages: null,
    seen: null,
    seenPos: null,
    positionSource: "adsb",
    military: false,
    interesting: false,
    pia: false,
    ladd: false,
    source: "test",
    ...partial,
  };
}

describe("predictPass", () => {
  it("predicts a near-overhead pass for an aircraft heading straight at the place", () => {
    // 60 nm due south, tracking north at 360 kt (6 nm/min) -> overhead in 10 min
    const p = predictPass(
      ac({ lat: 39, lon: -74, track: 0, groundSpeed: 360, altBaro: 35000 }),
      PLACE,
      NOW,
      12,
      15,
    );
    expect(p).not.toBeNull();
    expect(p!.minDistanceNm).toBeLessThan(0.1);
    expect(Math.round(p!.inSec)).toBe(600);
    expect(p!.etaMs).toBeCloseTo(NOW + 600_000, -3);
  });

  it("returns null for an aircraft heading away", () => {
    const p = predictPass(
      ac({ lat: 39, lon: -74, track: 180, groundSpeed: 360 }),
      PLACE,
      NOW,
      12,
      15,
    );
    expect(p).toBeNull();
  });

  it("respects the radius", () => {
    // passes ~10 nm east of the place
    const args = ac({ lat: 39, lon: -74, track: 10, groundSpeed: 360 });
    expect(predictPass(args, PLACE, NOW, 12, 15)!.minDistanceNm).toBeGreaterThan(
      9,
    );
    expect(predictPass(args, PLACE, NOW, 12, 5)).toBeNull();
  });

  it("respects the time horizon", () => {
    // 60 nm south at 120 kt (2 nm/min) -> 30 min away
    const args = ac({ lat: 39, lon: -74, track: 0, groundSpeed: 120 });
    expect(predictPass(args, PLACE, NOW, 12, 15)).toBeNull();
    expect(predictPass(args, PLACE, NOW, 35, 15)).not.toBeNull();
  });

  it("ignores aircraft on the ground, too slow, or without a heading", () => {
    const base = { lat: 39, lon: -74, track: 0, groundSpeed: 360 };
    expect(predictPass(ac({ ...base, onGround: true }), PLACE, NOW, 12, 15)).toBeNull();
    expect(predictPass(ac({ ...base, groundSpeed: 10 }), PLACE, NOW, 12, 15)).toBeNull();
    expect(predictPass(ac({ ...base, track: null }), PLACE, NOW, 12, 15)).toBeNull();
  });

  it("computes a sensible bearing at closest approach", () => {
    // ~12 nm north of the place and tracking east -> passes to the north (~0°)
    const p = predictPass(
      ac({ lat: 40.2, lon: -75, track: 90, groundSpeed: 300, altBaro: 10000 }),
      PLACE,
      NOW,
      20,
      15,
    );
    expect(p).not.toBeNull();
    expect(Math.min(p!.bearingDeg, 360 - p!.bearingDeg)).toBeLessThan(3);
    expect(p!.minDistanceNm).toBeGreaterThan(10);
    expect(p!.minDistanceNm).toBeLessThan(14);
  });
});

describe("viewFromPlace", () => {
  it("reports bearing, distance, low elevation, and closing state", () => {
    const v = viewFromPlace(
      ac({ lat: 39, lon: -74, track: 0, groundSpeed: 300, altBaro: 6076 }),
      PLACE,
    );
    expect(v).not.toBeNull();
    expect(Math.abs(v!.bearingDeg - 180)).toBeLessThan(1);
    expect(v!.distanceNm).toBeGreaterThan(55);
    expect(v!.distanceNm).toBeLessThan(65);
    expect(v!.elevationDeg).toBeGreaterThan(0);
    expect(v!.elevationDeg).toBeLessThan(3);
    expect(v!.closing).toBe(true);
  });

  it("reports opening when the aircraft is receding", () => {
    const v = viewFromPlace(
      ac({ lat: 39, lon: -74, track: 180, groundSpeed: 300 }),
      PLACE,
    );
    expect(v!.closing).toBe(false);
  });

  it("returns null without a position", () => {
    expect(viewFromPlace(ac({}), PLACE)).toBeNull();
  });
});
