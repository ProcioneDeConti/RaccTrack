<script lang="ts">
  import { layers, primaryPlace } from "../state";
  import { updateSettings } from "../api/backend";
  import type { MapLayers } from "../api/types";
  import Panel from "../ui/Panel.svelte";
  import { AIRSPACE_STYLE, FLIGHT_CATEGORY_COLORS } from "../theme/colors";

  export let onClose: () => void;

  function toggle(key: keyof MapLayers) {
    layers.update((l) => {
      const next = { ...l, [key]: !l[key] };
      void updateSettings({ layers: next });
      return next;
    });
  }

  $: l = $layers;

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
      color: AIRSPACE_STYLE.CLASS_B.color,
      tip: "Class B — the busiest terminal airspace, around major airports (surface up to ~10,000 ft MSL). ATC clearance required to enter.",
    },
    {
      abbr: "C",
      name: "Class C",
      color: AIRSPACE_STYLE.CLASS_C.color,
      tip: "Class C — moderately busy terminal airspace with an operating control tower and radar. Two-way radio contact required before entry.",
    },
    {
      abbr: "D",
      name: "Class D",
      color: AIRSPACE_STYLE.CLASS_D.color,
      tip: "Class D — airspace around an airport with an operating control tower (typically to ~2,500 ft AGL). Two-way radio contact required.",
    },
    {
      abbr: "E",
      name: "Class E",
      color: AIRSPACE_STYLE.CLASS_E.color,
      tip: "Class E — controlled airspace that isn't A/B/C/D. No entry requirements for VFR flight; IFR needs a clearance.",
    },
    {
      abbr: "Mode C",
      name: "veil",
      color: AIRSPACE_STYLE.MODE_C.color,
      tip: "Mode C veil — 30 nm ring around a Class B airport within which a transponder with altitude reporting is required.",
    },
    {
      abbr: "MOA",
      name: "Military ops",
      color: AIRSPACE_STYLE.MOA.color,
      tip: "Military Operations Area — military training activity. VFR traffic is permitted; exercise extreme caution when active.",
    },
    {
      abbr: "R / P / W",
      name: "Restricted etc.",
      color: AIRSPACE_STYLE.RESTRICTED.color,
      tip: "Restricted, Prohibited, and Warning areas — hazards to aircraft (weapons, airspace security). Entry is restricted, forbidden, or advised against.",
    },
    {
      abbr: "A",
      name: "Alert",
      color: AIRSPACE_STYLE.ALERT.color,
      tip: "Alert Area — high volume of pilot training or unusual aerial activity. Not regulatory; all traffic shares responsibility for collision avoidance.",
    },
  ];
</script>

<Panel title="Map layers" {onClose} width={230}>
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
      <div class="lh u-eyebrow">Airspace (hover for detail)</div>
      {#each AS_LEGEND as x}
        <div class="lrow" title={x.tip}>
          <span class="sw" style="background:{x.color}"></span>
          <b>{x.abbr}</b><span class="ex">{x.name}</span>
        </div>
      {/each}
    </div>
  {/if}
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
  .sw {
    width: 9px;
    height: 9px;
    border-radius: 2px;
    display: inline-block;
    flex-shrink: 0;
  }
</style>
