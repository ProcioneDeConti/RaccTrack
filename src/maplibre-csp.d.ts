// The CSP build ships without its own types; it is API-identical to the main
// entry point, so reuse those.
declare module "maplibre-gl/dist/maplibre-gl-csp" {
  import type maplibregl from "maplibre-gl";
  const m: typeof maplibregl;
  export default m;
}
