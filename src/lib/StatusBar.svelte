<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    total,
    lastUpdate,
    sourceStatus,
    shownCount,
    emergencyCount,
    atcStatus,
  } from "./state";
  import { atcStop, atcStartRecording, atcStopRecording } from "./api/backend";
  import { humanizeError } from "./ui/errors";
  import Icon from "./ui/Icon.svelte";

  let ageStr = "—";
  const tick = setInterval(() => {
    const t = $lastUpdate;
    ageStr = t ? `${Math.max(0, Math.round((Date.now() - t) / 1000))}s ago` : "—";
  }, 1000);
  onDestroy(() => clearInterval(tick));

  let recordError: string | null = null;

  async function toggleRecording() {
    recordError = null;
    try {
      if ($atcStatus?.recording) {
        await atcStopRecording();
      } else {
        await atcStartRecording();
      }
    } catch (e) {
      recordError = humanizeError(e);
    }
  }
</script>

<footer class="statusbar">
  <span class="src" class:bad={$sourceStatus && !$sourceStatus.healthy}>
    <span class="dot"></span>
    {$sourceStatus?.activeSources?.length ? $sourceStatus.activeSources.join(" + ") : "connecting…"}
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
  {#if $atcStatus?.running || $atcStatus?.lastError}
    <span class="atc">
      {#if $atcStatus.lastError}
        <span class="dot bad"></span>
        ATC: {humanizeError($atcStatus.lastError)}
        <button class="atc-stop" on:click={() => void atcStop()}>Dismiss</button>
      {:else if $atcStatus.retuning}
        <span class="dot retuning"></span>
        Retuning…
        <button class="atc-stop" on:click={() => void atcStop()}>Stop</button>
      {:else}
        <span class="dot" class:open={$atcStatus.squelchOpen}></span>
        {$atcStatus.scanning ? "Scanning, on" : "Listening"}
        {$atcStatus.tunedMhz?.toFixed(3)} MHz
        {#if $atcStatus.adsbPaused}(ADS-B paused){/if}
        <button
          class="atc-rec"
          class:on={$atcStatus.recording}
          title={$atcStatus.recording ? "Stop recording" : "Record session to WAV"}
          on:click={() => void toggleRecording()}
        >
          <span class="rec-dot"></span>
        </button>
        <button class="atc-stop" on:click={() => void atcStop()}>Stop</button>
      {/if}
      {#if recordError}<span class="rec-err">{recordError}</span>{/if}
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
  .atc {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .atc .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-dim);
    flex: 0 0 auto;
  }
  .atc .dot.open {
    background: var(--ok);
  }
  .atc .dot.bad {
    background: var(--emergency);
  }
  .atc .dot.retuning {
    background: var(--accent, var(--text-dim));
    animation: pulse 0.9s ease-in-out infinite;
  }
  @keyframes pulse {
    0%, 100% {
      opacity: 1;
    }
    50% {
      opacity: 0.25;
    }
  }
  .atc-stop {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text-dim);
    border-radius: var(--radius-sm, 4px);
    padding: 1px 6px;
    font-size: 10px;
    cursor: pointer;
  }
  .atc-stop:hover {
    color: var(--text);
  }
  .atc-rec {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text-dim);
    border-radius: 50%;
    width: 17px;
    height: 17px;
    padding: 0;
    cursor: pointer;
  }
  .atc-rec:hover {
    color: var(--text);
  }
  .atc-rec.on {
    color: #fff;
    background: var(--emergency);
    border-color: var(--emergency);
  }
  .rec-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: currentColor;
  }
  .rec-err {
    color: var(--emergency);
  }
  .spacer {
    flex: 1;
  }
  .muted {
    color: var(--text-dim);
  }
</style>
