// Basemap styles. All are key-free vector styles (CARTO + OpenFreeMap), using
// OpenStreetMap data. The active one is chosen in Settings.

export interface BasemapTheme {
  key: string;
  label: string;
  url: string;
  /** Host whose tile requests the (opt-in) cache proxy would intercept. */
  tileHost: string;
  dark: boolean;
}

export const BASEMAP_THEMES: BasemapTheme[] = [
  {
    key: "darkMatter",
    label: "Dark Matter (dark)",
    url: "https://basemaps.cartocdn.com/gl/dark-matter-gl-style/style.json",
    tileHost: "tiles.basemaps.cartocdn.com",
    dark: true,
  },
  {
    key: "positron",
    label: "Positron (light)",
    url: "https://basemaps.cartocdn.com/gl/positron-gl-style/style.json",
    tileHost: "tiles.basemaps.cartocdn.com",
    dark: false,
  },
  {
    key: "voyager",
    label: "Voyager (light, colourful)",
    url: "https://basemaps.cartocdn.com/gl/voyager-gl-style/style.json",
    tileHost: "tiles.basemaps.cartocdn.com",
    dark: false,
  },
  {
    key: "ofmLiberty",
    label: "OpenFreeMap Liberty (colourful)",
    url: "https://tiles.openfreemap.org/styles/liberty",
    tileHost: "tiles.openfreemap.org",
    dark: false,
  },
  {
    key: "ofmDark",
    label: "OpenFreeMap Dark (minimal)",
    url: "https://tiles.openfreemap.org/styles/dark",
    tileHost: "tiles.openfreemap.org",
    dark: true,
  },
];

export const DEFAULT_BASEMAP = "darkMatter";

export function themeFor(key: string | undefined): BasemapTheme {
  return (
    BASEMAP_THEMES.find((t) => t.key === key) ??
    BASEMAP_THEMES.find((t) => t.key === DEFAULT_BASEMAP)!
  );
}

export function resolveStyleUrl(key: string | undefined): string {
  return themeFor(key).url;
}

/** Legacy single-URL export (first paint before settings load). */
export const BASEMAP_STYLE_URL = themeFor(DEFAULT_BASEMAP).url;
export const TILE_UPSTREAM_HOST = themeFor(DEFAULT_BASEMAP).tileHost;

export const BASEMAP_ATTRIBUTION =
  '© <a href="https://www.openstreetmap.org/copyright" target="_blank">OpenStreetMap</a> · ' +
  'basemap © <a href="https://carto.com/attributions" target="_blank">CARTO</a> / ' +
  '<a href="https://openfreemap.org" target="_blank">OpenFreeMap</a>';

export const DATA_ATTRIBUTION =
  'Aircraft: <a href="https://adsb.lol" target="_blank">adsb.lol</a> / ' +
  '<a href="https://adsb.fi" target="_blank">adsb.fi</a> (ODbL)';
