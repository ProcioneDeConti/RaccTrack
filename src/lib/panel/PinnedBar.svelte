<script lang="ts">
  import { aircraft, pinned, togglePin, selectedHex, flyTo } from "../state";
  import { altitude, speed } from "../format";

  $: cards = $pinned.map((hex) => ({ hex, ac: $aircraft.get(hex) ?? null }));

  function pick(hex: string) {
    const a = $aircraft.get(hex);
    selectedHex.set(hex);
    if (a?.lat != null && a?.lon != null)
      flyTo.set({ lat: a.lat, lon: a.lon });
  }

  function vs(r: number | null | undefined): string {
    if (r == null || Math.abs(r) < 100) return "";
    return r > 0 ? "▲" : "▼";
  }
</script>

{#if cards.length}
  <div class="bar">
    {#each cards as c (c.hex)}
      <button
        class="card"
        class:sel={$selectedHex === c.hex}
        class:lost={!c.ac}
        on:click={() => pick(c.hex)}
      >
        <span class="cs">
          {(c.ac?.flight ?? c.ac?.registration ?? c.hex).trim()}
          <span
            class="unpin"
            role="button"
            tabindex="-1"
            on:click|stopPropagation={() => togglePin(c.hex)}
            on:keydown|stopPropagation
            title="Unpin">✕</span
          >
        </span>
        {#if c.ac}
          <span class="meta">
            {c.ac.typeCode ?? "—"} ·
            {c.ac.onGround ? "GND" : altitude(c.ac.altBaro ?? null)}
            {vs(c.ac.baroRate ?? c.ac.geomRate)}
            {#if c.ac.groundSpeed != null} · {speed(c.ac.groundSpeed)}{/if}
          </span>
        {:else}
          <span class="meta">not in feed</span>
        {/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .bar {
    position: absolute;
    left: 50%;
    bottom: 10px;
    transform: translateX(-50%);
    display: flex;
    gap: 6px;
    max-width: 80%;
    overflow-x: auto;
    z-index: 9;
    padding-bottom: 2px;
  }
  .card {
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 5px 9px;
    text-align: left;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
  }
  .card.sel {
    border-color: var(--accent);
  }
  .card.lost {
    opacity: 0.5;
  }
  .cs {
    font-size: 12px;
    font-weight: 700;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .unpin {
    color: var(--text-dim);
    font-size: 10px;
  }
  .unpin:hover {
    color: var(--text);
  }
  .meta {
    font-size: 10px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
</style>
