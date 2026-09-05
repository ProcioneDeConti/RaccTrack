<script lang="ts">
  import { layers, primaryPlace, mapColors, coverageEnabled } from "../state";
  import { updateSettings } from "../api/backend";
  import type { FillPattern, MapColors, MapLayers } from "../api/types";
  import Panel from "../ui/Panel.svelte";
  import {
    AIRSPACE_STYLE,
    FLIGHT_CATEGORY_COLORS,
    GEOFENCE_LINE_DEFAULT,
    GEOFENCE_FILL_DEFAULT,
    COVERAGE_LINE_DEFAULT,
    COVERAGE_FILL_DEFAULT,
  } from "../theme/colors";

  export let onClose: () => void;

  function toggle(key: keyof MapLayers) {
    layers.update((l) => {
      const next = { ...l, [key]: !l[key] };
      void updateSettings({ layers: next });
      return next;
    });
  }

  $: l = $layers;

  const PATTERNS: { value: FillPattern; label: string }[] = [
    { value: "solid", label: "Solid" },
    { value: "stripe", label: "Stripe" },
    { value: "hash", label: "Crosshatch" },
    { value: "dot", label: "Dots" },
    { value: "check", label: "Checkerboard" },
  ];

  async function saveColors(next: MapColors) {
    const s = await updateSettings({ colors: next });
    mapColors.set(s.colors);
  }
  function setAirspaceColor(keys: string[], hex: string) {
    const airspace = { ...$mapColors.airspace };
    for (const k of keys) airspace[k] = hex;
    void saveColors({ ...$mapColors, airspace });
  }
  // Plain functions, not an inline `as FillPattern` cast in the template —
  // Svelte's template-expression parser doesn't accept TS type assertions
  // there the way a `lang="ts"` script block does.
  function setGeofencePattern(v: string) {
    void saveColors({ ...$mapColors, geofencePattern: v as FillPattern });
  }
  function setCoveragePattern(v: string) {
    void saveColors({ ...$mapColors, coveragePattern: v as FillPattern });
  }

  function resetColors() {
    void saveColors({
      airspace: {},
      geofenceFill: null,
      geofenceLine: null,
      geofencePattern: null,
      coverageFill: null,
      coverageLine: null,
      coveragePattern: null,
    });
  }

  // `color` is pulled from the shared theme palette (src/lib/theme/colors.ts);
  // only the label/tooltip copy lives here.
  const WX_LEGEND = [
    {
      abbr: "VFR",
      name: "Visual",
      color: FLIGHT_CATEGORY_COLORS.VFR,
      tip: "Visual Flight Rules — ceiling greater than 3,000 ft AGL and visibility greater than 5 statute miles.",
    },
    {
      abbr: "MVFR",
      name: "Marginal",
      color: FLIGHT_CATEGORY_COLORS.MVFR,
      tip: "Marginal VFR — ceiling 1,000–3,000 ft AGL and/or visibility 3–5 statute miles.",
    },
    {
      abbr: "IFR",
      name: "Instrument",
      color: FLIGHT_CATEGORY_COLORS.IFR,
      tip: "Instrument Flight Rules — ceiling 500–1,000 ft AGL and/or visibility 1–3 statute miles.",
    },
    {
      abbr: "LIFR",
      name: "Low IFR",
      color: FLIGHT_CATEGORY_COLORS.LIFR,
      tip: "Low IFR — ceiling below 500 ft AGL and/or visibility below 1 statute mile.",
    },
  ];

  const AS_LEGEND = [
    {
      abbr: "B",
      name: "Class B",
      keys: ["CLASS_B"],
      tip: "Class B — the busiest terminal airspace, around major airports (surface up to ~10,000 ft MSL). ATC clearance required to enter.",
    },
    {
      abbr: "C",
      name: "Class C",
      keys: ["CLASS_C"],
      tip: "Class C — moderately busy terminal airspace with an operating control tower and radar. Two-way radio contact required before entry.",
    },
    {
      abbr: "D",
      name: "Class D",
      keys: ["CLASS_D"],
      tip: "Class D — airspace around an airport with an operating control tower (typically to ~2,500 ft AGL). Two-way radio contact required.",
    },
    {
      abbr: "E",
      name: "Class E",
      keys: ["CLASS_E"],
      tip: "Class E — controlled airspace that isn't A/B/C/D. No entry requirements for VFR flight; IFR needs a clearance.",
    },
    {
      abbr: "Mode C",
      name: "veil",
      keys: ["MODE_C"],
      tip: "Mode C veil — 30 nm ring around a Class B airport within which a transponder with altitude reporting is required.",
    },
    {
      abbr: "MOA",
      name: "Military ops",
      keys: ["MOA"],
      tip: "Military Operations Area — military training activity. VFR traffic is permitted; exercise extreme caution when active.",
    },
    {
      abbr: "R / P / W",
      name: "Restricted etc.",
      keys: ["RESTRICTED", "PROHIBITED", "WARNING"],
      tip: "Restricted, Prohibited, and Warning areas — hazards to aircraft (weapons, airspace security). Entry is restricted, forbidden, or advised against.",
    },
    {
      abbr: "A",
      name: "Alert",
      keys: ["ALERT"],
      tip: "Alert Area — high volume of pilot training or unusual aerial activity. Not regulatory; all traffic shares responsibility for collision avoidance.",
    },
  ];
</script>

