// Single source of truth for the data-driven colours the app assigns in code —
// altitude bands, METAR flight categories, airspace classes, and the two brand
// colours. These were previously copy-pasted across map layers, panels, and
// legends (see the polish audit's V4).
//
// UI-chrome tokens (bg / panel / border / text) stay in app.css as CSS custom
// properties. `--accent` and `--emergency` are mirrored here because map layers
// and canvas markers can't read CSS variables — keep the two in sync.

/** Brand accent + alert. Mirror of app.css `--accent` / `--emergency`. */
export const ACCENT = "#4c9be8";
export const EMERGENCY = "#ff3b30";

/** Caution-tape identity — the ADS-B coverage boundary stripe. */
export const CAUTION_GOLD = "#d4a017";
export const CAUTION_GREY = "#1b1b1b";

// --- METAR flight category ------------------------------------------------

/** Flight-category → colour. Shared by the airport-dot recolour (overlays),
 *  the airport-panel badge, and the Layers legend. */
export const FLIGHT_CATEGORY_COLORS: Record<string, string> = {
  VFR: "#3fb950",
  MVFR: "#3b82f6",
  IFR: "#ef4444",
  LIFR: "#d946ef",
};
/** Airport with weather on but no/unknown category. */
export const FLIGHT_CATEGORY_FALLBACK = "#c9d1d9";

// --- Airspace classes ----------------------------------------------------

export interface AirspaceStyle {
  color: string;
  /** MapLibre `line-dasharray`, omitted for solid lines. */
  dash?: number[];
}

/** Airspace category → line/fill style. Shared by the airspace layers, the
 *  click popup, and the Layers legend. Keys match the backend `category`. */
export const AIRSPACE_STYLE: Record<string, AirspaceStyle> = {
  CLASS_B: { color: "#3b82f6" },
  CLASS_C: { color: "#d946ef" },
  CLASS_D: { color: "#60a5fa", dash: [3, 2] },
  CLASS_E: { color: "#a78bfa", dash: [1, 2] },
  MODE_C: { color: "#94a3b8", dash: [1, 3] },
  MOA: { color: "#fb923c", dash: [4, 2] },
  RESTRICTED: { color: "#ef4444" },
  PROHIBITED: { color: "#ef4444" },
  WARNING: { color: "#ef4444", dash: [4, 2] },
  ALERT: { color: "#eab308", dash: [4, 2] },
};
export const AIRSPACE_FALLBACK = "#64748b";

// --- Altitude ----------------------------------------------------------

/** Aircraft on the ground / with no altitude. */
export const ALT_GROUND = "#9aa0a6";

/** Stepped altitude scale (feet ceiling, colour) — used to tint aircraft
 *  icons. `altBaro < ceiling` picks the band. */
const ALT_BANDS: readonly [number, string][] = [
  [10000, "#7ad151"],
  [20000, "#f9c74f"],
  [30000, "#f3722c"],
  [40000, "#e63946"],
  [Infinity, "#b5179e"],
];

export function altColor(altBaro: number | null, onGround: boolean): string {
  if (onGround || altBaro === null) return ALT_GROUND;
  for (const [ceiling, col] of ALT_BANDS) if (altBaro < ceiling) return col;
  return ALT_BANDS[ALT_BANDS.length - 1][1];
}

/** Continuous [altitude ft, colour] stops for the flight-trail line's
 *  MapLibre `interpolate` expression. Visually consistent with `altColor`. */
export const ALT_GRADIENT: readonly [number, string][] = [
  [0, "#7ad151"],
  [15000, "#f9c74f"],
  [30000, "#f3722c"],
  [42000, "#b5179e"],
];
