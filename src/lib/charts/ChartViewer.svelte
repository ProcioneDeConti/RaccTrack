<script lang="ts">
  import { onDestroy } from "svelte";
  import { chartTarget } from "../state";
  import { airportCharts, chartPdf, openExternal } from "../api/backend";
  import type { ChartRef, ChartSet } from "../api/types";
  import Icon from "../ui/Icon.svelte";

  let target: { ident: string; label: string } | null = null;
  let set: ChartSet | null = null;
  let groups: { name: string; charts: ChartRef[] }[] = [];
  let flat: ChartRef[] = [];
  let selected: ChartRef | null = null;

  let listLoading = false;
  let listError: string | null = null;
  let pdfLoading = false;
  let pdfError: string | null = null;
  let pdfUrl: string | null = null;

  const GROUP_ORDER = [
    "Airport Diagram",
    "Approach Procedures",
    "Departure Procedures",
    "Obstacle Departures",
    "Arrival Procedures",
    "Takeoff / Alternate Minimums",
    "Hot Spots / LAHSO",
    "Other",
  ];

  const unsub = chartTarget.subscribe((t) => {
    if (t?.ident === target?.ident) {
      target = t;
      return;
    }
    target = t;
    if (t) void load(t.ident);
    else reset();
  });
  onDestroy(() => {
    unsub();
    revoke();
  });

  function reset() {
    set = null;
    groups = [];
    flat = [];
    selected = null;
    listError = null;
    pdfError = null;
    revoke();
  }

  function revoke() {
    if (pdfUrl) URL.revokeObjectURL(pdfUrl);
    pdfUrl = null;
  }

  async function load(ident: string) {
    reset();
    listLoading = true;
    try {
      const s = await airportCharts(ident);
      if (chartTargetIdent() !== ident) return;
      set = s;
      const byGroup = new Map<string, ChartRef[]>();
      for (const c of s.charts) {
        if (!byGroup.has(c.group)) byGroup.set(c.group, []);
        byGroup.get(c.group)!.push(c);
      }
      const rank = (n: string) => {
        const i = GROUP_ORDER.indexOf(n);
        return i < 0 ? 99 : i;
      };
      groups = [...byGroup.entries()]
        .map(([name, charts]) => ({ name, charts }))
        .sort((a, b) => rank(a.name) - rank(b.name));
      flat = groups.flatMap((g) => g.charts);
      if (flat.length) void pick(flat[0]);
    } catch (e) {
      listError = String(e);
    } finally {
      if (chartTargetIdent() === ident) listLoading = false;
    }
  }

  function chartTargetIdent(): string | null {
    return target?.ident ?? null;
  }

  async function pick(c: ChartRef) {
    selected = c;
    pdfError = null;
    pdfLoading = true;
    revoke();
    try {
      const buf = await chartPdf(c.url);
      if (selected !== c) return; // moved on
      pdfUrl = URL.createObjectURL(
        new Blob([buf], { type: "application/pdf" }),
      );
    } catch (e) {
      pdfError = String(e);
    } finally {
      if (selected === c) pdfLoading = false;
    }
  }

  function step(delta: number) {
    if (!selected || !flat.length) return;
    const i = flat.indexOf(selected);
    const next = flat[(i + delta + flat.length) % flat.length];
    if (next) void pick(next);
  }

  function close() {
    chartTarget.set(null);
  }

  function openSelected() {
    if (selected) void openExternal(selected.url);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
    else if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      step(1);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      step(-1);
    }
  }
</script>

<svelte:window on:keydown={target ? onKey : undefined} />

{#if target}
  <div
    class="backdrop"
    on:click|self={close}
    on:keydown={(e) => e.key === "Enter" && close()}
    role="presentation"
  >
    <div class="modal" role="dialog" aria-label="Airport charts">
      <header>
        <div class="title">
          <strong>{set?.airport ?? target.ident}</strong>
          <span class="dim">{target.label}</span>
          {#if set}<span class="cycle">Cycle {set.cycle}</span>{/if}
        </div>
        <button class="close" on:click={close} aria-label="Close"><Icon name="x" size={15} /></button>
      </header>

      <div class="body">
        <nav class="picker">
          {#if listLoading}
            <p class="dim pad">Loading chart index…</p>
          {:else if listError}
            <p class="err pad">{listError}</p>
          {:else if !flat.length}
            <p class="dim pad">No published charts for this airport.</p>
          {:else}
            {#each groups as g}
              <h4>{g.name}</h4>
              {#each g.charts as c}
                <button
                  class="chart"
                  class:active={selected === c}
                  on:click={() => pick(c)}
                >
                  {c.name}
                </button>
              {/each}
            {/each}
          {/if}
        </nav>

        <div class="view">
          {#if selected}
            <div class="viewbar">
              <button
                class="ib"
                on:click={() => step(-1)}
                disabled={flat.length < 2}
                aria-label="Previous chart"
              >
                <Icon name="chevron-left" size={15} /> Prev
              </button>
              <span class="cur">{selected.name}</span>
              <button
                class="ib"
                on:click={() => step(1)}
                disabled={flat.length < 2}
                aria-label="Next chart"
              >
                Next <Icon name="chevron-right" size={15} />
              </button>
              <span class="spacer"></span>
              <button class="ib" on:click={openSelected}>
                Open in browser <Icon name="external-link" size={13} />
              </button>
            </div>
          {/if}
          <div class="pane">
            {#if pdfLoading}
              <p class="dim center">Loading chart…</p>
            {:else if pdfError}
              <div class="center">
                <p class="err">Couldn’t load this chart.</p>
                <p class="dim">{pdfError}</p>
                {#if selected}
                  <button class="ib" on:click={openSelected}>
                    Open in browser instead <Icon name="external-link" size={13} />
                  </button>
                {/if}
              </div>
            {:else if pdfUrl}
              <iframe src={pdfUrl} title={selected?.name ?? "chart"}></iframe>
            {:else if !listLoading && !listError && flat.length === 0}
              <p class="dim center">Nothing to show.</p>
            {/if}
          </div>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }
  .modal {
    width: min(1100px, 100%);
    height: min(820px, 100%);
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .title strong {
    font-size: 16px;
  }
  .dim {
    color: var(--text-dim);
    font-size: 12px;
  }
  .cycle {
    color: var(--text-dim);
    font-size: 11px;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 5px;
  }
  .close {
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
  }
  .close:hover {
    color: var(--text);
  }
  .ib {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .picker {
    width: 260px;
    flex-shrink: 0;
    border-right: 1px solid var(--border);
    overflow-y: auto;
    padding: 6px;
  }
  .picker h4 {
    margin: 10px 6px 4px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-dim);
  }
  .chart {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: var(--text);
    padding: 5px 8px;
    border-radius: 5px;
    font-size: 12px;
    cursor: pointer;
  }
  .chart:hover {
    background: var(--bg-elev);
  }
  .chart.active {
    background: var(--accent);
    color: #06121f;
  }
  .view {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .viewbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
  }
  .viewbar .cur {
    color: var(--text-dim);
  }
  .viewbar .spacer {
    flex: 1;
  }
  .viewbar button {
    font-size: 12px;
    padding: 3px 8px;
  }
  .pane {
    flex: 1;
    background: #333;
    min-height: 0;
  }
  .pane iframe {
    width: 100%;
    height: 100%;
    border: 0;
  }
  .center {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    text-align: center;
  }
  .pad {
    padding: 10px;
  }
  .err {
    color: var(--emergency);
  }
</style>
