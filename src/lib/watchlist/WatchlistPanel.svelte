<script lang="ts">
  import { onMount } from "svelte";
  import {
    addWatch,
    removeWatch,
    setWatchEnabled,
    listPresets,
  } from "../api/backend";
  import type { WatchKind, Preset } from "../api/types";
  import { watchEntries, refreshWatch } from "./watchStore";
  import { selectedHex, visibleAircraft, flyTo } from "../state";
  import Icon from "../ui/Icon.svelte";
  import Panel from "../ui/Panel.svelte";

  export let onClose: () => void;

  let tab: "list" | "presets" | "feed" = "list";
  let kind: WatchKind = "hex";
  let value = "";
  let label = "";

  let presets: Preset[] = [];

  onMount(async () => {
    await refreshWatch();
    presets = await listPresets().catch(() => []);
  });

  $: presetActive = new Set(
    $watchEntries.filter((w) => w.kind === "preset").map((w) => w.value),
  );

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

  async function togglePreset(p: Preset) {
    if (presetActive.has(p.id)) {
      const w = $watchEntries.find(
        (x) => x.kind === "preset" && x.value === p.id,
      );
      if (w) await removeWatch(w.id);
    } else {
      await addWatch("preset", p.id, p.label);
    }
    await refreshWatch();
  }

  $: feed = $visibleAircraft
    .filter((a) => a.military || a.interesting || a.pia || a.ladd)
    .slice(0, 60);

  function tags(a: {
    military: boolean;
    interesting: boolean;
    pia: boolean;
    ladd: boolean;
  }): string {
    const t: string[] = [];
    if (a.military) t.push("MIL");
    if (a.interesting) t.push("INT");
    if (a.pia) t.push("PIA");
    if (a.ladd) t.push("LADD");
    return t.join(" ");
  }

  function pick(hex: string, lat: number | null, lon: number | null) {
    selectedHex.set(hex);
    if (lat != null && lon != null) flyTo.set({ lat, lon });
  }
</script>

<Panel title="Watchlist" {onClose} width={340}>
  <div class="tabs">
    <button class:active={tab === "list"} on:click={() => (tab = "list")}>Watch</button>
    <button class:active={tab === "presets"} on:click={() => (tab = "presets")}>Presets</button>
    <button class:active={tab === "feed"} on:click={() => (tab = "feed")}>
      Feed{#if feed.length} ({feed.length}){/if}
    </button>
  </div>

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
      {#each $watchEntries.filter((w) => w.kind !== "preset") as w (w.id)}
        <li>
          <input
            type="checkbox"
            checked={w.enabled}
            on:change={(e) => toggle(w.id, e.currentTarget.checked)}
          />
          <span class="k">{w.kind}</span>
          <span class="v">{w.value}</span>
          {#if w.label}<span class="l">{w.label}</span>{/if}
          <button class="rm" on:click={() => remove(w.id)} aria-label="Remove"><Icon name="x" size={12} /></button>
        </li>
      {:else}
        <li class="muted">
          No custom watches. Emergency squawks (7500/7600/7700) always alert.
        </li>
      {/each}
    </ul>
  {:else if tab === "presets"}
    <ul>
      {#each presets as p}
        <li class="preset">
          <input
            type="checkbox"
            checked={presetActive.has(p.id)}
            on:change={() => togglePreset(p)}
          />
          <div>
            <div class="pl">{p.label}</div>
            <div class="pb">{p.blurb}</div>
          </div>
        </li>
      {/each}
    </ul>
  {:else if tab === "feed"}
    <ul class="feed">
      {#each feed as a (a.hex)}
        <li>
          <button class="link" on:click={() => pick(a.hex, a.lat, a.lon)}>
            {(a.flight ?? a.registration ?? a.hex).trim()}
          </button>
          <span class="ft">{a.typeCode ?? ""}</span>
          <span class="fl">{tags(a)}</span>
        </li>
      {:else}
        <li class="muted">No military / interesting aircraft in view.</li>
      {/each}
    </ul>
  {/if}

  <p class="hint">Past alerts now live in the Events &amp; history panel.</p>
</Panel>

<style>
  .tabs {
    display: flex;
    gap: 3px;
    flex-wrap: wrap;
    margin-bottom: 10px;
  }
  .tabs button {
    font-size: 11px;
    padding: 3px 7px;
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
    grid-column: 1 / -1;
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
  li.preset {
    align-items: flex-start;
  }
  .pl {
    font-weight: 600;
  }
  .pb {
    font-size: 10px;
    color: var(--text-dim);
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
    font-size: 11px;
  }
  .rm {
    margin-left: auto;
    border: none;
    background: transparent;
    display: inline-flex;
    align-items: center;
    color: var(--text-dim);
  }
  .rm:hover {
    color: var(--text);
  }
  .feed li {
    gap: 6px;
  }
  .feed .ft {
    color: var(--text-dim);
  }
  .feed .fl {
    margin-left: auto;
    font-size: 9px;
    color: var(--ok);
    letter-spacing: 0.03em;
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
  .hint {
    margin: 10px 0 0;
    font-size: 10px;
    color: var(--text-dim);
  }
</style>
