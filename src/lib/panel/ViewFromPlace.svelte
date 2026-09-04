<script lang="ts">
  import type { Aircraft } from "../api/types";
  import { primaryPlace } from "../state";
  import { viewFromPlace } from "../passes";
  import { compass, fmtDistanceNm } from "../geo";

  export let live: Aircraft | null;

  $: view =
    live && $primaryPlace ? viewFromPlace(live, $primaryPlace) : null;
</script>

{#if $primaryPlace && view}
  <section class="panel-section">
    <h4>View from {$primaryPlace.label}</h4>
    <div class="grid">
      <div class="cell">
        <div class="v">{compass(view.bearingDeg)} {Math.round(view.bearingDeg)}°</div>
        <div class="l">bearing</div>
      </div>
      <div class="cell">
        <div class="v">{Math.round(view.elevationDeg)}° up</div>
        <div class="l">elevation</div>
      </div>
      <div class="cell">
        <div class="v">{fmtDistanceNm(view.distanceNm)}</div>
        <div class="l">{view.closing ? "closing" : "opening"}</div>
      </div>
    </div>
  </section>
{/if}

<style>
  .grid {
    display: flex;
    gap: 8px;
  }
  .cell {
    flex: 1 1 0;
    min-width: 0;
    text-align: center;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 4px;
  }
  .v {
    font-size: 13px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .l {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-dim);
    margin-top: 2px;
  }
</style>
