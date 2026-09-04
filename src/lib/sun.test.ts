import { describe, it, expect } from "vitest";
import { solarPosition, lightPhase, litSide } from "./sun";

describe("solarPosition", () => {
  it("puts the midday sun near the zenith over the equator at an equinox", () => {
    const { elevation, azimuth } = solarPosition(
      new Date("2025-03-20T12:00:00Z"),
      0,
      0,
    );
    expect(elevation).toBeGreaterThan(80);
    expect(azimuth).toBeGreaterThanOrEqual(0);
    expect(azimuth).toBeLessThan(360);
  });

  it("puts the sun below the horizon at local midnight", () => {
    const { elevation } = solarPosition(
      new Date("2025-03-20T00:00:00Z"),
      0,
      0,
    );
    expect(elevation).toBeLessThan(0);
  });

  it("sees a much higher summer sun than winter sun at 40°N", () => {
    // ~solar noon for longitude -74 is close to 17:00 UTC
    const summer = solarPosition(new Date("2025-06-21T17:00:00Z"), 40, -74);
    const winter = solarPosition(new Date("2025-12-21T17:00:00Z"), 40, -74);
    expect(summer.elevation).toBeGreaterThan(winter.elevation + 30);
  });

  it("has the sun roughly due south at solar noon for a northern site", () => {
    const { azimuth } = solarPosition(new Date("2025-06-21T17:00:00Z"), 40, -74);
    expect(Math.abs(azimuth - 180)).toBeLessThan(35);
  });
});

describe("lightPhase", () => {
  it("classifies by sun elevation", () => {
    expect(lightPhase(30)).toBe("day");
    expect(lightPhase(3)).toBe("golden");
    expect(lightPhase(-3)).toBe("blue");
    expect(lightPhase(-20)).toBe("night");
  });
});

describe("litSide", () => {
  it("is front-lit when the sun is behind the photographer", () => {
    expect(litSide(0, 180, 20)).toBe("front");
  });
  it("is back-lit when shooting toward the sun", () => {
    expect(litSide(180, 180, 20)).toBe("back");
  });
  it("is side-lit at 90° off", () => {
    expect(litSide(90, 180, 20)).toBe("side");
  });
  it("is n/a once the sun is down", () => {
    expect(litSide(180, 180, -5)).toBe("n/a");
  });
});
