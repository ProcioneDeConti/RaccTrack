import { describe, it, expect } from "vitest";
import {
  wrap360,
  bearingDelta,
  elevationToFrac,
  bearingToX,
  HORIZON_FOV,
} from "./geometry";

describe("wrap360", () => {
  it("normalises into [0, 360)", () => {
    expect(wrap360(-10)).toBe(350);
    expect(wrap360(370)).toBe(10);
    expect(wrap360(0)).toBe(0);
    expect(wrap360(720)).toBe(0);
  });
});

describe("bearingDelta", () => {
  it("returns the shortest signed difference", () => {
    expect(bearingDelta(10, 350)).toBe(20);
    expect(bearingDelta(350, 10)).toBe(-20);
    expect(bearingDelta(0, 0)).toBe(0);
    expect(Math.abs(bearingDelta(180, 0))).toBe(180); // ±180 at the antipode
    expect(bearingDelta(100, 20)).toBe(80);
  });
});

describe("elevationToFrac", () => {
  it("is clamped to 0..1 and monotonic", () => {
    expect(elevationToFrac(0)).toBe(0);
    expect(elevationToFrac(-5)).toBe(0);
    expect(elevationToFrac(90)).toBeCloseTo(1);
    expect(elevationToFrac(120)).toBeCloseTo(1);
    expect(elevationToFrac(10)).toBeLessThan(elevationToFrac(45));
  });
  it("compresses low angles (45° sits above the midline)", () => {
    expect(elevationToFrac(45)).toBeGreaterThan(0.5);
  });
});

describe("bearingToX", () => {
  it("centres the view bearing", () => {
    expect(bearingToX(90, 90, 600)).toBe(300);
  });
  it("maps the FOV edges to the panel edges", () => {
    expect(bearingToX(90 - HORIZON_FOV / 2, 90, 600)).toBeCloseTo(0);
    expect(bearingToX(90 + HORIZON_FOV / 2, 90, 600)).toBeCloseTo(600);
  });
  it("culls bearings well outside the window", () => {
    expect(bearingToX(250, 90, 600)).toBeNull();
  });
  it("handles the wrap at north", () => {
    expect(bearingToX(350, 10, 600)).toBeCloseTo(300 - (20 / HORIZON_FOV) * 600);
  });
});
