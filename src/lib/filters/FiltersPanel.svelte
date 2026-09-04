<script lang="ts">
  import { filters } from "../state";
  import { ALT_CEILING, defaultFilters, isDefault } from "./filters";
  import Panel from "../ui/Panel.svelte";

  export let onClose: () => void;

  $: f = $filters;

  function reset() {
    filters.set(defaultFilters());
  }
  function typeList(s: string): string[] {
    return s
      .split(/[,\s]+/)
      .map((t) => t.trim().toUpperCase())
      .filter(Boolean);
  }
</script>

<Panel title="Filters" {onClose} width={260}>
  <svelte:fragment slot="actions">
    <button class="link" on:click={reset} disabled={isDefault(f)}>Reset</button>
  </svelte:fragment>

  <div class="row hdr">
    Altitude
    <span class="muted">{f.altMin.toLocaleString()}–{f.altMax.toLocaleString()} ft</span>
  </div>
  <input
    type="range"
    min="0"
    max={ALT_CEILING}
    step="500"
    bind:value={f.altMin}
    on:input={() => (f.altMin = Math.min(f.altMin, f.altMax))}
  />
  <input
    type="range"
    min="0"
    max={ALT_CEILING}
    step="500"
    bind:value={f.altMax}
    on:input={() => (f.altMax = Math.max(f.altMin, f.altMax))}
  />

  <label><input type="checkbox" bind:checked={f.militaryOnly} /> Military only</label>
  <label><input type="checkbox" bind:checked={f.emergencyOnly} /> Emergency squawk only</label>
  <label><input type="checkbox" bind:checked={f.hideGround} /> Hide on-ground</label>
  <label><input type="checkbox" bind:checked={f.requirePosition} /> Require position</label>

  <div class="row hdr">Types (ICAO)</div>
  <input
    type="text"
    placeholder="e.g. B738 A320 C172"
    value={f.types.join(" ")}
    on:change={(e) => (f.types = typeList(e.currentTarget.value))}
  />
</Panel>

<style>
  input[type="range"] {
    width: 100%;
  }
  input[type="text"] {
    width: 100%;
  }
  label {
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 5px 0;
  }
  .row {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    margin: 8px 0 4px;
  }
  .hdr {
    color: var(--text);
  }
  .muted {
    color: var(--text-dim);
  }
  .link {
    border: none;
    background: transparent;
    color: var(--accent);
    font-size: 12px;
    padding: 2px 4px;
  }
  .link:disabled {
    color: var(--text-dim);
    opacity: 0.6;
  }
</style>
