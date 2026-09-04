<script lang="ts">
  import { onDestroy } from "svelte";
  import { selectedAirport } from "../state";
  import { airportInfo, stationWx } from "../api/backend";
  import type { AirportInfo, StationWx } from "../api/types";
  import { altitude } from "../format";
  import { decodeMetar, decodeTaf } from "../wx/metar";

  let showRaw = false;

  $: decoded = wx?.metar ? decodeMetar(wx.metar) : null;
  $: taf = wx?.tafRaw ? decodeTaf(wx.tafRaw) : null;

  let info: AirportInfo | null = null;
  let wx: StationWx | null = null;
  let loading = false;
  let current: string | null = null;

  const unsub = selectedAirport.subscribe((code) => {
    if (code === current) return; // ignore duplicate emissions of the same airport
    current = code;
    info = null;
    wx = null;
    if (code) void load(code);
  });
  onDestroy(unsub);

  async function load(code: string) {
    loading = true;
    try {
      const i = await airportInfo(code);
      if (current !== code) return; // selection changed while awaiting
      info = i;
      const icao = i?.icao ?? i?.ident ?? code;
      const w = await stationWx(icao).catch(
        () => ({ metar: null, tafRaw: null }) as StationWx,
      );
      if (current !== code) return;
      wx = w;
    } finally {
      if (current === code) loading = false;
    }
  }

  function close() {
    selectedAirport.set(null);
  }

  const catColor: Record<string, string> = {
    VFR: "#3fb950",
    MVFR: "#3b82f6",
    IFR: "#ef4444",
    LIFR: "#d946ef",
  };

  const FREQ_NAMES: Record<string, string> = {
    TWR: "Tower",
    GND: "Ground",
    ATIS: "Automatic Terminal Information Service",
    "D-ATIS": "Digital ATIS",
    AWOS: "Automated Weather Observing System",
    ASOS: "Automated Surface Observing System",
    AWSS: "Automated Weather Sensor System",
    CTAF: "Common Traffic Advisory Frequency",
    UNIC: "Unicom",
    UNICOM: "Unicom",
    MULTICOM: "Multicom",
    APP: "Approach",
    DEP: "Departure",
    "A/D": "Arrival / Departure",
    ARR: "Arrival",
    CLD: "Clearance Delivery",
    "CLNC DEL": "Clearance Delivery",
    DEL: "Clearance Delivery",
    CTR: "Center",
    FSS: "Flight Service Station",
    RDO: "Radio",
    RMP: "Ramp",
    APRON: "Apron",
    OPS: "Operations",
    EMERG: "Emergency",
    AFIS: "Aerodrome Flight Information Service",
    TRACON: "Terminal Radar Approach Control",
    PMSV: "Pilot-to-Metro Service",
    GCA: "Ground-Controlled Approach",
    ATF: "Aerodrome Traffic Frequency",
    MF: "Mandatory Frequency",
    TIBA: "Traffic Information Broadcast by Aircraft",
  };

  function freqName(kind: string): string | null {
    return FREQ_NAMES[kind.trim().toUpperCase()] ?? null;
  }
</script>

