<script lang="ts">
  import { get } from "svelte/store";
  import { aircraft, selectedHex, selectedAirport, flyTo } from "../state";
  import { findAirport } from "../api/backend";
  import type { Airport } from "../api/types";

  let q = "";
  let acHits: { hex: string; label: string; sub: string }[] = [];
  let apHits: Airport[] = [];
  let open = false;
  let searching = false;
  let seq = 0;

  async function run() {
    const s = q.trim().toUpperCase();
    open = true;
    acHits = [];
    apHits = [];
    if (s.length < 2) return;

    const map = get(aircraft);
    for (const a of map.values()) {
      if (acHits.length >= 8) break;
      const cs = (a.flight ?? "").toUpperCase();
      const reg = (a.registration ?? "").toUpperCase();
      const hex = a.hex.toUpperCase();
      if (cs.includes(s) || reg.includes(s) || hex.includes(s)) {
        acHits.push({
          hex: a.hex,
          label: (a.flight ?? a.registration ?? a.hex).trim(),
          sub: [a.registration, a.typeCode].filter(Boolean).join(" · "),
        });
      }
    }

    const mine = ++seq;
    searching = true;
    try {
      const hits = await findAirport(s);
      if (mine === seq) apHits = hits.slice(0, 6);
    } catch {
      /* ignore */
    } finally {
      if (mine === seq) searching = false;
    }
  }

  function pickAc(hex: string) {
    const a = get(aircraft).get(hex);
    selectedAirport.set(null);
    selectedHex.set(hex);
    if (a?.lat != null && a?.lon != null)
      flyTo.set({ lat: a.lat, lon: a.lon, zoom: 9 });
    reset();
  }

  function pickAp(a: Airport) {
    selectedHex.set(null);
    selectedAirport.set(a.icao ?? a.ident);
    flyTo.set({ lat: a.lat, lon: a.lon, zoom: 11 });
    reset();
  }

  function reset() {
    q = "";
    open = false;
    acHits = [];
    apHits = [];
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") reset();
    if (e.key === "Enter") {
      if (acHits[0]) pickAc(acHits[0].hex);
      else if (apHits[0]) pickAp(apHits[0]);
    }
  }
</script>

<div class="search" class:open>
  <input
    type="text"
    placeholder="Search callsign · reg · hex · airport"
    bind:value={q}
    on:input={run}
    on:focus={() => (open = q.trim().length >= 2)}
    on:keydown={onKey}
  />
  {#if q}<button class="x" on:click={reset}>✕</button>{/if}

  {#if open && (acHits.length || apHits.length || searching)}
    <div class="results">
      {#if acHits.length}
        <div class="grp">Aircraft</div>
        {#each acHits as h}
          <button on:click={() => pickAc(h.hex)}>
            <span class="l">{h.label}</span><span class="s">{h.sub}</span>
          </button>
        {/each}
      {/if}
      {#if apHits.length}
        <div class="grp">Airports</div>
        {#each apHits as a}
          <button on:click={() => pickAp(a)}>
            <span class="l">{a.icao ?? a.ident}</span>
            <span class="s">{a.name}</span>
          </button>
        {/each}
      {/if}
      {#if searching && !apHits.length}<div class="grp">searching…</div>{/if}
    </div>
  {/if}
</div>

<style>
  .search {
    position: relative;
  }
  input {
    width: 210px;
    transition: width 0.15s;
  }
  .search.open input,
  input:focus {
    width: 250px;
  }
  .x {
    position: absolute;
    right: 2px;
    top: 2px;
    border: none;
    background: transparent;
    font-size: 11px;
    padding: 2px 5px;
  }
  .results {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    width: 280px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 4px;
    z-index: 20;
    max-height: 320px;
    overflow-y: auto;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
  }
  .grp {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-dim);
    padding: 4px 6px 2px;
  }
  .results button {
    width: 100%;
    display: flex;
    justify-content: space-between;
    gap: 8px;
    text-align: left;
    border: none;
    background: transparent;
    padding: 5px 6px;
    font-size: 12px;
    border-radius: 5px;
  }
  .results button:hover {
    background: var(--bg-elev);
  }
  .results .s {
    color: var(--text-dim);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .results .l {
    font-weight: 600;
    flex-shrink: 0;
  }
</style>
