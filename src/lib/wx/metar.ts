// Client-side METAR / TAF decoding. The backend already hands us every
// structured field aviationweather.gov exposes (wind, visibility, clouds,
// temp/dewpoint, altimeter, flight category, weather string), so the METAR
// decode is just presentation. TAF only arrives as raw text, so that half
// tokenises the string with the same wind/vis/wx/cloud helpers.

import type { Metar } from "../api/types";

export interface DecodedRow {
  label: string;
  value: string;
}

export interface DecodedMetar {
  rows: DecodedRow[];
  flags: string[]; // "Automated station", "Corrected", …
}

// ---- weather phenomena ------------------------------------------------------

const DESCRIPTOR: Record<string, string> = {
  MI: "shallow",
  PR: "partial",
  BC: "patches of",
  DR: "low drifting",
  BL: "blowing",
  SH: "showers",
  TS: "thunderstorm",
  FZ: "freezing",
};

const PHENOM: Record<string, string> = {
  DZ: "drizzle",
  RA: "rain",
  SN: "snow",
  SG: "snow grains",
  IC: "ice crystals",
  PL: "ice pellets",
  GR: "hail",
  GS: "small hail",
  UP: "unknown precipitation",
  BR: "mist",
  FG: "fog",
  FU: "smoke",
  VA: "volcanic ash",
  DU: "widespread dust",
  SA: "sand",
  HZ: "haze",
  PY: "spray",
  PO: "dust/sand whirls",
  SQ: "squalls",
  FC: "funnel cloud",
  SS: "sandstorm",
  DS: "duststorm",
};

/** Decode a single weather group like "-SHRA", "+TSRAGR", "VCFG", "FZFG". */
export function decodeWxToken(token: string): string | null {
  let t = token.toUpperCase();
  if (t === "NSW") return "No significant weather";

  let intensity = "";
  if (t.startsWith("+")) {
    intensity = "heavy ";
    t = t.slice(1);
  } else if (t.startsWith("-")) {
    intensity = "light ";
    t = t.slice(1);
  }

  let vicinity = false;
  if (t.startsWith("VC")) {
    vicinity = true;
    t = t.slice(2);
  }

  let descriptor = "";
  if (DESCRIPTOR[t.slice(0, 2)]) {
    descriptor = t.slice(0, 2);
    t = t.slice(2);
  }

  const phenoms: string[] = [];
  for (let i = 0; i + 2 <= t.length; i += 2) {
    const p = PHENOM[t.slice(i, i + 2)];
    if (!p) return null; // unrecognised — let the caller fall back to raw
    phenoms.push(p);
  }
  if (!descriptor && phenoms.length === 0) return null;

  const list = phenoms.join(" and ");
  let phrase: string;
  switch (descriptor) {
    case "TS":
      phrase = list ? `thunderstorm with ${list}` : "thunderstorm";
      break;
    case "SH":
      phrase = list ? `${list} showers` : "showers";
      break;
    case "FZ":
      phrase = list ? `freezing ${list}` : "freezing";
      break;
    case "MI":
    case "PR":
    case "BC":
    case "DR":
    case "BL":
      phrase = `${DESCRIPTOR[descriptor]} ${list}`.trim();
      break;
    default:
      phrase = list;
  }

  let out = (intensity + phrase).trim();
  if (vicinity) out += " in the vicinity";
  return out.charAt(0).toUpperCase() + out.slice(1);
}

/** Decode a space-separated weather string ("-RA BR" → "Light rain, Mist"). */
export function decodeWx(s: string | null | undefined): string | null {
  if (!s) return null;
  const parts = s
    .trim()
    .split(/\s+/)
    .map((tok) => decodeWxToken(tok) ?? tok);
  return parts.length ? parts.join(", ") : null;
}

// ---- wind / visibility / cloud helpers (shared with TAF) -------------------

const COMPASS = [
  "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE",
  "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW",
];

export function compassPoint(deg: number): string {
  return COMPASS[Math.round(((deg % 360) + 360) % 360 / 22.5) % 16];
}

export function describeWind(
  dir: number | string | null,
  speedKt: number | null,
  gustKt: number | null,
  variableRange?: [number, number] | null,
): string {
  if ((speedKt === 0 || speedKt === null) && (dir === null || dir === 0)) {
    return "Calm";
  }
  let base: string;
  if (dir === "VRB" || typeof dir === "string") {
    base = `Variable at ${speedKt ?? "?"} kt`;
  } else if (dir === null) {
    base = `${speedKt ?? "?"} kt`;
  } else {
    base = `From ${Math.round(dir)}° (${compassPoint(dir)}) at ${speedKt ?? "?"} kt`;
  }
  if (gustKt) base += `, gusting ${gustKt} kt`;
  if (variableRange) base += `, variable ${variableRange[0]}°–${variableRange[1]}°`;
  return base;
}

