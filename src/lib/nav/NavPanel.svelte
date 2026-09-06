<script lang="ts">
  import { onMount } from "svelte";
  import Panel from "../ui/Panel.svelte";
  import Icon from "../ui/Icon.svelte";
  import Message from "../ui/Message.svelte";
  import { vorStatus, primaryPlace, flyTo } from "../state";
  import {
    nearestNavaids,
    vorTune,
    vorStop,
    getVorStatus,
    vorFixStart,
  } from "../api/backend";
  import type { NavaidNear } from "../api/types";
  import { morseLine } from "./morse";
  import { humanizeError } from "../ui/errors";

  export let onClose: () => void;

  let nearby: NavaidNear[] = [];
  let pick = "";
  let manual = "";
  let loading = true;
  let actionError: string | null = null;

  onMount(async () => {
    try {
      const p = $primaryPlace;
      if (p) {
        nearby = await nearestNavaids(p.lat, p.lon, 250, true);
        pick = nearby[0]?.ident ?? "";
      }
    } finally {
      loading = false;
    }
  });

  $: s = $vorStatus;
  $: running = !!s?.running;
  $: fix = s?.fix ?? null;

  async function startFix() {
    actionError = null;
    try {
      await vorFixStart([]);
      vorStatus.set(await getVorStatus());
    } catch (e) {
      actionError = humanizeError(e);
    }
  }

  function showFixOnMap() {
    if (fix?.result) flyTo.set({ lat: fix.result.lat, lon: fix.result.lon, zoom: 9 });
  }

  const COMPASS = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
  const compass = (deg: number) => COMPASS[Math.round(((deg % 360) / 45)) % 8];

  async function tune() {
    actionError = null;
    const ident = (manual.trim() || pick).toUpperCase();
    if (!ident) return;
    try {
      await vorTune(ident);
      vorStatus.set(await getVorStatus());
    } catch (e) {
      actionError = humanizeError(e);
    }
  }
  async function stop() {
    actionError = null;
    await vorStop();
    vorStatus.set(await getVorStatus());
  }

  const fmtDeg = (d: number | null | undefined) =>
    d == null ? "—" : `${Math.round(((d % 360) + 360) % 360).toString().padStart(3, "0")}°`;
  const fmtDelta = (d: number | null | undefined) =>
    d == null ? "" : `${d >= 0 ? "+" : "−"}${Math.abs(d).toFixed(1)}°`;

  // Point on the dial for a compass bearing (0 = up, clockwise), centred at 50,50.
  const nx = (deg: number, len: number) => 50 + len * Math.cos(((deg - 90) * Math.PI) / 180);
  const ny = (deg: number, len: number) => 50 + len * Math.sin(((deg - 90) * Math.PI) / 180);
  const ticks = Array.from({ length: 12 }, (_, i) => i * 30);
</script>

