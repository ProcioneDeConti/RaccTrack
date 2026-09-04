<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    selectedHex,
    aircraft,
    followHex,
    pinned,
    togglePin,
    routeLine,
  } from "../state";
  import { getAircraftDetail, addWatch } from "../api/backend";
  import type { AircraftDetail } from "../api/types";
  import DatalinkSection from "./DatalinkSection.svelte";
  import Icon from "../ui/Icon.svelte";
  import {
    altitude,
    speed,
    verticalRate,
    degrees,
    age,
    squawkMeaning,
  } from "../format";
  import {
    distanceNm,
    fmtDistanceNm,
    fmtDuration,
    gcPath,
    projectOntoTrack,
  } from "../geo";

  let detail: AircraftDetail | null = null;
  let loading = false;
  let error: string | null = null;
  let currentHex: string | null = null;
  let photoIdx = 0;

  const unsub = selectedHex.subscribe((hex) => {
    currentHex = hex;
    detail = null;
    error = null;
    photoIdx = 0;
    routeLine.set(null);
    if (hex) void load(hex);
  });
  onDestroy(() => {
    unsub();
    routeLine.set(null);
  });

  async function load(hex: string) {
    loading = true;
    try {
      const d = await getAircraftDetail(hex);
      if (currentHex === hex) {
        detail = d;
        photoIdx = 0;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // Live telemetry comes from the aircraft store so it updates between detail fetches.
  $: live = currentHex ? ($aircraft.get(currentHex) ?? detail?.aircraft ?? null) : null;
  $: sqMeaning = squawkMeaning(live?.squawk ?? null);

  $: photos = detail?.photos ?? [];
  $: photo = photos[Math.min(photoIdx, Math.max(0, photos.length - 1))] ?? null;

  // --- type / age ---
  $: td = detail?.typeDetails ?? null;
  $: engineLine = td && td.engines && td.engType
    ? `${td.engines} × ${td.engType.toLowerCase()}`
    : null;
  $: builtYear = detail?.built ? parseInt(detail.built, 10) : NaN;
  $: builtLine = Number.isFinite(builtYear)
    ? `${builtYear} (${new Date().getFullYear() - builtYear} yr)`
    : null;

  // --- route progress ---
  $: prog = computeProgress(detail, live);
  $: routeLine.set(prog?.line ?? null);

  function computeProgress(d: AircraftDetail | null, l: typeof live) {
    const r = d?.route;
    if (
      !r ||
      r.originLat == null ||
      r.originLon == null ||
      r.destinationLat == null ||
      r.destinationLon == null
    )
      return null;
    const total = distanceNm(r.originLat, r.originLon, r.destinationLat, r.destinationLon);
    if (total < 1) return null;

    const path = gcPath(
      r.originLat,
      r.originLon,
      r.destinationLat,
      r.destinationLon,
      96,
    );

    const full: [number, number][] = path;
    const ageYears =
      r.updatedAt != null
        ? (Date.now() / 1000 - r.updatedAt) / (365.25 * 86400)
        : null;
    const base = {
      total,
      known: false,
      stale: false,
      flown: 0,
      toGo: total,
      pct: 0,
      etaHrs: null as number | null,
      line: { flown: [] as [number, number][], remain: full },
      destIcao: r.destinationIcao,
      updatedYear: r.updatedAt != null ? new Date(r.updatedAt * 1000).getUTCFullYear() : null,
      ageYears,
    };

    if (l?.lat == null || l?.lon == null) return base; // no live position

    const { along, cross } = projectOntoTrack(
      r.originLat,
      r.originLon,
      r.destinationLat,
      r.destinationLon,
      l.lat,
      l.lon,
    );

    // hexdb route data is often stale (the flight number now flies a different
    // pair). If the aircraft is clearly not on this path — or moderately off it
    // and the record is years old — don't pretend to know progress.
    const clearlyOff =
      cross > Math.max(120, total * 0.35) ||
      along < -Math.max(60, total * 0.1) ||
      along > total + Math.max(60, total * 0.1);
    const borderlineOff =
      cross > Math.max(60, total * 0.18) ||
      along < -Math.max(30, total * 0.05);
    if (clearlyOff || (borderlineOff && ageYears != null && ageYears > 3)) {
      return { ...base, stale: true };
    }

    const flown = Math.max(0, Math.min(total, along));
    const toGo = total - flown;
    const frac = flown / total;
    const k = Math.round(frac * (path.length - 1));
    const gs = l.groundSpeed ?? null;

    return {
      ...base,
      known: true,
      flown,
      toGo,
      pct: Math.round(frac * 100),
      etaHrs: gs && gs > 40 ? toGo / gs : null,
      line: { flown: path.slice(0, k + 1), remain: path.slice(k) },
    };
  }

  function close() {
    selectedHex.set(null);
  }

  async function watchThis() {
    if (!live) return;
    await addWatch("hex", live.hex, live.flight ?? live.registration ?? live.hex);
  }

  function prevPhoto() {
    photoIdx = (photoIdx - 1 + photos.length) % photos.length;
  }
  function nextPhoto() {
    photoIdx = (photoIdx + 1) % photos.length;
  }
</script>

{#if currentHex}
  <aside class="panel">
    <header class:has-photo={!!photo}>
      {#if photo}
        <img
          class="hero-img"
          src={photo.largeUrl ?? photo.thumbnailUrl}
          alt={live?.registration ?? currentHex}
        />
        <div class="hero-scrim"></div>
        {#if photos.length > 1}
          <button class="nav prev" on:click={prevPhoto} title="Previous photo" aria-label="Previous photo"><Icon name="chevron-left" size={18} /></button>
          <button class="nav next" on:click={nextPhoto} title="Next photo" aria-label="Next photo"><Icon name="chevron-right" size={18} /></button>
          <div class="dots">
            {#each photos as _, i}
              <span class="dot" class:on={i === photoIdx}></span>
            {/each}
          </div>
        {/if}
      {/if}
      <button class="close" on:click={close} title="Close" aria-label="Close"><Icon name="x" size={14} /></button>
      <div class="title">
        <span class="cs">{live?.flight ?? live?.registration ?? currentHex}</span>
        {#if live?.military}<span class="tag mil">MIL</span>{/if}
        {#if live?.emergency && live.emergency !== "none"}
          <span class="tag emg">{live.emergency.toUpperCase()}</span>
        {/if}
      </div>
      {#if photo}
        <a
          class="credit"
          href={photo.link ?? undefined}
          target="_blank"
          rel="noreferrer"
        >
          {#if photo.source === "wikipedia"}
            representative photo — {photo.photographer ?? "model"} · Wikipedia
          {:else}
            © {photo.photographer ?? "unknown"} · planespotters.net
          {/if}
        </a>
      {/if}
    </header>

    {#if loading && !detail}
      <p class="muted">Loading…</p>
    {:else if error}
      <p class="err">{error}</p>
    {/if}

    <section>
      <h4>Identity</h4>
      <dl>
        <dt>Registration</dt><dd>{detail?.aircraft.registration ?? live?.registration ?? "—"}</dd>
        <dt>Type</dt>
        <dd>
          {detail?.aircraft.typeCode ?? live?.typeCode ?? "—"}
          {#if detail?.aircraft.description}— {detail.aircraft.description}{/if}
        </dd>
        {#if engineLine}
          <dt>Engines</dt><dd>{engineLine}</dd>
        {/if}
        {#if td?.wtc}
          <dt>Wake category</dt><dd>{td.wtc}</dd>
        {/if}
        {#if builtLine}
          <dt>Built</dt><dd>{builtLine}</dd>
        {/if}
        <dt>Operator</dt>
        <dd>
          {#if detail?.operator}
            {detail.operator.name}
            {#if detail.operator.telephony}
              <span class="muted">· “{detail.operator.telephony}”</span>
            {/if}
          {:else}
            {detail?.ownerOperator ?? "—"}
          {/if}
        </dd>
        {#if detail?.operator && detail.ownerOperator && detail.ownerOperator !== detail.operator.name}
          <dt>Registered to</dt><dd>{detail.ownerOperator}</dd>
        {/if}
        <dt>Country</dt><dd>{detail?.country ?? "—"}</dd>
        <dt>ICAO hex</dt><dd class="mono">{currentHex}</dd>
      </dl>
    </section>

    <section>
      <h4>Route</h4>
      {#if detail?.route && (detail.route.originIcao || detail.route.destinationIcao)}
        <div class="route">
          <div>
            <strong>{detail.route.originIcao ?? "?"}</strong>
            <span class="muted">{detail.route.originName ?? ""}</span>
          </div>
          <div class="arrow"><Icon name="arrow-right" size={15} /></div>
          <div>
            <strong>{detail.route.destinationIcao ?? "?"}</strong>
            <span class="muted">{detail.route.destinationName ?? ""}</span>
          </div>
        </div>
        {#if prog?.known}
          <div class="bar" title="Great-circle position — no schedule data">
            <div class="bar-fill" style="width:{prog.pct}%"></div>
          </div>
          <div class="prog">
            <span>{prog.pct}%</span>
            <span>{fmtDistanceNm(prog.flown)} flown · {fmtDistanceNm(prog.toGo)} to go</span>
          </div>
          <p class="eta" title="Rough estimate from ground speed — no schedule data">
            {#if prog.etaHrs != null}
              ~{fmtDuration(prog.etaHrs)} to {prog.destIcao ?? "destination"} (est.)
            {:else}
              {fmtDistanceNm(prog.total)} total
            {/if}
          </p>
        {:else if prog?.stale}
          <p class="eta muted">
            {fmtDistanceNm(prog.total)} total · aircraft isn't on this path —
            route data{#if prog.updatedYear} (from {prog.updatedYear}){/if} looks out of date.
          </p>
        {:else if prog}
          <p class="eta muted">{fmtDistanceNm(prog.total)} total</p>
        {/if}
        {#if prog?.known && prog.updatedYear}
          <p class="src" title="hexdb route records are keyed by flight number and can lag reality">
            route data from {prog.updatedYear}
          </p>
        {/if}
      {:else}
        <p class="muted">Unknown</p>
      {/if}
    </section>

    <section>
      <h4>Live telemetry</h4>
      <dl>
        <dt>Altitude (baro)</dt><dd>{altitude(live?.altBaro ?? null)}</dd>
        <dt>Altitude (geom)</dt><dd>{altitude(live?.altGeom ?? null)}</dd>
        <dt>Ground speed</dt><dd>{speed(live?.groundSpeed ?? null)}</dd>
        <dt>IAS / TAS</dt><dd>{speed(live?.ias ?? null)} / {speed(live?.tas ?? null)}</dd>
        <dt>Mach</dt><dd>{live?.mach ?? "—"}</dd>
        <dt>Track</dt><dd>{degrees(live?.track ?? null)}</dd>
        <dt>Heading</dt><dd>{degrees(live?.trueHeading ?? live?.magHeading ?? null)}</dd>
        <dt>Vertical rate</dt>
        <dd class="vr">
          {#if (live?.baroRate ?? live?.geomRate ?? 0) > 0}<Icon name="arrow-up" size={11} />
          {:else if (live?.baroRate ?? live?.geomRate ?? 0) < 0}<Icon name="arrow-down" size={11} />{/if}
          {verticalRate(live?.baroRate ?? live?.geomRate ?? null)}
        </dd>
        <dt>Squawk</dt>
        <dd>{live?.squawk ?? "—"}{#if sqMeaning} <span class="muted">— {sqMeaning}</span>{/if}</dd>
        <dt>Selected alt</dt><dd>{altitude(live?.navAltitude ?? null)}</dd>
        <dt>On ground</dt><dd>{live?.onGround ? "yes" : "no"}</dd>
        <dt>Position source</dt><dd>{live?.positionSource ?? "—"}</dd>
        <dt>Signal</dt><dd>{live?.rssi ?? "—"} dBFS · {live?.messages ?? "—"} msgs</dd>
        <dt>Last message</dt><dd>{age(live?.seen ?? null)}</dd>
        <dt>Feed</dt><dd>{live?.source ?? "—"}</dd>
      </dl>
    </section>

    {#if currentHex}
      {#key currentHex}
        <DatalinkSection hex={currentHex} />
      {/key}
    {/if}

    <footer>
      <div class="btnrow">
        <button
          class="ib"
          class:active={$followHex === currentHex}
          on:click={() =>
            followHex.set($followHex === currentHex ? null : currentHex)}
        >
          <Icon name="crosshair" size={14} />
          {$followHex === currentHex ? "Following" : "Follow"}
        </button>
        <button
          class="ib"
          class:active={currentHex ? $pinned.includes(currentHex) : false}
          on:click={() => currentHex && togglePin(currentHex)}
        >
          <Icon name="pin" size={14} />
          {currentHex && $pinned.includes(currentHex) ? "Pinned" : "Pin"}
        </button>
      </div>
      <button class="ib" on:click={watchThis}>
        <Icon name="plus" size={14} /> Watch this aircraft
      </button>
    </footer>
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
    position: relative;
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin: -12px -14px 8px;
    padding: 10px 12px;
    min-height: 40px;
  }
  header.has-photo {
    align-items: flex-end;
    min-height: 132px;
    border-radius: 10px 10px 0 0;
    overflow: hidden;
    padding: 10px 12px 8px;
  }
  .hero-img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center 45%;
  }
  .hero-scrim {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      180deg,
      rgba(13, 17, 23, 0.15) 0%,
      rgba(13, 17, 23, 0.05) 45%,
      rgba(13, 17, 23, 0.82) 100%
    );
  }
  .nav {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    border: none;
    background: rgba(0, 0, 0, 0.4);
    color: #fff;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 5px 7px;
    border-radius: 4px;
    z-index: 2;
  }
  .nav:hover {
    background: rgba(0, 0, 0, 0.65);
  }
  .nav.prev {
    left: 6px;
  }
  .nav.next {
    right: 6px;
  }
  .dots {
    position: absolute;
    top: 8px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    gap: 4px;
    z-index: 2;
  }
  .dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.4);
  }
  .dot.on {
    background: #fff;
  }
  .title {
    position: relative;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .cs {
    font-size: 19px;
    font-weight: 700;
  }
  header.has-photo .cs {
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.9);
  }
  .credit {
    position: absolute;
    right: 8px;
    bottom: 6px;
    font-size: 9px;
    color: rgba(255, 255, 255, 0.75);
    text-decoration: none;
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.9);
    max-width: 60%;
    text-align: right;
  }
  .credit:hover {
    color: #fff;
  }
  .tag {
    font-size: 10px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 4px;
  }
  .tag.mil {
    background: #274d3d;
    color: #7ee2b8;
  }
  .tag.emg {
    background: var(--emergency);
    color: #fff;
  }
  .close {
    position: absolute;
    top: 6px;
    right: 8px;
    border: none;
    background: transparent;
    display: inline-flex;
    align-items: center;
    color: var(--text);
    z-index: 3;
  }
  header.has-photo .close {
    color: #fff;
    background: rgba(0, 0, 0, 0.35);
    border-radius: 4px;
    padding: 4px;
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
  }
  dl {
    display: grid;
    grid-template-columns: 40% 60%;
    gap: 2px 8px;
    margin: 0;
  }
  dt {
    color: var(--text-dim);
  }
  dd {
    margin: 0;
    text-align: right;
  }
  .mono,
  .muted {
    color: var(--text-dim);
  }
  .mono {
    font-family: ui-monospace, monospace;
  }
  .route {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 6px;
  }
  .route .muted {
    display: block;
    font-size: 11px;
  }
  .arrow {
    color: var(--accent);
    display: inline-flex;
    align-items: center;
  }
  dd.vr :global(svg) {
    display: inline-block;
    vertical-align: middle;
    color: var(--text-dim);
  }
  .bar {
    margin-top: 8px;
    height: 5px;
    border-radius: 3px;
    background: var(--border);
    overflow: hidden;
  }
  .bar-fill {
    height: 100%;
    background: var(--accent);
  }
  .prog {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-dim);
    margin-top: 3px;
  }
  .eta {
    margin: 4px 0 0;
    font-size: 12px;
  }
  .src {
    margin: 2px 0 0;
    font-size: 10px;
    color: var(--text-dim);
  }
  .err {
    color: var(--emergency);
  }
  footer {
    margin-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  footer button {
    width: 100%;
  }
  .btnrow {
    display: flex;
    gap: 6px;
  }
  .btnrow button {
    flex: 1;
  }
  button.ib {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
  }
</style>
