import { writable } from "svelte/store";

export const units = writable<"imperial" | "metric">("imperial");
let current: "imperial" | "metric" = "imperial";
units.subscribe((u) => (current = u));

export function altitude(ft: number | null): string {
  if (ft === null) return "—";
  return current === "metric"
    ? `${Math.round(ft * 0.3048).toLocaleString()} m`
    : `${Math.round(ft).toLocaleString()} ft`;
}

export function speed(kt: number | null): string {
  if (kt === null) return "—";
  return current === "metric"
    ? `${Math.round(kt * 1.852)} km/h`
    : `${Math.round(kt)} kt`;
}

export function verticalRate(fpm: number | null): string {
  if (fpm === null || fpm === 0) return "level";
  const arrow = fpm > 0 ? "▲" : "▼";
  const v =
    current === "metric"
      ? `${Math.round(Math.abs(fpm) * 0.00508 * 100) / 100} m/s`
      : `${Math.abs(Math.round(fpm)).toLocaleString()} fpm`;
  return `${arrow} ${v}`;
}

export function degrees(d: number | null): string {
  return d === null ? "—" : `${Math.round(d)}°`;
}

export function age(seconds: number | null): string {
  if (seconds === null) return "—";
  if (seconds < 1) return "now";
  if (seconds < 60) return `${Math.round(seconds)}s ago`;
  return `${Math.round(seconds / 60)}m ago`;
}

const SQUAWK_MEANING: Record<string, string> = {
  "7500": "Unlawful interference (hijack)",
  "7600": "Radio failure",
  "7700": "General emergency",
  "7777": "Military intercept / no ATC",
  "1200": "VFR (US)",
  "7000": "VFR (Europe)",
};

export function squawkMeaning(sq: string | null): string | null {
  return sq ? (SQUAWK_MEANING[sq] ?? null) : null;
}
