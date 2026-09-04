<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    total,
    lastUpdate,
    sourceStatus,
    shownCount,
    emergencyCount,
  } from "./state";
  import Icon from "./ui/Icon.svelte";

  let ageStr = "—";
  const tick = setInterval(() => {
    const t = $lastUpdate;
    ageStr = t ? `${Math.max(0, Math.round((Date.now() - t) / 1000))}s ago` : "—";
  }, 1000);
  onDestroy(() => clearInterval(tick));
</script>

<footer class="statusbar">
  <span class="src" class:bad={$sourceStatus && !$sourceStatus.healthy}>
    <span class="dot"></span>
    {$sourceStatus?.activeSource ?? "connecting…"}
    {#if $sourceStatus && !$sourceStatus.healthy}(degraded){/if}
  </span>
  <span>{$shownCount} shown / {$total} in feed</span>
  <span>updated {ageStr}</span>
  {#if $sourceStatus}
    <span class="muted">{$sourceStatus.requestsLastMinute} req/min</span>
  {/if}
  {#if $emergencyCount > 0}
    <span class="emg">
      <Icon name="alert-triangle" size={12} />
      {$emergencyCount} emergency squawk{$emergencyCount > 1 ? "s" : ""} (NA)
    </span>
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
    color: var(--ok);
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .src .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
    flex: 0 0 auto;
  }
  .src.bad {
    color: var(--emergency);
  }
  .emg {
    color: var(--emergency);
    font-weight: 700;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .spacer {
    flex: 1;
  }
  .muted {
    color: var(--text-dim);
  }
</style>
