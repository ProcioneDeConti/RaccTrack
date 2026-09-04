<script lang="ts">
  import { onDestroy } from "svelte";
  import { datalinkFor } from "../api/backend";
  import type { DlMessage } from "../api/types";
  import { ACCENT } from "../theme/colors";

  export let hex: string;

  let loaded = false;
  let loading = false;
  let error: string | null = null;
  let msgs: DlMessage[] = [];
  let timer: ReturnType<typeof setInterval> | null = null;

  // Datalink protocol → chip colour (a palette of its own — not the altitude
  // scale, though a couple of hues overlap).
  const KIND_COLOR: Record<string, string> = {
    ACARS: ACCENT,
    VDL2: "#7ad151",
    HFDL: "#f9c74f",
    SATCOM: "#b5179e",
  };

  async function refresh() {
    loading = true;
    error = null;
    try {
      msgs = await datalinkFor(hex);
      loaded = true;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function start() {
    void refresh();
    timer ??= setInterval(refresh, 60_000);
  }

  onDestroy(() => {
    if (timer) clearInterval(timer);
  });

  function when(ms: number): string {
    if (!ms) return "";
    const s = Math.round((Date.now() - ms) / 1000);
    if (s < 60) return `${s}s ago`;
    if (s < 3600) return `${Math.round(s / 60)}m ago`;
    if (s < 86400) return `${Math.round(s / 3600)}h ago`;
    return new Date(ms).toISOString().slice(0, 10);
  }
</script>

<section>
  <h4>
    Datalink
    {#if loaded}
      <button class="mini" on:click={refresh} disabled={loading} title="Refresh">
        ⟳
      </button>
    {/if}
  </h4>

  {#if !loaded}
    <button class="load" on:click={start} disabled={loading}>
      {loading ? "Loading…" : "Show recent ACARS / VDL messages"}
    </button>
    {#if error}<p class="err">{error}</p>{/if}
  {:else if error && !msgs.length}
    <p class="err">{error}</p>
  {:else if !msgs.length}
    <p class="muted">No recent datalink messages for this aircraft.</p>
  {:else}
    <ul class="dl">
      {#each msgs as m}
        <li>
          <div class="meta">
            <span
              class="kind"
              style="background:{KIND_COLOR[m.kind] ?? '#8b949e'}">{m.kind}</span
            >
            {#if m.label}<span class="lbl" title={m.labelDesc ?? ""}>{m.label}</span>{/if}
            {#if m.labelDesc}<span class="ldesc">{m.labelDesc}</span>{/if}
            <span class="spacer"></span>
            {#if m.freqMhz}<span class="dim">{m.freqMhz.toFixed(3)}</span>{/if}
            <span class="dim">{when(m.time)}</span>
          </div>
          {#if m.route}<div class="route">{m.route}</div>{/if}
          <pre>{m.text}</pre>
          {#if m.station}<div class="dim stn">via {m.station}</div>{/if}
        </li>
      {/each}
    </ul>
    <p class="src">Source: airframes.io (community ACARS/VDL network)</p>
  {/if}
</section>

<style>
  h4 {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .mini,
  .load {
    font-size: 11px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    border-radius: 5px;
    cursor: pointer;
  }
  .mini {
    padding: 0 5px;
    text-transform: none;
    letter-spacing: 0;
  }
  .load {
    width: 100%;
    padding: 7px 10px;
    text-align: left;
  }
  .load:hover {
    border-color: var(--accent);
  }
  .dl {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .dl li {
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 7px;
    background: var(--bg);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    margin-bottom: 3px;
  }
  .spacer {
    flex: 1;
  }
  .kind {
    color: #06121f;
    font-weight: 700;
    padding: 0 4px;
    border-radius: 3px;
  }
  .lbl {
    font-family: ui-monospace, monospace;
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0 3px;
  }
  .ldesc,
  .dim {
    color: var(--text-dim);
  }
  .route {
    font-size: 11px;
    color: var(--text-dim);
    margin-bottom: 2px;
  }
  pre {
    margin: 0;
    font-family: ui-monospace, monospace;
    font-size: 11px;
    white-space: pre-wrap;
    word-break: break-word;
    line-height: 1.35;
  }
  .stn {
    font-size: 10px;
    margin-top: 3px;
  }
  .src {
    font-size: 10px;
    color: var(--text-dim);
    margin: 6px 0 0;
  }
  .err {
    color: var(--emergency);
    font-size: 12px;
  }
  .muted {
    color: var(--text-dim);
  }
</style>
