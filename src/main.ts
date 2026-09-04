import "maplibre-gl/dist/maplibre-gl.css";
// CSP-friendly MapLibre build: the worker is a separate same-origin asset rather
// than a blob: URL, so it runs under a strict Content-Security-Policy.
import maplibregl from "maplibre-gl/dist/maplibre-gl-csp";
import mlWorkerUrl from "maplibre-gl/dist/maplibre-gl-csp-worker.js?url";
import "./app.css";
import { installDiagnostics } from "./lib/diag";
import { basemap } from "./lib/state";
import { themeFor } from "./lib/map/style";
import App from "./App.svelte";

maplibregl.setWorkerUrl(mlWorkerUrl);
installDiagnostics();

// V6: the app chrome follows the basemap — light panels/rail/etc. under a
// light basemap. Driven off the same `basemap` store MapView reads.
basemap.subscribe((key) => {
  document.documentElement.classList.toggle("theme-light", !themeFor(key).dark);
});

const app = new App({
  target: document.getElementById("app")!,
});

export default app;
