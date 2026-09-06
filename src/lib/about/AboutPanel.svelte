<script lang="ts">
  import Panel from "../ui/Panel.svelte";
  import Icon from "../ui/Icon.svelte";
  import RaccoonMark from "../ui/RaccoonMark.svelte";
  import { openExternal, checkForUpdate } from "../api/backend";
  import { disclaimerOpen, updateInfo } from "../state";

  export let onClose: () => void;

  const version = __APP_VERSION__;

  let checking = false;
  async function checkUpdates() {
    checking = true;
    try {
      updateInfo.set(await checkForUpdate(true));
    } finally {
      checking = false;
    }
  }
  const repo = "https://github.com/ProcioneDeConti/RaccTrack";

  const stack = [
    "Rust",
    "Tauri v2",
    "Svelte",
    "MapLibre GL",
    "TypeScript",
    "Vite",
  ];

  const data: { label: string; url: string; note: string }[] = [
    { label: "adsb.lol · adsb.fi", url: "https://adsb.lol/", note: "live aircraft (ODbL)" },
    { label: "OurAirports", url: "https://ourairports.com/", note: "airports, runways, navaids" },
    { label: "Mictronics", url: "https://www.mictronics.de/", note: "aircraft & type database" },
    { label: "OpenFlights", url: "https://openflights.org/", note: "airline callsigns" },
    { label: "hexdb.io", url: "https://hexdb.io/", note: "route lookups" },
    { label: "aviationweather.gov", url: "https://aviationweather.gov/", note: "METAR / TAF" },
    { label: "FAA d-TPP", url: "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dtpp/", note: "approach plates" },
    { label: "airframes.io", url: "https://airframes.io/", note: "ACARS / VDL (CC-BY-4.0)" },
    { label: "planespotters.net · Wikimedia", url: "https://www.planespotters.net/", note: "aircraft photos" },
    { label: "Photon by Komoot", url: "https://photon.komoot.io/", note: "home-location search" },
    { label: "CARTO · OpenFreeMap · OpenStreetMap", url: "https://www.openstreetmap.org/copyright", note: "basemaps" },
    { label: "Lucide", url: "https://lucide.dev/", note: "icon geometry (ISC)" },
  ];

  function open(url: string) {
    void openExternal(url);
  }
</script>

<Panel title="About" {onClose} width={330}>
  <div class="hero">
    <span class="mark"><RaccoonMark size={64} /></span>
    <div>
      <div class="name">RaccTrack <span class="sub">(ADS-B)</span></div>
      <div class="ver">v{version}</div>
    </div>
  </div>

  <p class="blurb">
    A free, no-paywall live ADS-B flight tracker for North America — a live map
    plus the most public per-aircraft detail I could pull together. No
    schedule, gate or delay data: there's no free source for it, and the app
    says so where it matters.
  </p>

  <div class="upd">
    <button class="row" on:click={checkUpdates} disabled={checking}>
      <Icon name="refresh-cw" size={13} />
      <span>{checking ? "Checking…" : "Check for updates"}</span>
    </button>
    {#if $updateInfo}
      {#if $updateInfo.error}
        <p class="upd-msg bad">Couldn't check — {$updateInfo.error}</p>
      {:else if $updateInfo.newer}
        <p class="upd-msg ok">
          v{$updateInfo.latest} is available —
          <button class="lnk" on:click={() => open($updateInfo.assetUrl ?? $updateInfo.url)}>download</button>
          ·
          <button class="lnk" on:click={() => open($updateInfo.url)}>what's new</button>
        </p>
      {:else}
        <p class="upd-msg">You're on the latest version.</p>
      {/if}
    {/if}
  </div>

  <div class="links">
    <button class="row" on:click={() => open(repo)}>
      <Icon name="external-link" size={13} />
      <span>GitHub — ProcioneDeConti/RaccTrack</span>
    </button>
    <button class="row" on:click={() => open(`${repo}/blob/main/LICENSE`)}>
      <Icon name="external-link" size={13} />
      <span>Licensed Apache-2.0</span>
    </button>
    <button class="row" on:click={() => disclaimerOpen.set(true)}>
      <Icon name="alert-triangle" size={13} />
      <span>Safety &amp; data disclaimer</span>
    </button>
  </div>

  <h4>Built by</h4>
  <p class="credit">
    <b>ProcioneDeConti</b><br />
    with <b>Claude</b> (Anthropic) via Claude Code
  </p>

  <h4>Built with</h4>
  <p class="chips">
    {#each stack as s}<span>{s}</span>{/each}
  </p>

  <h4>Data &amp; content</h4>
  <ul class="data">
    {#each data as d}
      <li>
        <button class="lnk" on:click={() => open(d.url)}>{d.label}</button>
        <span class="note">{d.note}</span>
      </li>
    {/each}
  </ul>

  <p class="foot">
    Aircraft positions © the adsb.lol / adsb.fi feeder communities (ODbL).
    Basemap data © OpenStreetMap contributors. Full attribution in the repo's
    NOTICE file.
  </p>
</Panel>

<style>
  .hero {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 10px;
  }
  .mark {
    color: var(--accent);
  }
  .name {
    font-size: 17px;
    font-weight: 700;
  }
  .name .sub {
    font-weight: 400;
    color: var(--text-dim);
  }
  .ver {
    font-size: 12px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .blurb,
  .foot {
    font-size: 12px;
    color: var(--text-dim);
    margin: 0 0 12px;
    line-height: 1.5;
  }
  .foot {
    margin: 12px 0 0;
    font-size: 10px;
  }
  .links {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 12px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    text-align: left;
    border: 1px solid var(--border);
    background: var(--bg);
    border-radius: var(--radius-sm);
    padding: 6px 9px;
    font-size: 12px;
  }
  .row:hover {
    border-color: var(--accent);
  }
  .row:disabled {
    opacity: 0.6;
  }
  .upd {
    margin-bottom: 12px;
  }
  .upd-msg {
    font-size: 11px;
    color: var(--text-dim);
    margin: 6px 2px 0;
    line-height: 1.5;
  }
  .upd-msg.ok {
    color: var(--ok);
  }
  .upd-msg.bad {
    color: var(--emergency);
  }
  .credit {
    font-size: 12px;
    margin: 0 0 12px;
    line-height: 1.6;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0 0 12px;
  }
  .chips span {
    font-size: 11px;
    color: var(--text-dim);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 6px;
  }
  .data {
    list-style: none;
    margin: 0 0 4px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .data li {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    font-size: 11px;
  }
  .lnk {
    border: none;
    background: transparent;
    color: var(--accent);
    padding: 0;
    font-size: 11px;
    text-align: left;
  }
  .lnk:hover {
    text-decoration: underline;
  }
  .data .note {
    color: var(--text-dim);
    flex: 0 0 auto;
  }
</style>
