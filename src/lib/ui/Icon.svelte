<script lang="ts" context="module">
  // Inline SVG icon set. Geometry is from Lucide (https://lucide.dev, ISC
  // licence) — copied rather than pulled in as a dependency so the bundle stays
  // small and there's no build-step change. See NOTICE. Everything is authored
  // on a 24x24 grid with a 2px round stroke and inherits `currentColor`.
  //
  // Each icon is one or more SVG path `d` strings (circles/polygons from the
  // originals are rewritten as paths so rendering is a plain {#each} — Svelte's
  // {@html} does not reliably parse into the SVG namespace).
  export type IconName =
    | "x"
    | "settings"
    | "home"
    | "list"
    | "star"
    | "filter"
    | "layers"
    | "search"
    | "arrow-up"
    | "arrow-down"
    | "arrow-right"
    | "chevron-up"
    | "chevron-down"
    | "chevron-left"
    | "chevron-right"
    | "alert-triangle"
    | "external-link"
    | "pin"
    | "crosshair"
    | "plus";

  export const ICON_PATHS: Record<IconName, string[]> = {
    x: ["M18 6 6 18", "m6 6 12 12"],
    settings: [
      "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z",
      "M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z",
    ],
    home: ["m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z", "M9 22V12h6v10"],
    list: [
      "M3 12h.01",
      "M3 18h.01",
      "M3 6h.01",
      "M8 12h13",
      "M8 18h13",
      "M8 6h13",
    ],
    star: [
      "M12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26Z",
    ],
    filter: ["M22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3Z"],
    layers: [
      "M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z",
      "M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12",
      "M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17",
    ],
    search: ["M19 11a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z", "m21 21-4.3-4.3"],
    "arrow-up": ["m5 12 7-7 7 7", "M12 19V5"],
    "arrow-down": ["M12 5v14", "m19 12-7 7-7-7"],
    "arrow-right": ["M5 12h14", "m12 5 7 7-7 7"],
    "chevron-up": ["m18 15-6-6-6 6"],
    "chevron-down": ["m6 9 6 6 6-6"],
    "chevron-left": ["m15 18-6-6 6-6"],
    "chevron-right": ["m9 18 6-6-6-6"],
    "alert-triangle": [
      "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z",
      "M12 9v4",
      "M12 17h.01",
    ],
    "external-link": [
      "M15 3h6v6",
      "M10 14 21 3",
      "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6",
    ],
    pin: [
      "M12 17v5",
      "M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z",
    ],
    crosshair: [
      "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0Z",
      "M22 12h-4",
      "M6 12H2",
      "M12 6V2",
      "M12 22v-4",
    ],
    plus: ["M5 12h14", "M12 5v14"],
  };
</script>

<script lang="ts">
  export let name: IconName;
  export let size: number | string = 16;
  /** Stroke weight; Lucide's default is 2. */
  export let stroke = 2;
</script>

<svg
  class="icon"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false">
  {#each ICON_PATHS[name] as d}
    <path {d} />
  {/each}
</svg>

<style>
  .icon {
    display: block;
    flex: 0 0 auto;
  }
</style>
