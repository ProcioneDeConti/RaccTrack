<script lang="ts">
  import { onMount } from "svelte";
  import {
    addWatch,
    removeWatch,
    setWatchEnabled,
  } from "../api/backend";
  import type { WatchKind } from "../api/types";
  import { watchEntries, alertLog, refreshWatch } from "./watchStore";
  import { selectedHex } from "../state";

  export let onClose: () => void;

  let tab: "list" | "log" = "list";
  let kind: WatchKind = "hex";
  let value = "";
  let label = "";

  onMount(refreshWatch);

  async function add() {
    const v = value.trim();
    if (!v) return;
    await addWatch(kind, v, label.trim() || null);
    value = "";
    label = "";
    await refreshWatch();
  }

  async function remove(id: number) {
    await removeWatch(id);
    await refreshWatch();
  }

  async function toggle(id: number, enabled: boolean) {
    await setWatchEnabled(id, enabled);
    await refreshWatch();
  }
</script>

<aside class="panel">
  <header>
    <div class="tabs">
      <button class:active={tab === "list"} on:click={() => (tab = "list")}>Watchlist</button>
      <button class:active={tab === "log"} on:click={() => (tab = "log")}>
        Alerts {#if $alertLog.length}({$alertLog.length}){/if}
      </button>
    </div>
    <button class="close" on:click={onClose}>✕</button>
  </header>

  {#if tab === "list"}
    <form class="add" on:submit|preventDefault={add}>
      <select bind:value={kind}>
        <option value="hex">ICAO hex</option>
        <option value="registration">Registration</option>
        <option value="type">Type code</option>
        <option value="callsign">Callsign</option>
      </select>
      <input placeholder="value" bind:value />
      <input placeholder="label (optional)" bind:value={label} />
      <button type="submit">Add</button>
    </form>

    <ul>
      {#each $watchEntries as w (w.id)}
        <li>
          <input
            type="checkbox"
            checked={w.enabled}
            on:change={(e) => toggle(w.id, e.currentTarget.checked)}
          />
          <span class="k">{w.kind}</span>
          <span class="v">{w.value}</span>
          {#if w.label}<span class="l">{w.label}</span>{/if}
          <button class="rm" on:click={() => remove(w.id)}>✕</button>
        </li>
      {:else}
        <li class="muted">
          No watches. Emergency squawks (7500/7600/7700) always alert.
        </li>
      {/each}
    </ul>
  {:else}
    <ul class="log">
      {#each $alertLog as a}
        <li class:emg={a.emergency}>
          <button class="link" on:click={() => selectedHex.set(a.hex)}>{a.hex}</button>
          <span>{a.reason}</span>
          <time>{new Date(a.at).toLocaleTimeString()}</time>
        </li>
      {:else}
        <li class="muted">No alerts yet.</li>
      {/each}
    </ul>
  {/if}
</aside>

<style>
  .panel {
    position: absolute;
    top: 14px;
    left: 52px;
    width: 320px;
    max-height: 70vh;
    overflow-y: auto;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 12px;
    z-index: 11;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
  }
  .tabs {
    display: flex;
    gap: 4px;
  }
  .close {
    border: none;
    background: transparent;
  }
  .add {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
    margin-bottom: 8px;
  }
  .add select,
  .add input {
    min-width: 0;
  }
  .add button {
    grid-column: span 2;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .k {
    color: var(--text-dim);
    text-transform: uppercase;
    font-size: 10px;
  }
  .v {
    font-family: ui-monospace, monospace;
  }
  .l {
    color: var(--text-dim);
  }
  .rm {
    margin-left: auto;
    border: none;
    background: transparent;
  }
  .log li {
    flex-wrap: wrap;
  }
  .log time {
    margin-left: auto;
    color: var(--text-dim);
  }
  .log li.emg .link {
    color: var(--emergency);
  }
  .link {
    border: none;
    background: transparent;
    color: var(--accent);
    padding: 0;
    font-family: ui-monospace, monospace;
  }
  .muted {
    color: var(--text-dim);
  }
</style>