<Panel title="VOR navigation" {onClose} width={320} bodyPad={false}>
  <div class="intro">
    Tune a nearby VOR off the dongle and decode its radial + Morse ident, then
    compare with the geometric radial from your primary place. Shares the ADS-B
    dongle (pauses decoding for the session). <em>Experimental — unverified
    against real hardware; a VHF antenna is needed, not the 1090 one.</em>
  </div>

  <div class="controls">
    <div class="row">
      {#if nearby.length}
        <select bind:value={pick} disabled={running || !!manual.trim()}>
          {#each nearby as n}
            <option value={n.ident}>
              {n.ident} · {n.kind} · {Math.round(n.distanceNm)} nm
            </option>
          {/each}
        </select>
      {/if}
      <input
        class="ident"
        placeholder="or ident"
        bind:value={manual}
        disabled={running}
        maxlength="4"
      />
      {#if running}
        <button class="btn-link" on:click={stop}>Stop</button>
      {:else}
        <button class="btn-link" on:click={tune}>Tune</button>
      {/if}
    </div>
    <div class="row">
      <button class="btn-link fix-btn" on:click={startFix} disabled={running || !$primaryPlace}>
        <Icon name="crosshair" size={12} /> Fix my position (nearby VORs)
      </button>
    </div>
    {#if !$primaryPlace}
      <p class="hint">Set a primary place to get the nearby list, the geometric radial, and position fixes.</p>
    {/if}
    {#if actionError}
      <p class="err"><Icon name="alert-triangle" size={11} /> {actionError}</p>
    {:else if s?.lastError}
      <p class="err"><Icon name="alert-triangle" size={11} /> {humanizeError(s.lastError)}</p>
    {/if}
  </div>

  <div class="body">
    {#if fix}
      <div class="fix">
        {#if fix.phase === "tuning"}
          <div class="fix-head">
            Fixing position — station {fix.stationIndex + 1} of {fix.stationCount}
            {#if fix.currentIdent}(<span class="mono">{fix.currentIdent}</span>){/if}
          </div>
        {:else if fix.phase === "failed"}
          <div class="fix-head bad"><Icon name="alert-triangle" size={12} /> {fix.error}</div>
        {:else if fix.result}
          <div class="fix-head ok">Position fix ({fix.result.lopCount} stations)</div>
        {/if}

        <ul class="lops">
          {#each fix.collected as c}
            <li>
              <span class="mono">{c.ident}</span>
              <span class="lop-r">{c.radialMagDeg == null ? "no lock" : fmtDeg(c.radialMagDeg) + " R"}</span>
              {#if c.identOk === true}<span class="ok">✓</span>
              {:else if c.identOk === false}<span class="bad">ident?</span>{/if}
            </li>
          {/each}
          {#if fix.phase === "tuning"}
            {#each Array(Math.max(0, fix.stationCount - fix.collected.length)) as _}
              <li class="pending"><span class="mono">···</span></li>
            {/each}
          {/if}
        </ul>

        {#if fix.result}
          <dl class="readout">
            <div><dt>Fix</dt><dd class="mono">{fix.result.lat.toFixed(4)}, {fix.result.lon.toFixed(4)}</dd></div>
            <div><dt>Uncertainty</dt><dd>± {fix.result.uncertaintyNm.toFixed(1)} nm</dd></div>
            {#if fix.result.distanceFromPlaceNm != null}
              <div>
                <dt>From your place</dt>
                <dd>
                  {fix.result.distanceFromPlaceNm.toFixed(1)} nm
                  {compass(fix.result.bearingFromPlaceDeg ?? 0)}
                </dd>
              </div>
            {/if}
          </dl>
          <div class="row">
            <button class="btn-link" on:click={showFixOnMap}>Show on map</button>
            <button class="btn-link" on:click={stop}>Clear</button>
          </div>
        {:else if fix.phase === "tuning"}
          <button class="btn-link" on:click={stop}>Cancel</button>
        {:else}
          <button class="btn-link" on:click={stop}>Dismiss</button>
        {/if}
      </div>
    {:else if loading && !s}
      <Message kind="loading">Loading…</Message>
    {:else if !running}
      <Message kind="empty">Not tuned. Pick a VOR and hit Tune, or run a position fix.</Message>
    {:else}
      <div class="station">
        <b>{s?.stationIdent}</b>
        <span>{s?.stationName}</span>
        <span class="mono">{s?.freqMhz?.toFixed(2)} MHz · {s?.stationKind}</span>
      </div>

      <div class="dial-wrap">
        <svg viewBox="0 0 100 100" class="dial">
          <circle cx="50" cy="50" r="46" class="ring" />
          {#each ticks as t}
            <line
              x1={nx(t, 46)}
              y1={ny(t, 46)}
              x2={nx(t, t % 90 === 0 ? 38 : 42)}
              y2={ny(t, t % 90 === 0 ? 38 : 42)}
              class="tick"
            />
          {/each}
          <text x="50" y="12" class="card">0</text>
          {#if s?.geometricRadialDeg != null}
            <line x1="50" y1="50" x2={nx(s.geometricRadialDeg, 40)} y2={ny(s.geometricRadialDeg, 40)} class="geo" />
          {/if}
          {#if s?.receivedRadialDeg != null}
            <line x1="50" y1="50" x2={nx(s.receivedRadialDeg, 40)} y2={ny(s.receivedRadialDeg, 40)} class="recv" />
          {/if}
          <circle cx="50" cy="50" r="2.5" class="hub" />
        </svg>
      </div>

      <dl class="readout">
        <div><dt>Received radial</dt><dd class="big">{fmtDeg(s?.receivedRadialDeg)}</dd></div>
        <div>
          <dt>Geometric radial</dt>
          <dd>
            {fmtDeg(s?.geometricRadialDeg)}
            {#if s?.radialDeltaDeg != null}<span class="delta">{fmtDelta(s.radialDeltaDeg)}</span>{/if}
          </dd>
        </div>
        {#if s?.distanceNm != null}
          <div><dt>Distance</dt><dd>{s.distanceNm.toFixed(1)} nm{#if s?.hasDme} <span class="dim">(has DME)</span>{/if}</dd></div>
        {/if}
        <div>
          <dt>Ident</dt>
          <dd>
            <span class="mono">{s?.decodedIdent ?? "…"}</span>
            {#if s?.identOk === true}<span class="ok">✓</span>
            {:else if s?.identOk === false}<span class="bad">≠ {s?.expectedIdent}</span>{/if}
            <span class="morse">{morseLine(s?.expectedIdent ?? "")}</span>
          </dd>
        </div>
      </dl>

      <div class="sig">
        <span>signal</span>
        <div class="bar"><div class="fill" style="width:{Math.round((s?.signal ?? 0) * 100)}%"></div></div>
      </div>
      {#if s?.adsbPaused}<p class="hint">ADS-B decoding is paused while this session runs.</p>{/if}
    {/if}
  </div>
</Panel>

<style>
  .intro,
  .hint {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.5;
  }
  .hint {
    padding: 0 10px;
    margin: 0;
  }
  .intro {
    border-bottom: 1px solid var(--border);
  }
  .controls {
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .row select {
    flex: 1 1 auto;
    min-width: 0;
  }
  .ident {
    width: 64px;
    text-transform: uppercase;
  }
  .err {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--emergency);
    font-size: 11px;
    margin: 0;
  }
  .body {
    padding: 8px 10px;
  }
  .station {
    display: flex;
    flex-direction: column;
    gap: 1px;
    font-size: 12px;
    margin-bottom: 8px;
  }
  .station b {
    font-size: 14px;
  }
  .station span {
    color: var(--text-dim);
    font-size: 11px;
  }
  .mono {
    font-family: ui-monospace, monospace;
  }
  .dial-wrap {
    display: flex;
    justify-content: center;
    margin: 4px 0 10px;
  }
  .dial {
    width: 150px;
    height: 150px;
  }
  .ring {
    fill: var(--bg);
    stroke: var(--border);
    stroke-width: 1.5;
  }
  .tick {
    stroke: var(--text-dim);
    stroke-width: 1;
  }
  .card {
    fill: var(--text-dim);
    font-size: 9px;
    text-anchor: middle;
  }
  .geo {
    stroke: var(--text-dim);
    stroke-width: 2;
    stroke-dasharray: 3 2;
  }
  .recv {
    stroke: var(--accent);
    stroke-width: 2.5;
  }
  .hub {
    fill: var(--accent);
  }
  .readout {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .readout div {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
  }
  dt {
    color: var(--text-dim);
    font-size: 11px;
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
    font-size: 12px;
    text-align: right;
  }
  dd.big {
    font-size: 16px;
    font-weight: 700;
  }
  .delta {
    color: var(--text-dim);
    margin-left: 5px;
    font-size: 11px;
  }
  .dim {
    color: var(--text-dim);
    font-size: 10px;
  }
  .ok {
    color: var(--ok);
  }
  .bad {
    color: var(--emergency);
    font-size: 11px;
  }
  .morse {
    display: block;
    color: var(--text-dim);
    font-family: ui-monospace, monospace;
    letter-spacing: 1px;
    font-size: 10px;
  }
  .sig {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 10px;
    font-size: 10px;
    color: var(--text-dim);
  }
  .bar {
    flex: 1 1 auto;
    height: 5px;
    border-radius: 3px;
    background: var(--bg-elev);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.4s ease-out;
  }
  .fix-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .fix {
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12px;
  }
  .fix-head {
    font-weight: 600;
  }
  .fix-head.ok {
    color: var(--ok);
  }
  .fix-head.bad {
    color: var(--emergency);
    display: flex;
    align-items: center;
    gap: 4px;
    font-weight: 400;
  }
  .lops {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .lops li {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-size: 11px;
  }
  .lops li.pending {
    opacity: 0.4;
  }
  .lop-r {
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
</style>
