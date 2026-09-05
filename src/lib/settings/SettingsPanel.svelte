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
    listRtlsdrDevices,
    rtlsdrStatus,
    fixUsbDriver,
    computeCoverage,
    type TileCacheStats,
    type DownloadProgress,
  } from "../api/backend";
  import type { AppSettings, RtlSdrStatus } from "../api/types";
  import { units } from "../format";
  import { basemap, uiTheme, coverageResult, coverageEnabled } from "../state";
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
  let rtlStatus: RtlSdrStatus | null = null;
  let rtlStatusTimer: number | undefined;

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
    const pollRtlStatus = () => {
      void rtlsdrStatus().then((v) => (rtlStatus = v));
    };
    pollRtlStatus();
    rtlStatusTimer = window.setInterval(pollRtlStatus, 2000);
    return () => {
      unlisten?.();
      clearInterval(rtlStatusTimer);
    };
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

  function setUiTheme(v: string) {
    const t = v === "light" || v === "dark" ? v : "auto";
    uiTheme.set(t); // live-swaps the app chrome
    void patch({ uiTheme: t }); // persists
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

  // --- direct RTL-SDR ---
  let rtlDevices: string[] = [];
  let rtlDetecting = false;
  let rtlDetectError: string | null = null;

  async function detectRtlDevices() {
    rtlDetecting = true;
    rtlDetectError = null;
    try {
      rtlDevices = await listRtlsdrDevices();
      if (rtlDevices.length === 0) rtlDetectError = "No RTL-SDR dongles found.";
    } catch (e) {
      rtlDetectError = humanizeError(e);
    } finally {
      rtlDetecting = false;
    }
  }

  function setGainMode(v: string) {
    void patch({ rtlsdrGainTenthsDb: v === "auto" ? null : num(v, 297) });
  }
  const num = (v: string, fallback: number) => {
    const n = parseFloat(v);
    return Number.isFinite(n) ? n : fallback;
  };

  let fixingDriver = false;
  let fixDriverResult: string | null = null;
  let fixDriverOk = false;
  async function doFixUsbDriver() {
    fixingDriver = true;
    fixDriverResult = null;
    try {
      await fixUsbDriver();
      fixDriverOk = true;
      fixDriverResult = "Zadig launched — pick your dongle and click Install Driver.";
    } catch (e) {
      fixDriverOk = false;
      fixDriverResult = humanizeError(e);
    } finally {
      fixingDriver = false;
    }
  }

  let coverageComputing = false;
  let coverageError: string | null = null;
  async function doComputeCoverage() {
    coverageComputing = true;
    coverageError = null;
    try {
      coverageResult.set(await computeCoverage());
    } catch (e) {
      coverageError = humanizeError(e);
    } finally {
      coverageComputing = false;
    }
  }
</script>

<Panel title="Settings" {onClose} width={320}>
 <div class="stack">
  {#if s}
    <details open>
      <summary>Data sources</summary>
      <div class="stack section">
        <label class="row">
          <span>Online community feeds (adsb.lol / adsb.fi)</span>
          <input
            type="checkbox"
            checked={s.onlineSourcesEnabled}
            on:change={(e) => patch({ onlineSourcesEnabled: e.currentTarget.checked })}
          />
        </label>
        {#if !s.onlineSourcesEnabled}
          <p class="muted">
            Local sources only — the local receiver and direct RTL-SDR below
            are unaffected. If neither is on, nothing will show.
          </p>
        {/if}

        <hr />
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
            Tried first, with the community feeds as automatic fallback if
            it's unreachable. A faster poll interval is worth it — no rate
            limits.
          </p>
        {/if}

        <hr />
        <h4>Direct RTL-SDR</h4>
        <label class="row">
          <span>Decode ADS-B straight from a USB dongle</span>
          <input
            type="checkbox"
            checked={s.rtlsdrEnabled}
            on:change={(e) => patch({ rtlsdrEnabled: e.currentTarget.checked })}
          />
        </label>
        {#if s.rtlsdrEnabled}
          {#if rtlStatus}
            <div class="row">
              <span class="muted">Device</span>
              <span
                class="rx"
                class:ok={rtlStatus.deviceOpen}
                class:bad={!rtlStatus.deviceOpen && !!rtlStatus.lastError}
              >
                {#if rtlStatus.deviceOpen}
                  Open — {rtlStatus.messagesDecoded.toLocaleString()} messages, {rtlStatus.aircraftTracked}
                  aircraft
                {:else if rtlStatus.lastError}
                  {humanizeError(rtlStatus.lastError)}
                {:else}
                  Connecting…
                {/if}
              </span>
            </div>
          {/if}
          <div class="row">
            <button on:click={detectRtlDevices} disabled={rtlDetecting}>
              {rtlDetecting ? "Detecting…" : "Detect devices"}
            </button>
            {#if rtlDetectError}
              <span class="rx bad">{rtlDetectError}</span>
            {/if}
          </div>
          {#if rtlDevices.length > 0}
            <label class="row">
              Device
              <select
                value={s.rtlsdrDeviceIndex}
                on:change={(e) => patch({ rtlsdrDeviceIndex: +e.currentTarget.value })}
              >
                {#each rtlDevices as d, i}
                  <option value={i}>{d}</option>
                {/each}
              </select>
            </label>
          {/if}
          <label class="row">
            Gain
            <select
              value={s.rtlsdrGainTenthsDb === null ? "auto" : String(s.rtlsdrGainTenthsDb)}
              on:change={(e) => setGainMode(e.currentTarget.value)}
            >
              <option value="auto">Auto</option>
              <option value="0">0.0 dB</option>
              <option value="150">15.0 dB</option>
              <option value="297">29.7 dB</option>
              <option value="420">42.0 dB</option>
              <option value="496">49.6 dB</option>
            </select>
          </label>
          <p class="muted">
            Outranks even the local receiver above when on. Decodes 1090ES
            (ADS-B) only — no Mode A/C, and position needs both an even and
            an odd frame within 10s of each other, same as any Mode S
            decoder.
          </p>
          <div class="row">
            <span class="muted">Windows dongle not showing "Open"?</span>
            <button on:click={doFixUsbDriver} disabled={fixingDriver}>
              {fixingDriver ? "Downloading…" : "Fix USB driver"}
            </button>
          </div>
          {#if fixDriverResult}
            <p class="rx" class:ok={fixDriverOk} class:bad={!fixDriverOk}>{fixDriverResult}</p>
          {/if}
          <p class="muted">
            Downloads and launches Zadig (elevation prompt expected) so the
            dongle can be bound to WinUSB — the same one-time step every SDR
            tool needs on Windows.
          </p>

          <hr />
          <h4>Reception coverage estimate</h4>
          <label class="row">
            <span>Show coverage polygon on the map</span>
            <input
              type="checkbox"
              checked={s.coverageEnabled}
              on:change={(e) => {
                coverageEnabled.set(e.currentTarget.checked);
                patch({ coverageEnabled: e.currentTarget.checked });
              }}
            />
          </label>
          {#if s.coverageEnabled}
            <label class="row">
              Target altitude
              <select
                value={s.coverageTargetAltFt}
                on:change={(e) => patch({ coverageTargetAltFt: +e.currentTarget.value })}
              >
                <option value={1000}>1,000 ft</option>
                <option value={3000}>3,000 ft</option>
                <option value={5000}>5,000 ft</option>
                <option value={10000}>10,000 ft</option>
                <option value={18000}>18,000 ft</option>
                <option value={35000}>35,000 ft</option>
              </select>
            </label>
            <label class="row">
              Antenna height (AGL)
              <span class="row" style="gap:4px">
                <input
                  type="number"
                  min="0"
                  step="1"
                  style="width:56px"
                  value={s.coverageAntennaHeightFt}
                  on:change={(e) =>
                    patch({ coverageAntennaHeightFt: num(e.currentTarget.value, 20) })}
                /> ft
              </span>
            </label>
            <div class="row">
              <button on:click={doComputeCoverage} disabled={coverageComputing}>
                {coverageComputing ? "Computing…" : "Recompute coverage"}
              </button>
              {#if $coverageResult}
                <span class="rx ok">
                  ground {Math.round($coverageResult.receiverGroundElevFt)} ft
                </span>
              {/if}
            </div>
            {#if coverageError}
              <p class="rx bad">{coverageError}</p>
            {/if}
            <p class="muted">
              Terrain line-of-sight around whichever place is marked "RTL-SDR"
              in Places &amp; alerts, for the altitude above — not a plain
              circle. Approximate; doesn't yet account for buildings or tree
              cover. Recompute after moving the antenna, changing its height,
              or picking a different altitude.
            </p>
          {/if}
        {/if}
      </div>
    </details>

    <details open>
      <summary>Appearance</summary>
      <div class="stack section">
        <label class="row">
          Map theme
          <select value={s.basemap} on:change={(e) => setBasemap(e.currentTarget.value)}>
            {#each BASEMAP_THEMES as t}
              <option value={t.key}>{t.label}</option>
            {/each}
          </select>
        </label>
        <label class="row">
          App theme
          <select value={s.uiTheme} on:change={(e) => setUiTheme(e.currentTarget.value)}>
            <option value="auto">Match map</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </label>
        <label class="row">
          Units
          <select value={s.units} on:change={(e) => setUnits(e.currentTarget.value)}>
            <option value="imperial">ft / kt</option>
            <option value="metric">m / km/h</option>
          </select>
        </label>
        <label class="row">
          <span>Show trails for all visible aircraft</span>
          <input
            type="checkbox"
            checked={s.showAllTrails}
            on:change={(e) => patch({ showAllTrails: e.currentTarget.checked })}
          />
        </label>
      </div>
    </details>

    <details>
      <summary>Aircraft photos</summary>
      <div class="stack section">
        <label class="stack">
          <span class="muted"
            >planespotters.net contact (URL or email) — required by them to
            show photos of the exact aircraft. Leave blank to use model
            photos from Wikipedia only.</span
          >
          <input
            type="text"
            placeholder="https://github.com/you  ·  you@example.com"
            value={s.contact}
            on:change={(e) => patch({ contact: e.currentTarget.value.trim() })}
          />
        </label>
      </div>
    </details>

    <details>
      <summary>Alerts &amp; history</summary>
      <div class="stack section">
        <label class="row">
          <span>Desktop notifications</span>
          <input
            type="checkbox"
            checked={s.notificationsEnabled}
            on:change={(e) => patch({ notificationsEnabled: e.currentTarget.checked })}
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

        <hr />
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
      </div>
    </details>

    <details>
      <summary>Offline maps</summary>
      <div class="stack section">
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
      </div>
    </details>
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
  details {
    border-bottom: 1px solid var(--border);
    padding-bottom: 8px;
  }
  details:last-child {
    border-bottom: none;
    padding-bottom: 0;
  }
  summary {
    margin: 0;
    padding: 4px 0;
    font-size: var(--fs-sm);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-dim);
    cursor: pointer;
    list-style: revert;
  }
  summary:hover {
    color: var(--text);
  }
  details[open] > summary {
    color: var(--text);
    margin-bottom: 2px;
  }
  .section {
    padding: 4px 2px 2px;
  }
  .section h4 {
    margin-top: 4px;
  }
</style>
