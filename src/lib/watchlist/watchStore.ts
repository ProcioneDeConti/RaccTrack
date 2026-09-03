import { writable } from "svelte/store";
import type { AlertEvent, WatchEntry } from "../api/types";
import { listWatch } from "../api/backend";

export const watchEntries = writable<WatchEntry[]>([]);
export const alertLog = writable<AlertEvent[]>([]);
export const lastAlert = writable<AlertEvent | null>(null);

export async function refreshWatch(): Promise<void> {
  watchEntries.set(await listWatch());
}

export function pushAlert(a: AlertEvent): void {
  alertLog.update((l) => [a, ...l].slice(0, 100));
  lastAlert.set(a);
}
