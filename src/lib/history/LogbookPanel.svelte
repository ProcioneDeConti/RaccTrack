<script lang="ts">
  import Panel from "../ui/Panel.svelte";
  import Icon from "../ui/Icon.svelte";
  import Message from "../ui/Message.svelte";
  import {
    logbook,
    logbookCount,
    exportLogbook,
    clearLogbook,
    deleteSighting,
  } from "../api/backend";
  import { openExternal } from "../api/backend";
  import { aircraft, selectedHex, flyTo } from "../state";
  import type { Sighting } from "../api/types";

  export let onClose: () => void;

  type Sort = "last" | "first" | "count" | "reg";
  let sort: Sort = "last";
  let search = "";
  let rows: Sighting[] = [];
  let total = 0;
  let loading = true;
  let exported: string | null = null;
  let searchTimer: ReturnType<typeof setTimeout>;

  async function load() {
    loading = true;
    try {
      [rows, total] = await Promise.all([
        logbook(sort, search, 2000),
        logbookCount(),
      ]);
    } finally {
      loading = false;
    }
  }
  // runs once on mount and again whenever the sort changes
  $: if (sort) void load();

  function onSearch() {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(load, 250);
  }

  function pick(s: Sighting) {
    selectedHex.set(s.hex);
    const a = $aircraft.get(s.hex);
    if (a?.lat != null && a?.lon != null) flyTo.set({ lat: a.lat, lon: a.lon });
  }

  async function remove(hex: string) {
    await deleteSighting(hex);
    rows = rows.filter((r) => r.hex !== hex);
    total -= 1;
  }

  async function wipe() {
    if (!confirm("Delete the entire logbook?")) return;
    await clearLogbook();
    rows = [];
    total = 0;
  }

  async function doExport() {
    exported = await exportLogbook();
  }

  const day = (ms: number) =>
    new Date(ms).toLocaleDateString([], {
      year: "2-digit",
      month: "short",
      day: "numeric",
    });
</script>

<Panel title="Logbook" {onClose} width={340} bodyPad={false}>
  <svelte:fragment slot="actions">
    <select bind:value={sort} title="Sort">
      <option value="last">Recent</option>
      <option value="first">First seen</option>
      <option value="count">Most seen</option>
      <option value="reg">Registration</option>
    </select>
    <button class="btn-link" on:click={doExport} title="Export CSV">Export</button>
  </svelte:fragment>

  <div class="head">
    <input
      type="search"
      placeholder="reg · hex · type · callsign"
      bind:value={search}
      on:input={onSearch}
    />
    <div class="sub">
      <span>{total.toLocaleString()} airframe{total === 1 ? "" : "s"} logged</span>
      {#if total > 0}<button class="btn-link" on:click={wipe}>Clear</button>{/if}
    </div>
    {#if exported}
      <button class="exported" on:click={() => openExternal(exported ?? "")}>
        Saved → {exported}
      </button>
    {/if}
  </div>

  <div class="scroll">
    {#if loading && rows.length === 0}
      <Message kind="loading">Loading…</Message>
    {:else if rows.length === 0}
      <Message kind="empty">
        {search
          ? "No matches."
          : "Nothing logged yet — every aircraft that comes into view gets recorded here."}
      </Message>
    {:else}
      <ul>
        {#each rows as s (s.hex)}
          <li>
            <button class="row" on:click={() => pick(s)}>
              <span class="id">
                {s.registration ?? s.hex.toUpperCase()}
                {#if s.military}<span class="mil">M</span>{/if}
              </span>
              <span class="ty">{s.typeCode ?? "—"}</span>
              <span class="seen">{s.count}×</span>
              <time>{day(s.lastSeen)}</time>
            </button>
            <button class="rm" title="Remove" aria-label="Remove" on:click={() => remove(s.hex)}>
              <Icon name="x" size={11} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</Panel>

<style>
  .head {
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .head input {
    width: 100%;
  }
  .sub {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 11px;
    color: var(--text-dim);
  }
  .exported {
    border: 1px solid var(--border);
    background: var(--bg);
    border-radius: var(--radius-sm);
    padding: 4px 7px;
    font-size: 10px;
    color: var(--text-dim);
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .scroll {
    padding: 4px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
  }
  .row {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    border: none;
    background: transparent;
    text-align: left;
    padding: 5px 6px;
    border-radius: 5px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .row:hover {
    background: var(--bg-elev);
  }
  .id {
    font-weight: 600;
    color: var(--text);
    font-family: ui-monospace, monospace;
    flex: 0 0 auto;
    min-width: 74px;
  }
  .mil {
    color: var(--ok);
    margin-left: 3px;
  }
  .ty {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .seen {
    flex: 0 0 auto;
    font-variant-numeric: tabular-nums;
  }
  time {
    flex: 0 0 auto;
    font-variant-numeric: tabular-nums;
  }
  .rm {
    flex: 0 0 auto;
    border: none;
    background: transparent;
    color: var(--text-dim);
    display: inline-flex;
    align-items: center;
    padding: 4px;
    opacity: 0;
  }
  li:hover .rm {
    opacity: 1;
  }
  .rm:hover {
    color: var(--emergency);
  }
</style>
