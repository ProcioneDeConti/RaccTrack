import { describe, it, expect } from "vitest";
import { moonPosition, angularSeparation, illuminatedFraction } from "./moon";
import { solarPosition } from "./sun";

describe("moonPosition", () => {
  it("returns finite alt/az within range", () => {
    const p = moonPosition(new Date("2025-06-15T12:00:00Z"), 40, -74);
    expect(Number.isFinite(p.elevation)).toBe(true);
    expect(p.azimuth).toBeGreaterThanOrEqual(0);
    expect(p.azimuth).toBeLessThan(360);
    expect(p.elevation).toBeGreaterThanOrEqual(-90);
    expect(p.elevation).toBeLessThanOrEqual(90);
  });

  it("puts a full moon high in the sky at local midnight", () => {
    // Wolf Moon was full ~2025-01-13 22:27 UTC; 05:00 UTC ≈ midnight US Eastern
    const p = moonPosition(new Date("2025-01-13T05:00:00Z"), 40, -74);
    expect(p.elevation).toBeGreaterThan(20);
  });

  it("moves several degrees across an hour", () => {
    const a = moonPosition(new Date("2025-03-10T02:00:00Z"), 40, -74);
    const b = moonPosition(new Date("2025-03-10T03:00:00Z"), 40, -74);
    expect(angularSeparation(a, b)).toBeGreaterThan(5);
  });
});

describe("angularSeparation", () => {
  it("is 0 for identical positions and 180 for opposite", () => {
    expect(angularSeparation({ azimuth: 10, elevation: 30 }, { azimuth: 10, elevation: 30 })).toBeCloseTo(0);
    expect(
      angularSeparation({ azimuth: 0, elevation: 90 }, { azimuth: 0, elevation: -90 }),
    ).toBeCloseTo(180);
  });
});

describe("illuminatedFraction", () => {
  it("is near 1 at full moon and near 0 at new moon", () => {
    const at = (iso: string): number =>
      illuminatedFraction(
        solarPosition(new Date(iso), 40, -74),
        moonPosition(new Date(iso), 40, -74),
      );
    expect(at("2025-01-13T05:00:00Z")).toBeGreaterThan(0.85);
    expect(at("2025-01-29T12:00:00Z")).toBeLessThan(0.2);
  });
});
