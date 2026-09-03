<script lang="ts">
  import {
    total,
    lastUpdate,
    sourceStatus,
    aircraft,
    emergencyCount,
  } from "./state";

  let ageStr = "—";
  const tick = setInterval(() => {
    const t = $lastUpdate;
    ageStr = t ? `${Math.max(0, Math.round((Date.now() - t) / 1000))}s ago` : "—";
  }, 1000);
  import { onDestroy } from "svelte";
  onDestroy(() => clearInterval(tick));

  $: withPos = [...$aircraft.values()].filter((a) => a.lat !== null).length;
</script>

<footer class="statusbar">
  <span class="src" class:bad={$sourceStatus && !$sourceStatus.healthy}>
    ● {$sourceStatus?.activeSource ?? "connecting…"}
    {#if $sourceStatus && !$sourceStatus.healthy}(degraded){/if}
  </span>
  <span>{withPos} shown / {$total} in feed</span>
  <span>updated {ageStr}</span>
  {#if $sourceStatus}
    <span class="muted">{$sourceStatus.requestsLastMinute} req/min</span>
  {/if}
  {#if $emergencyCount > 0}
    <span class="emg">⚠ {$emergencyCount} emergency squawk{$emergencyCount > 1 ? "s" : ""} (NA)</span>
  {/if}
  <span class="spacer"></span>
  <span class="muted">North America coverage</span>
</footer>

<style>
  .statusbar {
    flex: 0 0 auto;
    height: 26px;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 0 12px;
    background: var(--bg-panel);
    border-top: 1px solid var(--border);
    font-size: 11px;
    z-index: 8;
  }
  .src {
    color: #7ee2b8;
  }
  .src.bad {
    color: var(--emergency);
  }
  .emg {
    color: var(--emergency);
    font-weight: 700;
  }
  .spacer {
    flex: 1;
  }
  .muted {
    color: var(--text-dim);
  }
</style>
