import { writable } from "svelte/store";
import type { AlertEvent, WatchEntry } from "../api/types";
import { listWatch } from "../api/backend";

export const watchEntries = writable<WatchEntry[]>([]);
/** Most recent alert — drives the toast. Full history lives in the Events panel. */
export const lastAlert = writable<AlertEvent | null>(null);

export async function refreshWatch(): Promise<void> {
  watchEntries.set(await listWatch());
}

export function pushAlert(a: AlertEvent): void {
  lastAlert.set(a);
}
