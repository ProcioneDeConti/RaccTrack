<script lang="ts">
  import { layers, home } from "../state";
  import { updateSettings } from "../api/backend";
  import type { MapLayers } from "../api/types";
  import Icon from "../ui/Icon.svelte";
  import { AIRSPACE_STYLE, FLIGHT_CATEGORY_COLORS } from "../theme/colors";

  let open = false;

  function toggle(key: keyof MapLayers) {
    layers.update((l) => {
      const next = { ...l, [key]: !l[key] };
      void updateSettings({ layers: next });
      return next;
    });
  }

  $: l = $layers;
  $: anyOn = l.airports || l.weather || l.airspace || l.rangeRings;

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

<div class="layers">
  <button class="mapbtn" class:active={anyOn} on:click={() => (open = !open)} title="Map layers">
    <Icon name="layers" size={14} />
    Layers
  </button>
  {#if open}
    <div class="menu">
      <label><input type="checkbox" checked={l.airports} on:change={() => toggle("airports")} /> Airports</label>
      <label><input type="checkbox" checked={l.weather} on:change={() => toggle("weather")} /> Weather (METAR)</label>
      <label><input type="checkbox" checked={l.airspace} on:change={() => toggle("airspace")} /> Airspace</label>
      <label
        class:disabled={!$home}
        title={$home ? "" : "Set a home location first"}
      >
        <input
          type="checkbox"
          checked={l.rangeRings}
          disabled={!$home}
          on:change={() => toggle("rangeRings")}
        /> Range rings
      </label>

      {#if l.weather}
        <div class="legend">
          <div class="lh">Flight category (hover for detail)</div>
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
          <div class="lh">Airspace (hover for detail)</div>
          {#each AS_LEGEND as x}
            <div class="lrow" title={x.tip}>
              <span class="sw" style="background:{x.color}"></span>
              <b>{x.abbr}</b><span class="ex">{x.name}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .layers {
    position: absolute;
    top: 14px;
    left: 150px;
    z-index: 10;
  }
  .mapbtn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .menu {
    margin-top: 6px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 170px;
  }
  label {
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  label.disabled {
    opacity: 0.5;
  }
  .legend {
    border-top: 1px solid var(--border);
    padding-top: 5px;
    margin-top: 2px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .lh {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-dim);
    margin-bottom: 1px;
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
