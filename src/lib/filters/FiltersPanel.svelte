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
    <button class="btn-link" on:click={reset} disabled={isDefault(f)}>Reset</button>
  </svelte:fragment>

  <h4 class="row">
    Altitude
    <span class="muted">{f.altMin.toLocaleString()}–{f.altMax.toLocaleString()} ft</span>
  </h4>
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

  <h4 class="row">Types (ICAO)</h4>
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
    align-items: baseline;
    margin: 10px 0 4px;
  }
  .muted {
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-dim);
  }
</style>