<Panel title="Map layers" {onClose} width={230}>
  <label>
    <input type="checkbox" checked={l.aircraft} on:change={() => toggle("aircraft")} /> Aircraft
  </label>
  <label><input type="checkbox" checked={l.airports} on:change={() => toggle("airports")} /> Airports</label>
  <label><input type="checkbox" checked={l.weather} on:change={() => toggle("weather")} /> Weather (METAR)</label>
  <label><input type="checkbox" checked={l.radar} on:change={() => toggle("radar")} /> Weather radar</label>
  <label><input type="checkbox" checked={l.airspace} on:change={() => toggle("airspace")} /> Airspace</label>
  <label class:disabled={!$primaryPlace} title={$primaryPlace ? "" : "Add a place first"}>
    <input
      type="checkbox"
      checked={l.rangeRings}
      disabled={!$primaryPlace}
      on:change={() => toggle("rangeRings")}
    /> Range rings
  </label>

  {#if l.weather}
    <div class="legend">
      <div class="lh u-eyebrow">Flight category (hover for detail)</div>
      {#each WX_LEGEND as x}
        <div class="lrow" title={x.tip}>
          <span class="sw" style="background:{x.color}"></span>
          <b>{x.abbr}</b><span class="ex">{x.name}</span>
        </div>
      {/each}
    </div>
  {/if}
  {#if l.airspace}
    <div class="legend">
      <div class="lh u-eyebrow">Airspace (hover for detail, click swatch to recolor)</div>
      {#each AS_LEGEND as x}
        <div class="lrow" title={x.tip}>
          <input
            type="color"
            class="sw"
            value={$mapColors.airspace[x.keys[0]] ?? AIRSPACE_STYLE[x.keys[0]].color}
            on:input={(e) => setAirspaceColor(x.keys, e.currentTarget.value)}
          />
          <b>{x.abbr}</b><span class="ex">{x.name}</span>
        </div>
      {/each}
    </div>
  {/if}

  <div class="legend">
    <div class="lh u-eyebrow">Geofence (place alerts)</div>
    <div class="lrow">
      <input
        type="color"
        class="sw"
        value={$mapColors.geofenceLine ?? GEOFENCE_LINE_DEFAULT}
        on:input={(e) => saveColors({ ...$mapColors, geofenceLine: e.currentTarget.value })}
      />
      <span class="ex">Outline</span>
    </div>
    <div class="lrow">
      <input
        type="color"
        class="sw"
        value={$mapColors.geofenceFill ?? GEOFENCE_FILL_DEFAULT}
        on:input={(e) => saveColors({ ...$mapColors, geofenceFill: e.currentTarget.value })}
      />
      <span class="ex">Fill</span>
      <select
        class="pattern"
        value={$mapColors.geofencePattern ?? "solid"}
        on:change={(e) => setGeofencePattern(e.currentTarget.value)}
      >
        {#each PATTERNS as p}<option value={p.value}>{p.label}</option>{/each}
      </select>
    </div>
  </div>

  {#if $coverageEnabled}
    <div class="legend">
      <div class="lh u-eyebrow">Reception coverage estimate</div>
      <div class="lrow">
        <input
          type="color"
          class="sw"
          value={$mapColors.coverageLine ?? COVERAGE_LINE_DEFAULT}
          on:input={(e) => saveColors({ ...$mapColors, coverageLine: e.currentTarget.value })}
        />
        <span class="ex">Outline</span>
      </div>
      <div class="lrow">
        <input
          type="color"
          class="sw"
          value={$mapColors.coverageFill ?? COVERAGE_FILL_DEFAULT}
          on:input={(e) => saveColors({ ...$mapColors, coverageFill: e.currentTarget.value })}
        />
        <span class="ex">Fill</span>
        <select
          class="pattern"
          value={$mapColors.coveragePattern ?? "solid"}
          on:change={(e) => setCoveragePattern(e.currentTarget.value)}
        >
          {#each PATTERNS as p}<option value={p.value}>{p.label}</option>{/each}
        </select>
      </div>
    </div>
  {/if}

  <button type="button" class="reset-colors" on:click={resetColors}>Reset colors to default</button>
</Panel>

<style>
  label {
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 5px 0;
  }
  label.disabled {
    opacity: 0.5;
  }
  .legend {
    border-top: 1px solid var(--border);
    padding-top: 6px;
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .lh {
    margin-bottom: 2px;
  }
  .lrow {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    cursor: help;
  }
  .lrow b {
    min-width: 44px;
    font-weight: 700;
  }
  .lrow .ex {
    color: var(--text-dim);
  }
  input.sw {
    width: 14px;
    height: 14px;
    border-radius: 2px;
    display: inline-block;
    flex-shrink: 0;
    padding: 0;
    border: none;
    background: none;
    cursor: pointer;
  }
  input.sw::-webkit-color-swatch-wrapper {
    padding: 0;
  }
  input.sw::-webkit-color-swatch {
    border: none;
    border-radius: 2px;
  }
  select.pattern {
    margin-left: auto;
    font-size: 9px;
    padding: 1px 3px;
    max-width: 90px;
  }
  .reset-colors {
    margin-top: 8px;
    border: none;
    background: transparent;
    color: var(--text-dim);
    font-size: 10px;
    text-decoration: underline;
    padding: 0;
  }
  .reset-colors:hover {
    color: var(--text);
  }
</style>
