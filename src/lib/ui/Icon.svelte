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
    | "refresh-cw"
    | "pin"
    | "crosshair"
    | "plane"
    | "activity"
    | "info"
    | "book"
    | "map-pin"
    | "plane-takeoff"
    | "pencil"
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
    "refresh-cw": [
      "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8",
      "M21 3v5h-5",
      "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16",
      "M8 16H3v5",
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
    plane: [
      "M17.8 19.2 16 11l3.5-3.5C21 6 21.5 4 21 3c-1-.5-3 0-4.5 1.5L13 8 4.8 6.2c-.5-.1-.9.1-1.1.5l-.3.5c-.2.5-.1 1 .3 1.3L9 12l-2 3H4l-1 1 3 2 2 3 1-1v-3l3-2 3.5 5.3c.3.4.8.5 1.3.3l.5-.2c.4-.3.6-.7.5-1.2z",
    ],
    activity: ["M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"],
    info: ["M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z", "M12 16v-4", "M12 8h.01"],
    book: [
      "M12 7v14",
      "M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z",
    ],
    "map-pin": [
      "M20 10c0 4.993-5.539 10.193-7.399 11.799a1 1 0 0 1-1.202 0C9.539 20.193 4 14.993 4 10a8 8 0 0 1 16 0",
      "M15 10a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z",
    ],
    "plane-takeoff": [
      "M2 22h20",
      "M6.36 17.4 4 17l-2-4 1.1-.55a2 2 0 0 1 1.8 0l.17.1a2 2 0 0 0 1.8 0L8 12 5 6l.9-.45a2 2 0 0 1 2.09.2l4.02 3c.66.49 1.5.61 2.27.32l4.24-1.57a2 2 0 0 1 2.53 1.25 2 2 0 0 1-1.14 2.53l-14 5.14a2 2 0 0 1-1.24.06z",
    ],
    pencil: [
      "M12 20h9",
      "M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4Z",
    ],
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
