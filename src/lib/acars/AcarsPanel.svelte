<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import Panel from "../ui/Panel.svelte";
  import Icon from "../ui/Icon.svelte";
  import Message from "../ui/Message.svelte";
  import { acarsStatus } from "../state";
  import {
    getSettings,
    acarsStart,
    acarsScan,
    acarsStop,
    getAcarsStatus,
    acarsMessages,
    acarsClearMessages,
  } from "../api/backend";
  import type { AcarsMessage } from "../api/types";
  import { humanizeError } from "../ui/errors";

  export let onClose: () => void;

  let freqs: number[] = [];
  let selected = 0;
  let messages: AcarsMessage[] = [];
  let loading = true;
  let actionError: string | null = null;

  async function load() {
    try {
      const [s, msgs] = await Promise.all([getSettings(), acarsMessages()]);
      freqs = s.acarsFreqs;
      selected = freqs[0] ?? 131.55;
      messages = msgs;
    } finally {
      loading = false;
    }
  }
  onMount(load);

  // While the panel is open, pick up newly-decoded messages without the
  // user needing to reopen it — the backend is just a poll target (see
  // AcarsStatus.messageCount), so this mirrors that polling shape at the
  // same 1s cadence, not a push channel.
  let msgTimer: ReturnType<typeof setInterval>;
  onMount(() => {
    msgTimer = setInterval(async () => {
      if ($acarsStatus?.messageCount !== messages.length) {
        messages = await acarsMessages();
      }
    }, 1000);
  });
  onDestroy(() => clearInterval(msgTimer));

  async function toggleListen() {
    actionError = null;
    if ($acarsStatus?.running && !$acarsStatus.scanning) {
      await acarsStop();
      return;
    }
    try {
      const s = await getSettings();
      await acarsStart(selected, s.acarsDeviceIndex);
      acarsStatus.set(await getAcarsStatus());
    } catch (e) {
      actionError = humanizeError(e);
    }
  }

  async function toggleScanAll() {
    actionError = null;
    if ($acarsStatus?.running && $acarsStatus.scanning) {
      await acarsStop();
      return;
    }
    if (freqs.length < 2) return;
    try {
      const s = await getSettings();
      await acarsScan(freqs, s.acarsDeviceIndex);
      acarsStatus.set(await getAcarsStatus());
    } catch (e) {
      actionError = humanizeError(e);
    }
  }

  async function wipe() {
    await acarsClearMessages();
    messages = [];
  }

  const time = (ms: number) =>
    new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
</script>

<Panel title="ACARS" {onClose} width={360} bodyPad={false}>
  <div class="controls">
    <div class="row">
      <select bind:value={selected} disabled={$acarsStatus?.scanning}>
        {#each freqs as f}
          <option value={f}>{f.toFixed(3)} MHz</option>
        {/each}
      </select>
      <button class="btn-link" on:click={toggleListen} disabled={$acarsStatus?.scanning}>
        {$acarsStatus?.running && !$acarsStatus.scanning ? "Stop" : "Listen"}
      </button>
      <button
        class="btn-link"
        on:click={toggleScanAll}
        disabled={($acarsStatus?.running && !$acarsStatus.scanning) || freqs.length < 2}
      >
        {$acarsStatus?.scanning ? "Stop scan" : "Scan all"}
      </button>
    </div>
    {#if $acarsStatus?.running || $acarsStatus?.lastError || actionError}
      <div class="status">
        {#if actionError}
          <span class="err"><Icon name="alert-triangle" size={11} /> {actionError}</span>
        {:else if $acarsStatus?.lastError}
          <span class="err">
            <Icon name="alert-triangle" size={11} /> {humanizeError($acarsStatus.lastError)}
          </span>
        {:else if $acarsStatus?.retuning}
          <span class="dot retuning"></span> Retuning…
        {:else}
          <span class="dot" class:open={$acarsStatus?.squelchOpen}></span>
          {$acarsStatus?.scanning ? "Scanning, on" : "Listening"}
          {$acarsStatus?.tunedMhz?.toFixed(3)} MHz
          {#if $acarsStatus?.adsbPaused}(ADS-B paused){/if}
        {/if}
      </div>
    {/if}
  </div>

  <div class="scroll">
    {#if loading && messages.length === 0}
      <Message kind="loading">Loading…</Message>
    {:else if messages.length === 0}
      <Message kind="empty">
        No ACARS messages decoded yet — start listening on a frequency above.
      </Message>
    {:else}
      <ul>
        {#each messages as m, i (i)}
          <li>
            <div class="head">
              <span class="tail">{m.tail || "?"}</span>
              <span class="label">{m.label}</span>
              <span class="freq">{m.freqMhz.toFixed(3)}</span>
              <time>{time(m.timestampMs)}</time>
            </div>
            {#if m.text}<div class="text">{m.text}</div>{/if}
            {#if !m.bccOk}
              <div class="warn" title="Block check didn't match — field boundaries may be off">
                <Icon name="alert-triangle" size={10} /> unverified
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <svelte:fragment slot="actions">
    {#if messages.length > 0}
      <button class="btn-link" on:click={wipe}>Clear</button>
    {/if}
  </svelte:fragment>
</Panel>

<style>
  .controls {
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .row select {
    flex: 1 1 auto;
    min-width: 0;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .status .err {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--emergency);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-dim);
    flex: 0 0 auto;
  }
  .dot.open {
    background: var(--ok);
  }
  .dot.retuning {
    background: var(--accent);
  }
  .scroll {
    padding: 4px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    padding: 6px 8px;
    border-radius: 5px;
    font-size: 11px;
  }
  li:hover {
    background: var(--bg-elev);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .tail {
    font-weight: 600;
    font-family: ui-monospace, monospace;
    color: var(--text);
  }
  .label {
    color: var(--text-dim);
    font-family: ui-monospace, monospace;
  }
  .freq {
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  time {
    margin-left: auto;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .text {
    margin-top: 2px;
    color: var(--text);
    word-break: break-word;
  }
  .warn {
    margin-top: 2px;
    display: flex;
    align-items: center;
    gap: 3px;
    color: var(--text-dim);
    font-size: 10px;
  }
</style>
