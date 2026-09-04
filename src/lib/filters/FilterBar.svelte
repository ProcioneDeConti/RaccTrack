<script lang="ts">
  import { filters } from "../state";
  import { ALT_CEILING, defaultFilters, isDefault } from "./filters";
  import Icon from "../ui/Icon.svelte";

  let open = false;
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

<div class="filterbar">
  <button class="mapbtn" class:active={open} on:click={() => (open = !open)}>
    <Icon name="filter" size={14} />
    Filters {#if !isDefault(f)}<span class="dot"></span>{/if}
  </button>

  {#if open}
    <div class="popover">
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

      <div class="actions">
        <button on:click={reset} disabled={isDefault(f)}>Reset</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .filterbar {
    position: absolute;
    top: 14px;
    left: 52px;
    z-index: 10;
  }
  .mapbtn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    margin-left: 4px;
    vertical-align: middle;
  }
  .popover {
    margin-top: 6px;
    width: 240px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .popover input[type="range"] {
    width: 100%;
  }
  label {
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .row {
    justify-content: space-between;
  }
  .hdr {
    display: flex;
    font-size: 12px;
    color: var(--text);
  }
  .muted {
    color: var(--text-dim);
  }
  .actions {
    margin-top: 4px;
  }
</style>
