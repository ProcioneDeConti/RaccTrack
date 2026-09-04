<script lang="ts">
  import { onMount } from "svelte";
  import {
    getSettings,
    updateSettings,
    tileCacheStats,
    clearTileCache,
    downloadTileArea,
    geocode,
    onDownloadProgress,
    clearHistory,
    type TileCacheStats,
    type DownloadProgress,
  } from "../api/backend";
  import type { AppSettings, GeoResult, HomeLocation } from "../api/types";
  import { units } from "../format";
  import { basemap, home, goHomeSignal } from "../state";
  import { BASEMAP_THEMES } from "../map/style";
  import { clipToRegion, type Bbox } from "../map/region";
  import Icon from "../ui/Icon.svelte";
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

  // --- home location ---
  let homeQuery = "";
  let homeResults: GeoResult[] = [];
  let homeSearching = false;
  let homeError = "";

  async function searchHome() {
    const q = homeQuery.trim();
    if (!q) return;
    homeSearching = true;
    homeError = "";
    homeResults = [];
    try {
      homeResults = await geocode(q);
      if (homeResults.length === 0) homeError = "No matches.";
    } catch (e) {
      homeError = humanizeError(e);
    } finally {
      homeSearching = false;
    }
  }

  async function pickHome(r: GeoResult) {
    const h: HomeLocation = {
      label: r.label,
      lat: r.lat,
      lon: r.lon,
      bbox: r.bbox,
    };
    home.set(h);
    homeResults = [];
    homeQuery = "";
    goHomeSignal.update((n) => n + 1);
    await patch({ home: h });
  }

  async function clearHome() {
    home.set(null);
    await patch({ home: null });
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
        <option value={2000}>2 s</option>
        <option value={3000}>3 s</option>
        <option value={5000}>5 s</option>
        <option value={10000}>10 s</option>
      </select>
    </label>

    <label class="row">
      Map theme
      <select value={s.basemap} on:change={(e) => setBasemap(e.currentTarget.value)}>
        {#each BASEMAP_THEMES as t}
          <option value={t.key}>{t.label}</option>
        {/each}
      </select>
    </label>

    <hr />
    <h4>Home location</h4>
    <form class="home-search" on:submit|preventDefault={searchHome}>
      <input
        type="text"
        placeholder="state, city, ZIP, address, or lat, lon"
        bind:value={homeQuery}
      />
      <button type="submit" disabled={homeSearching}>
        {homeSearching ? "…" : "Search"}
      </button>
    </form>
    {#if homeError}<p class="err">{homeError}</p>{/if}
    {#if homeResults.length}
      <ul class="results">
        {#each homeResults as r}
          <li>
            <button on:click={() => pickHome(r)}>
              <span class="lbl">{r.label}</span>
              <span class="kind">{r.kind}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if $home}
      <div class="current-home">
        <span class="hl" title={$home.label}><Icon name="home" size={13} /> {$home.label}</span>
        <span class="home-actions">
          <button on:click={() => goHomeSignal.update((n) => n + 1)}>Go</button>
          <button on:click={clearHome}>Clear</button>
        </span>
      </div>
    {/if}
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
  .hl {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .home-search {
    display: flex;
    gap: 4px;
  }
  .home-search input {
    flex: 1;
    min-width: 0;
  }
  .results {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-height: 180px;
    overflow-y: auto;
  }
  .results button {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    text-align: left;
    font-size: 12px;
    padding: 5px 8px;
  }
  .results .lbl {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .results .kind {
    color: var(--text-dim);
    font-size: 10px;
    flex-shrink: 0;
  }
  .current-home {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .current-home > span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .home-actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }
  .home-actions button {
    font-size: 11px;
    padding: 2px 8px;
  }
  .err {
    color: var(--emergency);
    font-size: 11px;
    margin: 0;
  }
</style>
