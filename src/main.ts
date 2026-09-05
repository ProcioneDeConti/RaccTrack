import "maplibre-gl/dist/maplibre-gl.css";
// CSP-friendly MapLibre build: the worker is a separate same-origin asset rather
// than a blob: URL, so it runs under a strict Content-Security-Policy.
import maplibregl from "maplibre-gl/dist/maplibre-gl-csp";
import mlWorkerUrl from "maplibre-gl/dist/maplibre-gl-csp-worker.js?url";
import "./app.css";
import { derived } from "svelte/store";
import { installDiagnostics } from "./lib/diag";
import { basemap, uiTheme } from "./lib/state";
import { themeFor } from "./lib/map/style";
import App from "./App.svelte";

maplibregl.setWorkerUrl(mlWorkerUrl);
installDiagnostics();

// V6: the app chrome's light/dark theme. "auto" follows the basemap (light
// panels/rail/etc. under a light basemap, the original behavior); "light"/
// "dark" pin it regardless of the map tiles, so e.g. a light map + dark UI
// is possible. Driven off the same `basemap` store MapView reads, plus the
// user's explicit `uiTheme` choice.
derived([basemap, uiTheme], ([$basemap, $uiTheme]) => {
  if ($uiTheme === "light") return true;
  if ($uiTheme === "dark") return false;
  return !themeFor($basemap).dark;
}).subscribe((light) => {
  document.documentElement.classList.toggle("theme-light", light);
});

const app = new App({
  target: document.getElementById("app")!,
});

export default app;
