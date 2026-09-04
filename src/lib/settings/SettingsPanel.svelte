<script lang="ts">
  import { onMount } from "svelte";
  import {
    getSettings,
    updateSettings,
    tileCacheStats,
    clearTileCache,
    downloadTileArea,
    onDownloadProgress,
    clearHistory,
    clearLogbook,
    exportLogbook,
    testLocalReceiver,
    type TileCacheStats,
    type DownloadProgress,
  } from "../api/backend";
  import type { AppSettings } from "../api/types";
  import { units } from "../format";
  import { basemap } from "../state";
  import { BASEMAP_THEMES } from "../map/style";
  import { clipToRegion, type Bbox } from "../map/region";
  import Panel from "../ui/Panel.svelte";
  import { humanizeError } from "../ui/errors";

  export let onClose: () => void;
  export let currentBbox: () => Bbox | null;

  let s: AppSettings | null = null;
  let cache: TileCacheStats = { tiles: 0, bytes: 0 };
  let progress: DownloadProgress | null = null;
  let unlisten: (() => void) | undefined;

  onMount(() => {
    void (async () => {
      s = await getSettings();
      units.set(s.units);
      cache = await tileCacheStats();
      unlisten = await onDownloadProgress((p) => {
        progress = p;
        if (p.finished) void refreshCache();
      });
    })();
    return () => unlisten?.();
  });

  async function patch(p: Partial<AppSettings>) {
    s = await updateSettings(p);
    if (p.units) units.set(p.units);
  }

  function setUnits(v: string) {
    void patch({ units: v === "metric" ? "metric" : "imperial" });
  }

  function setBasemap(key: string) {
    basemap.set(key); // live-swaps the map style
    void patch({ basemap: key }); // persists
  }

  async function refreshCache() {
    cache = await tileCacheStats();
  }

  async function clear() {
    await clearTileCache();
    await refreshCache();
  }

  async function wipeHistory() {
    if (confirm("Delete all recorded flight history?")) await clearHistory();
  }

  let logbookExported: string | null = null;
  async function doExportLogbook() {
    logbookExported = await exportLogbook();
  }
  async function wipeLogbook() {
    if (confirm("Delete the entire spotter logbook?")) await clearLogbook();
  }

  async function downloadHere() {
    const b = currentBbox();
    if (!b) return;
    const clipped = clipToRegion(b);
    if (!clipped) return;
    progress = { done: 0, total: 0, finished: false };
    await downloadTileArea(clipped, 3, 9);
  }

  function mb(bytes: number): string {
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  // --- local receiver ---
  let rxTesting = false;
  let rxResult: string | null = null;
  let rxOk = false;

  async function testRx() {
    if (!s) return;
    rxTesting = true;
    rxResult = null;
    try {
      const p = await testLocalReceiver(s.localReceiverUrl);
      rxOk = true;
      rxResult = `${p.aircraft} aircraft (${p.withPosition} with position)`;
    } catch (e) {
      rxOk = false;
      rxResult = humanizeError(e);
    } finally {
      rxTesting = false;
    }
  }
</script>

<Panel title="Settings" {onClose} width={320}>
 <div class="stack">
  {#if s}
    <label class="row">
      Poll interval
      <select
        value={s.pollIntervalMs}
        on:change={(e) => patch({ pollIntervalMs: +e.currentTarget.value })}
      >
        {#if s.localReceiverEnabled}<option value={1000}>1 s</option>{/if}
        <option value={2000}>2 s</option>
        <option value={3000}>3 s</option>
        <option value={5000}>5 s</option>
        <option value={10000}>10 s</option>
      </select>
    </label>

    <hr />
    <h4>Local ADS-B receiver</h4>
    <label class="row">
      <span>Use a local receiver (dump1090 / readsb / tar1090)</span>
      <input
        type="checkbox"
        checked={s.localReceiverEnabled}
        on:change={(e) =>
          patch({ localReceiverEnabled: e.currentTarget.checked })}
      />
    </label>
    {#if s.localReceiverEnabled}
      <input
        type="text"
        value={s.localReceiverUrl}
        placeholder="http://localhost:8080/data/aircraft.json"
        on:change={(e) => {
          patch({ localReceiverUrl: e.currentTarget.value.trim() });
          rxResult = null;
        }}
      />
      <div class="row">
        <button on:click={testRx} disabled={rxTesting}>
          {rxTesting ? "Testing…" : "Test connection"}
        </button>
        {#if rxResult}
          <span class="rx" class:ok={rxOk} class:bad={!rxOk}>{rxResult}</span>
        {/if}
      </div>
      <p class="muted">
        Tried first, with the community feeds as automatic fallback if it's
        unreachable. A faster poll interval is worth it — no rate limits.
      </p>
    {/if}
    <hr />

    <label class="row">
      Map theme
      <select value={s.basemap} on:change={(e) => setBasemap(e.currentTarget.value)}>
        {#each BASEMAP_THEMES as t}
          <option value={t.key}>{t.label}</option>
        {/each}
      </select>
    </label>

    <hr />

    <h4>Aircraft photos</h4>
    <label class="stack">
      <span class="muted"
        >planespotters.net contact (URL or email) — required by them to show
        photos of the exact aircraft. Leave blank to use model photos from
        Wikipedia only.</span
      >
      <input
        type="text"
        placeholder="https://github.com/you  ·  you@example.com"
        value={s.contact}
        on:change={(e) => patch({ contact: e.currentTarget.value.trim() })}
      />
    </label>
    <hr />

    <label class="row">
      Units
      <select value={s.units} on:change={(e) => setUnits(e.currentTarget.value)}>
        <option value="imperial">ft / kt</option>
        <option value="metric">m / km/h</option>
      </select>
    </label>

    <label class="row">
      <span>Desktop notifications</span>
      <input
        type="checkbox"
        checked={s.notificationsEnabled}
        on:change={(e) => patch({ notificationsEnabled: e.currentTarget.checked })}
      />
    </label>

    <label class="row">
      <span>Show trails for all visible aircraft</span>
      <input
        type="checkbox"
        checked={s.showAllTrails}
        on:change={(e) => patch({ showAllTrails: e.currentTarget.checked })}
      />
    </label>

    <label class="row">
      <span>NA-wide emergency-squawk watch</span>
      <input
        type="checkbox"
        checked={s.emergencyWatchEnabled}
        on:change={(e) => patch({ emergencyWatchEnabled: e.currentTarget.checked })}
      />
    </label>

    <label class="row">
      <span>Record flight history (viewed aircraft)</span>
      <input
        type="checkbox"
        checked={s.historyEnabled}
        on:change={(e) => patch({ historyEnabled: e.currentTarget.checked })}
      />
    </label>
    {#if s.historyEnabled}
      <label class="row">
        Keep history for
        <select
          value={s.historyRetentionDays}
          on:change={(e) => patch({ historyRetentionDays: +e.currentTarget.value })}
        >
          <option value={7}>7 days</option>
          <option value={30}>30 days</option>
          <option value={90}>90 days</option>
          <option value={365}>1 year</option>
        </select>
      </label>
      <div class="row">
        <span class="muted">Recorded events</span>
        <button on:click={wipeHistory}>Clear history</button>
      </div>
    {/if}

    <label class="row">
      <span>Spotter logbook (every airframe seen)</span>
      <input
        type="checkbox"
        checked={s.logbookEnabled}
        on:change={(e) => patch({ logbookEnabled: e.currentTarget.checked })}
      />
    </label>
    {#if s.logbookEnabled}
      <div class="row">
        <span class="muted">Logbook</span>
        <span class="btns">
          <button on:click={doExportLogbook}>Export CSV</button>
          <button on:click={wipeLogbook}>Clear</button>
        </span>
      </div>
      {#if logbookExported}
        <p class="muted">Saved → {logbookExported}</p>
      {/if}
    {/if}

    <hr />

    <label class="row">
      <span>Cache map tiles for offline use <em>(experimental)</em></span>
      <input
        type="checkbox"
        checked={s.tileCacheEnabled}
        on:change={(e) => patch({ tileCacheEnabled: e.currentTarget.checked })}
      />
    </label>
    <div class="row">
      <span class="muted">Cached</span>
      <span class="muted">{cache.tiles} tiles · {mb(cache.bytes)}</span>
    </div>
    {#if s.tileCacheEnabled}
      <p class="muted">Restart the app for the tile cache to take effect.</p>
    {/if}
    <label class="row">
      Max cache size
      <select
        value={s.tileCacheMaxMb}
        on:change={(e) => patch({ tileCacheMaxMb: +e.currentTarget.value })}
      >
        <option value={200}>200 MB</option>
        <option value={500}>500 MB</option>
        <option value={1000}>1 GB</option>
        <option value={4000}>4 GB</option>
      </select>
    </label>
    <div class="btns">
      <button on:click={downloadHere}>Download current area (z3–9)</button>
      <button on:click={clear}>Clear cache</button>
    </div>
    {#if progress && !progress.finished}
      <p class="muted">
        Downloading… {progress.done}/{progress.total || "?"}
      </p>
    {:else if progress?.finished}
      <p class="muted">Download complete.</p>
    {/if}

  {/if}
 </div>
</Panel>

<style>
  .stack {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .btns {
    display: flex;
    gap: 6px;
  }
  .btns button {
    flex: 1;
    font-size: 11px;
  }
  hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: 2px 0;
  }
  .muted {
    color: var(--text-dim);
    font-size: 11px;
  }
  .rx {
    font-size: 11px;
    text-align: right;
  }
  .rx.ok {
    color: var(--ok);
  }
  .rx.bad {
    color: var(--emergency);
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
  }
  .stack input {
    width: 100%;
  }
</style>