const KM_PER_SM = 1.60934;

/** `visib` is a number of statute miles or a string like "10+". */
export function describeVisibility(v: number | string | null): string | null {
  if (v === null || v === undefined) return null;
  if (typeof v === "string") {
    const plus = v.includes("+") || v.startsWith("P");
    const n = parseFloat(v.replace(/[^0-9.]/g, ""));
    if (!isFinite(n)) return v;
    const km = n * KM_PER_SM;
    return `${n}${plus ? "+" : ""} SM (≈ ${km.toFixed(km < 2 ? 1 : 0)}${plus ? "+" : ""} km)`;
  }
  const km = v * KM_PER_SM;
  return `${v} SM (≈ ${km.toFixed(km < 2 ? 1 : 0)} km)`;
}

const COVER: Record<string, string> = {
  FEW: "Few",
  SCT: "Scattered",
  BKN: "Broken",
  OVC: "Overcast",
  SKC: "Sky clear",
  CLR: "Sky clear",
  NCD: "No cloud detected",
  NSC: "No significant cloud",
  VV: "Vertical visibility",
};

function coverText(code: string | null): string {
  return COVER[(code ?? "").toUpperCase()] ?? code ?? "";
}

const ft = (n: number) => `${Math.round(n).toLocaleString()} ft`;

export function describeClouds(
  clouds: { cover: string | null; baseFt: number | null }[],
): { layers: string; ceilingFt: number | null } {
  if (!clouds.length) return { layers: "No cloud reported", ceilingFt: null };
  const parts: string[] = [];
  let ceiling: number | null = null;
  for (const c of clouds) {
    const cov = (c.cover ?? "").toUpperCase();
    if (cov === "SKC" || cov === "CLR" || cov === "NSC" || cov === "NCD") {
      parts.push(coverText(cov));
      continue;
    }
    if (c.baseFt == null) {
      parts.push(coverText(cov));
      continue;
    }
    parts.push(`${coverText(cov)} at ${ft(c.baseFt)}`);
    if ((cov === "BKN" || cov === "OVC" || cov === "VV") && ceiling === null) {
      ceiling = c.baseFt;
    }
  }
  return { layers: parts.join(", "), ceilingFt: ceiling };
}

// ---- METAR ----------------------------------------------------------------

function relativeAge(epochMs: number): string {
  const min = Math.round((Date.now() - epochMs) / 60000);
  if (min < 1) return "just now";
  if (min < 60) return `${min} min ago`;
  const h = Math.floor(min / 60);
  return `${h}h ${min % 60}m ago`;
}

