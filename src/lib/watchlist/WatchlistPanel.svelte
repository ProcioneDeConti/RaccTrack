<script lang="ts">
  import { onMount } from "svelte";
  import {
    addWatch,
    removeWatch,
    setWatchEnabled,
    listPresets,
    listGeofences,
    addGeofence,
    removeGeofence,
    setGeofenceEnabled,
  } from "../api/backend";
  import type { WatchKind, Preset, Geofence } from "../api/types";
  import { watchEntries, alertLog, refreshWatch } from "./watchStore";
  import {
    selectedHex,
    visibleAircraft,
    home,
    flyTo,
    geofences as geofenceStore,
  } from "../state";

  export let onClose: () => void;

  let tab: "list" | "presets" | "feed" | "fences" | "log" = "list";
  let kind: WatchKind = "hex";
  let value = "";
  let label = "";

  let presets: Preset[] = [];
  let fences: Geofence[] = [];

  // new fence form
  let fLabel = "";
  let fRadius = 15;
  let fMaxAlt = "";
  let fMil = false;

  onMount(async () => {
    await refreshWatch();
    presets = await listPresets().catch(() => []);
    fences = await listGeofences().catch(() => []);
    geofenceStore.set(fences);
  });

  $: geofenceStore.set(fences);

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

  async function createFence() {
    if (!fLabel.trim() || !$home) return;
    const g = await addGeofence({
      label: fLabel.trim(),
      lat: $home.lat,
      lon: $home.lon,
      radiusNm: fRadius,
      maxAltFt: fMaxAlt.trim() ? Number(fMaxAlt) : null,
      milOnly: fMil,
      enabled: true,
    });
    fences = [...fences, g];
    fLabel = "";
    fMaxAlt = "";
  }
  async function delFence(id: number) {
    await removeGeofence(id);
    fences = fences.filter((f) => f.id !== id);
  }
  async function toggleFence(f: Geofence) {
    await setGeofenceEnabled(f.id, !f.enabled);
    fences = fences.map((x) => (x.id === f.id ? { ...x, enabled: !x.enabled } : x));
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

<aside class="panel">
  <header>
    <div class="tabs">
      <button class:active={tab === "list"} on:click={() => (tab = "list")}>Watch</button>
      <button class:active={tab === "presets"} on:click={() => (tab = "presets")}>Presets</button>
      <button class:active={tab === "feed"} on:click={() => (tab = "feed")}>
        Feed{#if feed.length} ({feed.length}){/if}
      </button>
      <button class:active={tab === "fences"} on:click={() => (tab = "fences")}>Fences</button>
      <button class:active={tab === "log"} on:click={() => (tab = "log")}>
        Alerts{#if $alertLog.length} ({$alertLog.length}){/if}
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
          <button class="rm" on:click={() => remove(w.id)}>✕</button>
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
  {:else if tab === "fences"}
    {#if !$home}
      <p class="muted">Set a home location first — new fences are centred on it.</p>
    {:else}
      <form class="add fence" on:submit|preventDefault={createFence}>
        <input placeholder="label" bind:value={fLabel} />
        <label>radius <input type="number" min="1" max="250" bind:value={fRadius} /> nm</label>
        <label>below <input type="number" placeholder="any" bind:value={fMaxAlt} /> ft</label>
        <label><input type="checkbox" bind:checked={fMil} /> military only</label>
        <button type="submit">Add fence around home</button>
      </form>
    {/if}
    <ul>
      {#each fences as f (f.id)}
        <li>
          <input type="checkbox" checked={f.enabled} on:change={() => toggleFence(f)} />
          <span class="v">{f.label}</span>
          <span class="l">
            {f.radiusNm} nm{#if f.maxAltFt} · &lt;{Math.round(f.maxAltFt).toLocaleString()} ft{/if}{#if f.milOnly} · mil{/if}
          </span>
          <button class="rm" on:click={() => delFence(f.id)}>✕</button>
        </li>
      {:else}
        <li class="muted">No geofences.</li>
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
    width: 340px;
    max-height: 74vh;
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
    gap: 3px;
    flex-wrap: wrap;
  }
  .tabs button {
    font-size: 11px;
    padding: 3px 7px;
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
  .add.fence {
    grid-template-columns: 1fr;
  }
  .add select,
  .add input {
    min-width: 0;
  }
  .add button {
    grid-column: 1 / -1;
  }
  .add.fence label {
    font-size: 11px;
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .add.fence label input {
    width: 70px;
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
    color: #7ee2b8;
    letter-spacing: 0.03em;
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
