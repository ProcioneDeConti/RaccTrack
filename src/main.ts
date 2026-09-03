import "maplibre-gl/dist/maplibre-gl.css";
// CSP-friendly MapLibre build: the worker is a separate same-origin asset rather
// than a blob: URL, so it runs under a strict Content-Security-Policy.
import maplibregl from "maplibre-gl/dist/maplibre-gl-csp";
import mlWorkerUrl from "maplibre-gl/dist/maplibre-gl-csp-worker.js?url";
import "./app.css";
import { installDiagnostics } from "./lib/diag";
import App from "./App.svelte";

maplibregl.setWorkerUrl(mlWorkerUrl);
installDiagnostics();

const app = new App({
  target: document.getElementById("app")!,
});

export default app;