function utcHhmm(epochMs: number): string {
  const d = new Date(epochMs);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getUTCHours())}:${p(d.getUTCMinutes())}Z`;
}

function relHumidity(tempC: number, dewpC: number): number {
  const e = (t: number) => Math.exp((17.625 * t) / (243.04 + t));
  return Math.round(100 * (e(dewpC) / e(tempC)));
}

export function decodeMetar(m: Metar): DecodedMetar {
  const raw = (m.raw ?? "").toUpperCase();
  const rows: DecodedRow[] = [];
  const flags: string[] = [];

  if (/\bAUTO\b/.test(raw)) flags.push("Automated station (AUTO)");
  if (/\bCOR\b/.test(raw)) flags.push("Corrected report (COR)");
  const cavok = /\bCAVOK\b/.test(raw);

  if (m.obsTime) {
    rows.push({
      label: "Observed",
      value: `${utcHhmm(m.obsTime)} · ${relativeAge(m.obsTime)}`,
    });
  }

  if (m.flightCategory) {
    const meaning: Record<string, string> = {
      VFR: "Visual conditions",
      MVFR: "Marginal visual conditions",
      IFR: "Instrument conditions",
      LIFR: "Low instrument conditions",
    };
    rows.push({
      label: "Flight category",
      value: `${m.flightCategory} — ${meaning[m.flightCategory] ?? ""}`.trim(),
    });
  }

  const vrb = raw.match(/\b(\d{3})V(\d{3})\b/);
  rows.push({
    label: "Wind",
    value: describeWind(
      m.windDir,
      m.windKt,
      m.gustKt,
      vrb ? [parseInt(vrb[1], 10), parseInt(vrb[2], 10)] : null,
    ),
  });

  if (cavok) {
    rows.push({ label: "Visibility", value: "10+ km, no significant cloud (CAVOK)" });
  } else {
    const vis = describeVisibility(m.visibility);
    if (vis) rows.push({ label: "Visibility", value: vis });
  }

  const wx = decodeWx(m.wxString);
  if (wx) rows.push({ label: "Weather", value: wx });

  if (!cavok) {
    const { layers, ceilingFt } = describeClouds(m.clouds);
    rows.push({ label: "Clouds", value: layers });
    if (ceilingFt != null) {
      rows.push({ label: "Ceiling", value: ft(ceilingFt) });
    }
  }

  if (m.tempC != null && m.dewpointC != null) {
    const spread = Math.round((m.tempC - m.dewpointC) * 10) / 10;
    rows.push({
      label: "Temp / dewpoint",
      value: `${Math.round(m.tempC)} °C / ${Math.round(m.dewpointC)} °C (spread ${spread} °C)`,
    });
    rows.push({
      label: "Humidity",
      value: `${relHumidity(m.tempC, m.dewpointC)}%`,
    });
  } else if (m.tempC != null) {
    rows.push({ label: "Temperature", value: `${Math.round(m.tempC)} °C` });
  }

  if (m.altimeterHpa != null) {
    const inHg = m.altimeterHpa * 0.0295299830714;
    rows.push({
      label: "Altimeter",
      value: `${inHg.toFixed(2)} inHg (${Math.round(m.altimeterHpa)} hPa)`,
    });
  }

  return { rows, flags };
}

// ---- TAF ----------------------------------------------------------------

export interface TafPeriod {
  heading: string; // "From 20:00Z", "Temporarily 03/20–03/23Z (30%)", …
  lines: string[]; // decoded wind / visibility / weather / cloud lines
  extra: string[]; // tokens we didn't recognise, shown verbatim
}

export interface DecodedTaf {
  issued: string | null;
  valid: string | null;
  periods: TafPeriod[];
}

const WIND_RE = /^(\d{3}|VRB)(\d{2,3})(?:G(\d{2,3}))?(KT|MPS)$/;
const CLOUD_RE = /^(FEW|SCT|BKN|OVC|VV)(\d{3})(CB|TCU)?$/;
const VIS_SM_RE = /^(M)?(P)?(\d{1,2})(?:\/(\d))?SM$/;
const WX_RE =
  /^(\+|-|VC)?(MI|PR|BC|DR|BL|SH|TS|FZ)?(DZ|RA|SN|SG|IC|PL|GR|GS|UP|BR|FG|FU|VA|DU|SA|HZ|PY|PO|SQ|FC|SS|DS|NSW)+$/;

function fmtDayHour(ddhhmm: string): string {
  return `${ddhhmm.slice(0, 2)} ${ddhhmm.slice(2, 4)}:${ddhhmm.slice(4, 6) || "00"}Z`;
}

function periodLabel(a: string, b: string): string {
  const day = (s: string) => `${s.slice(0, 2)} ${s.slice(2, 4)}:00Z`;
  return `${day(a)} – ${day(b)}`;
}

function decodeTafSegment(tokens: string[]): { lines: string[]; extra: string[] } {
  const lines: string[] = [];
  const extra: string[] = [];
  const clouds: { cover: string | null; baseFt: number | null }[] = [];
  const wx: string[] = [];
  let convective = "";

  for (const tok of tokens) {
    let mm: RegExpMatchArray | null;
    if ((mm = tok.match(WIND_RE))) {
      const dir = mm[1] === "VRB" ? "VRB" : parseInt(mm[1], 10);
      lines.push(
        "Wind: " +
          describeWind(dir, parseInt(mm[2], 10), mm[3] ? parseInt(mm[3], 10) : null),
      );
    } else if (tok === "P6SM") {
      lines.push("Visibility: 6+ SM (≈ 10+ km)");
    } else if ((mm = tok.match(VIS_SM_RE))) {
      const n = mm[4] ? parseInt(mm[3], 10) / parseInt(mm[4], 10) : parseInt(mm[3], 10);
      const val = (mm[1] ? "less than " : mm[2] ? "more than " : "") + n;
      lines.push(`Visibility: ${val} SM (≈ ${(n * KM_PER_SM).toFixed(n < 2 ? 1 : 0)} km)`);
    } else if ((mm = tok.match(CLOUD_RE))) {
      clouds.push({ cover: mm[1], baseFt: parseInt(mm[2], 10) * 100 });
      if (mm[3]) convective = mm[3] === "CB" ? "cumulonimbus" : "towering cumulus";
    } else if (tok === "SKC" || tok === "CLR" || tok === "NSC") {
      clouds.push({ cover: tok, baseFt: null });
    } else if (tok === "CAVOK") {
      lines.push("Visibility 10+ km, no significant cloud (CAVOK)");
    } else if (WX_RE.test(tok) || tok === "NSW") {
      wx.push(decodeWxToken(tok) ?? tok);
    } else if (/^(TX|TN)M?\d+\/\d+Z$/.test(tok)) {
      const t = tok.match(/^(TX|TN)(M?\d+)\/(\d+)Z$/)!;
      const c = parseInt(t[2].replace("M", "-"), 10);
      lines.push(
        `${t[1] === "TX" ? "Max" : "Min"} temp ${c} °C at ${t[3].slice(0, 2)} ${t[3].slice(2, 4)}:00Z`,
      );
    } else if (tok.startsWith("QNH")) {
      const q = parseInt(tok.slice(3), 10);
      if (isFinite(q)) lines.push(`Altimeter: ${q / 100} inHg`);
    } else if (tok === "AMD" || tok === "COR") {
      // handled at heading level
    } else {
      extra.push(tok);
    }
  }

  if (wx.length) lines.splice(1, 0, "Weather: " + wx.join(", "));
  if (clouds.length) {
    const { layers, ceilingFt } = describeClouds(clouds);
    lines.push(
      "Clouds: " + layers + (convective ? ` (${convective})` : ""),
    );
    if (ceilingFt != null) lines.push("Ceiling: " + ft(ceilingFt));
  } else if (convective) {
    lines.push(`Clouds: ${convective}`);
  }
  return { lines, extra };
}

export function decodeTaf(rawTaf: string | null | undefined): DecodedTaf | null {
  if (!rawTaf) return null;
  const tokens = rawTaf
    .toUpperCase()
    .replace(/\s+/g, " ")
    .replace(/^TAF (AMD |COR )?/, "")
    .trim()
    .split(" ");
  if (tokens.length < 3) return null;

  let i = 0;
  // station
  if (/^[A-Z]{4}$/.test(tokens[i])) i++;
  let issued: string | null = null;
  if (/^\d{6}Z$/.test(tokens[i])) {
    issued = `${tokens[i].slice(0, 2)} ${tokens[i].slice(2, 4)}:${tokens[i].slice(4, 6)}Z`;
    i++;
  }
  let valid: string | null = null;
  if (/^\d{4}\/\d{4}$/.test(tokens[i])) {
    const [a, b] = tokens[i].split("/");
    valid = periodLabel(a, b);
    i++;
  }

  const periods: TafPeriod[] = [];
  let curHeading = "Initial forecast";
  let curTokens: string[] = [];

  const flush = () => {
    if (!curTokens.length && periods.length) return;
    const { lines, extra } = decodeTafSegment(curTokens);
    periods.push({ heading: curHeading, lines, extra });
    curTokens = [];
  };

  for (; i < tokens.length; i++) {
    const tok = tokens[i];
    let mm: RegExpMatchArray | null;
    if ((mm = tok.match(/^FM(\d{6})$/))) {
      flush();
      curHeading = `From ${fmtDayHour(mm[1])}`;
    } else if (tok === "BECMG" && /^\d{4}\/\d{4}$/.test(tokens[i + 1] ?? "")) {
      flush();
      const [a, b] = tokens[++i].split("/");
      curHeading = `Becoming ${periodLabel(a, b)}`;
    } else if (tok === "TEMPO" && /^\d{4}\/\d{4}$/.test(tokens[i + 1] ?? "")) {
      flush();
      const [a, b] = tokens[++i].split("/");
      curHeading = `Temporarily ${periodLabel(a, b)}`;
    } else if ((mm = tok.match(/^PROB(\d{2})$/))) {
      flush();
      let head = `${mm[1]}% probability`;
      if (tokens[i + 1] === "TEMPO") {
        i++;
        head += " (temporary)";
      }
      if (/^\d{4}\/\d{4}$/.test(tokens[i + 1] ?? "")) {
        const [a, b] = tokens[++i].split("/");
        head += ` ${periodLabel(a, b)}`;
      }
      curHeading = head;
    } else {
      curTokens.push(tok);
    }
  }
  flush();

  return { issued, valid, periods };
}
