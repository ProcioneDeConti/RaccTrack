<script lang="ts">
  import Panel from "../ui/Panel.svelte";
  import Icon from "../ui/Icon.svelte";
  import {
    aircraft,
    primaryPlace,
    selectedHex,
    flyTo,
    upcomingPasses,
  } from "../state";
  import { passHorizonMin, passRadiusNm, type PredictedPass } from "../passes";
  import { compass, fmtDistanceNm } from "../geo";
  import { altitude } from "../format";

  export let onClose: () => void;

  function countdown(sec: number): string {
    if (sec < 90) return `in ${Math.round(sec)}s`;
    return `in ${Math.round(sec / 60)}m`;
  }

  function lightLabel(l: PredictedPass["light"]): string {
    const lit =
      l.lit === "n/a"
        ? ""
        : ` · ${l.lit === "back" ? "back-lit" : l.lit === "front" ? "front-lit" : "side-lit"}`;
    return `${l.phase}${lit}`;
  }

  function pick(p: PredictedPass) {
    selectedHex.set(p.hex);
    const ac = $aircraft.get(p.hex);
    if (ac?.lat != null && ac?.lon != null)
      flyTo.set({ lat: ac.lat, lon: ac.lon });
  }

  const num = (v: string, fallback: number) => {
    const n = parseFloat(v);
    return Number.isFinite(n) ? n : fallback;
  };
</script>

<Panel title="Passes overhead" {onClose} width={340}>
  {#if !$primaryPlace}
    <p class="muted">
      Set a primary place in <strong>Places &amp; alerts</strong> (the star) and
      RaccTrack will show which tracked aircraft are about to fly near it — when,
      how close, at what angle, and in what light.
    </p>
  {:else}
    <div class="head">
      <span class="place" title={$primaryPlace.label}>
        <Icon name="map-pin" size={12} />
        {$primaryPlace.label}
      </span>
    </div>

    <div class="controls">
      <label>
        within
        <input
          type="number"
          min="1"
          max="60"
          step="1"
          value={$passRadiusNm}
          on:change={(e) =>
            passRadiusNm.set(
              Math.min(60, Math.max(1, num(e.currentTarget.value, 15))),
            )}
        /> nm
      </label>
      <label>
        next
        <input
          type="number"
          min="2"
          max="30"
          step="1"
          value={$passHorizonMin}
          on:change={(e) =>
            passHorizonMin.set(
              Math.min(30, Math.max(2, num(e.currentTarget.value, 12))),
            )}
        /> min
      </label>
    </div>

    {#if $upcomingPasses.length === 0}
      <p class="muted">
        Nothing predicted within {$passRadiusNm} nm in the next {$passHorizonMin}
        min. Aircraft need a position, heading, and speed to be projected.
      </p>
    {:else}
      <ul>
        {#each $upcomingPasses as p (p.hex)}
          <li>
            <button class="row" on:click={() => pick(p)}>
              <div class="line1">
                <span class="cs">{p.callsign}</span>
                {#if p.typeCode}<span class="ty">{p.typeCode}</span>{/if}
                {#if p.military}<span class="tag mil">MIL</span>{/if}
                {#if p.emergency}<span class="tag emg">EMG</span>{/if}
                <span class="eta">{countdown(p.inSec)}</span>
              </div>
              <div class="line2">
                <span>{fmtDistanceNm(p.minDistanceNm)}</span>
                <span class="sep">·</span>
                <span
                  >{compass(p.bearingDeg)} {Math.round(p.bearingDeg)}° · {Math.round(
                    p.elevationDeg,
                  )}° up</span
                >
                <span class="sep">·</span>
                <span>{altitude(p.altBaroFt)}</span>
              </div>
              <div class="light" data-phase={p.light.phase}>
                {lightLabel(p.light)}
              </div>
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    <p class="caveat">
      Straight-line estimate from current track and speed — aircraft that turn
      (especially on approach) won't match.
    </p>
  {/if}
</Panel>

<style>
  .head {
    display: flex;
    align-items: center;
    margin-bottom: 8px;
  }
  .place {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-weight: 600;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .controls {
    display: flex;
    gap: 12px;
    margin-bottom: 10px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .controls label {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .controls input {
    width: 48px;
  }
  .muted {
    color: var(--text-dim);
    font-size: 12px;
    line-height: 1.5;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .row {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 3px;
    text-align: left;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: transparent;
    padding: 6px 8px;
  }
  .row:hover {
    background: var(--bg-elev);
  }
  .line1 {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .cs {
    font-weight: 600;
    font-size: 12px;
  }
  .ty {
    font-size: 10px;
    color: var(--text-dim);
  }
  .eta {
    margin-left: auto;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: var(--accent);
  }
  .line2 {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    font-size: 10px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .sep {
    opacity: 0.5;
  }
  .light {
    font-size: 10px;
    text-transform: capitalize;
    color: var(--text-dim);
  }
  .light[data-phase="day"] {
    color: var(--text-dim);
  }
  .light[data-phase="golden"] {
    color: #e3a008;
  }
  .light[data-phase="blue"] {
    color: #6ea8fe;
  }
  .light[data-phase="night"] {
    color: var(--text-dim);
    opacity: 0.7;
  }
  .tag {
    font-size: 9px;
    font-weight: 700;
    padding: 0 4px;
    border-radius: 3px;
  }
  .tag.mil {
    background: #274d3d;
    color: #7ee2b8;
  }
  .tag.emg {
    background: var(--emergency);
    color: #fff;
  }
  .caveat {
    margin: 10px 0 0;
    font-size: 10px;
    color: var(--text-dim);
    line-height: 1.4;
  }
</style>
