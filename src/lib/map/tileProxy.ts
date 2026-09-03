import type { RequestParameters } from "maplibre-gl";
import { TILE_UPSTREAM_HOST } from "./style";

// The Rust side registers a custom URI scheme `ofmtiles` that serves basemap
// resources from an on-disk SQLite cache (see src-tauri/src/tiles.rs), fetching
// and storing on a miss. MapLibre's transformRequest rewrites every request for
// the upstream tile host to that scheme so panning fills the cache and a
// pre-downloaded area keeps working offline.
//
// Tauri exposes custom schemes differently per platform:
//   Windows/Android:  http://<scheme>.localhost/<path>
//   macOS/Linux/iOS:  <scheme>://localhost/<path>

const SCHEME = "ofmtiles";

async function schemeBase(): Promise<string> {
  try {
    const os = await import("@tauri-apps/plugin-os");
    const platform = await os.platform();
    // Tauri serves custom URI schemes as http://<scheme>.localhost on
    // Windows/Android and <scheme>://localhost elsewhere.
    if (platform === "windows" || platform === "android") {
      return `http://${SCHEME}.localhost/`;
    }
    return `${SCHEME}://localhost/`;
  } catch {
    // Not running under Tauri (e.g. `vite` in a browser) — no proxy.
    return "";
  }
}

export async function makeTransformRequest(
  enabled: boolean,
): Promise<
  ((url: string, resourceType?: string) => RequestParameters) | undefined
> {
  if (!enabled) return undefined;
  const base = await schemeBase();
  if (!base) return undefined;

  const prefix = `https://${TILE_UPSTREAM_HOST}/`;
  return (url: string): RequestParameters => {
    if (url.startsWith(prefix)) {
      return { url: base + url.slice(prefix.length) };
    }
    return { url };
  };
}
