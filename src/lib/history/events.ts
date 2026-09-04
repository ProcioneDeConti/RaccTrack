import type { AircraftEvent, EventKind } from "../api/types";
import type { IconName } from "../ui/Icon.svelte";
import { squawkMeaning } from "../format";

export function eventIcon(k: EventKind): IconName {
  switch (k) {
    case "emergency":
    case "emergency_clear":
      return "alert-triangle";
    case "takeoff":
      return "arrow-up";
    case "landing":
      return "arrow-down";
    case "alert":
      return "star";
    case "squawk":
    case "callsign":
    default:
      return "arrow-right";
  }
}

/** Whether this event should read as urgent (emergency styling). */
export function eventIsUrgent(k: EventKind): boolean {
  return k === "emergency";
}

const clean = (s: string | null | undefined) => (s ?? "").trim();

export function eventText(e: AircraftEvent): string {
  const from = clean(e.from);
  const to = clean(e.to);
  switch (e.kind) {
    case "squawk":
      return `Squawk ${from || "—"} → ${to || "—"}`;
    case "emergency": {
      const m = squawkMeaning(to);
      return `Emergency — squawk ${to}${m ? ` (${m})` : ""}`;
    }
    case "emergency_clear":
      return `Emergency cleared${from ? ` — was squawk ${from}` : ""}`;
    case "callsign":
      return `Callsign ${from || "—"} → ${to || "—"}`;
    case "takeoff":
      return "Took off";
    case "landing":
      return "Landed";
    case "alert":
      return to || "Alert";
  }
}

export function eventTime(ms: number): string {
  return new Date(ms).toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

/** Coarse category for the Events-panel filter chips. */
export type EventFilter = "all" | "emergency" | "squawk" | "movement" | "watch";

export function matchesFilter(k: EventKind, f: EventFilter): boolean {
  switch (f) {
    case "all":
      return true;
    case "emergency":
      return k === "emergency" || k === "emergency_clear";
    case "squawk":
      return k === "squawk";
    case "movement":
      return k === "takeoff" || k === "landing";
    case "watch":
      return k === "alert";
  }
}