{#if current}
  <aside class="panel">
    <header>
      <div class="title">
        <span class="code">{info?.icao ?? current}</span>
        {#if info?.iata}<span class="iata">{info.iata}</span>{/if}
        {#if wx?.metar?.flightCategory}
          <span
            class="cat"
            style="background:{catColor[wx.metar.flightCategory] ?? '#888'}"
            >{wx.metar.flightCategory}</span
          >
        {/if}
      </div>
      <button class="close" on:click={close}>✕</button>
    </header>

    {#if loading && !info}
      <p class="muted">Loading…</p>
    {:else if !info}
      <p class="muted">No data for {current}.</p>
    {:else}
      <p class="name">{info.name}</p>
      <p class="muted sub">
        {info.municipality ?? ""}{info.region ? ` · ${info.region}` : ""}
        {#if info.elevationFt != null} · elev {altitude(info.elevationFt)}{/if}
        · {info.kind.replace("_", " ")}
      </p>

      {#if info.runways.length}
        <section>
          <h4>Runways</h4>
          {#each info.runways.filter((r) => !r.closed) as r}
            <div class="rw">
              <strong>{r.name}</strong>
              <span class="muted">
                {#if r.lengthFt}{Math.round(r.lengthFt).toLocaleString()} ft{/if}
                {#if r.surface} · {r.surface}{/if}
                {#if r.lighted} · lit{/if}
              </span>
            </div>
          {/each}
        </section>
      {/if}

      {#if info.frequencies.length}
        <section>
          <h4>Frequencies</h4>
          {#each info.frequencies as f}
            <div class="fq">
              <span class="fk">
                {f.kind}
                {#if freqName(f.kind)}<em>{freqName(f.kind)}</em>{/if}
                {#if f.description && f.description.toUpperCase() !== f.kind.toUpperCase() && f.description !== freqName(f.kind)}
                  <span class="fdesc">· {f.description}</span>
                {/if}
              </span>
              <strong>{f.mhz}</strong>
            </div>
          {/each}
        </section>
      {/if}

      <section>
        <h4>
          Weather
          {#if wx?.metar}
            <button class="rawtoggle" on:click={() => (showRaw = !showRaw)}>
              {showRaw ? "hide raw" : "raw"}
            </button>
          {/if}
        </h4>
        {#if wx === null}
          <p class="muted">Loading…</p>
        {:else if wx.metar}
          {#if decoded}
            <dl class="decoded">
              {#each decoded.rows as row}
                <dt>{row.label}</dt>
                <dd>{row.value}</dd>
              {/each}
            </dl>
            {#each decoded.flags as f}
              <p class="wxflag">{f}</p>
            {/each}
          {/if}
          {#if showRaw}
            <p class="raw">{wx.metar.raw}</p>
          {/if}

          {#if taf && taf.periods.length}
            <h5>
              Forecast (TAF)
              {#if taf.valid}<span class="muted">· valid {taf.valid}</span>{/if}
            </h5>
            {#each taf.periods as p}
              <div class="tafp">
                <div class="tafhead">{p.heading}</div>
                {#each p.lines as ln}<div class="tafline">{ln}</div>{/each}
                {#if p.extra.length}
                  <div class="tafline muted">{p.extra.join(" ")}</div>
                {/if}
              </div>
            {/each}
            {#if showRaw}<p class="raw taf">{wx.tafRaw}</p>{/if}
          {:else if wx.tafRaw && showRaw}
            <p class="raw taf">{wx.tafRaw}</p>
          {/if}
        {:else}
          <p class="muted">
            No weather station at this airport — nearest METAR shown on the map.
          </p>
        {/if}
      </section>
    {/if}
  </aside>
{/if}

<style>
  .panel {
    position: absolute;
    top: 14px;
    right: 14px;
    bottom: 14px;
    width: 320px;
    overflow-y: auto;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px 14px;
    z-index: 10;
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.45);
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .code {
    font-size: 18px;
    font-weight: 700;
  }
  .iata {
    font-size: 11px;
    color: var(--text-dim);
  }
  .cat {
    font-size: 10px;
    font-weight: 700;
    color: #06121f;
    padding: 1px 5px;
    border-radius: 4px;
  }
  .close {
    border: none;
    background: transparent;
  }
  .name {
    margin: 6px 0 0;
    font-weight: 600;
  }
  .sub {
    margin: 2px 0 0;
    font-size: 11px;
  }
  section {
    border-top: 1px solid var(--border);
    padding-top: 8px;
    margin-top: 8px;
  }
  h4 {
    margin: 0 0 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-dim);
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .rw,
  .fq {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
    padding: 1px 0;
  }
  .fk em {
    font-size: 10px;
    color: var(--text-dim);
    font-style: italic;
    margin-left: 3px;
  }
  .fdesc {
    font-size: 10px;
    color: var(--text-dim);
  }
  .muted {
    color: var(--text-dim);
  }
  .raw {
    font-family: ui-monospace, monospace;
    font-size: 11px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    margin: 0 0 6px;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .taf {
    color: var(--text-dim);
  }
  .rawtoggle {
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-dim);
    font-size: 9px;
    text-transform: none;
    letter-spacing: 0;
    padding: 1px 5px;
    border-radius: 4px;
    cursor: pointer;
  }
  dl.decoded {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 10px;
    margin: 0 0 4px;
    font-size: 12px;
  }
  dl.decoded dt {
    color: var(--text-dim);
    white-space: nowrap;
  }
  dl.decoded dd {
    margin: 0;
  }
  .wxflag {
    font-size: 10px;
    color: var(--text-dim);
    margin: 0;
  }
  h5 {
    margin: 10px 0 4px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-dim);
    font-weight: 600;
  }
  .tafp {
    margin: 0 0 6px;
    font-size: 12px;
  }
  .tafhead {
    font-weight: 600;
  }
  .tafline {
    padding-left: 10px;
  }
</style>
