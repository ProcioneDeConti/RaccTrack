<script lang="ts">
  import { onDestroy } from "svelte";
  import { selectedHex, aircraft } from "../state";
  import { getAircraftDetail, addWatch } from "../api/backend";
  import type { AircraftDetail } from "../api/types";
  import {
    altitude,
    speed,
    verticalRate,
    degrees,
    age,
    squawkMeaning,
  } from "../format";

  let detail: AircraftDetail | null = null;
  let loading = false;
  let error: string | null = null;
  let currentHex: string | null = null;

  const unsub = selectedHex.subscribe((hex) => {
    currentHex = hex;
    detail = null;
    error = null;
    if (hex) void load(hex);
  });
  onDestroy(unsub);

  async function load(hex: string) {
    loading = true;
    try {
      const d = await getAircraftDetail(hex);
      if (currentHex === hex) detail = d;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // Live telemetry comes from the aircraft store so it updates between detail fetches.
  $: live = currentHex ? ($aircraft.get(currentHex) ?? detail?.aircraft ?? null) : null;
  $: sqMeaning = squawkMeaning(live?.squawk ?? null);

  function close() {
    selectedHex.set(null);
  }

  async function watchThis() {
    if (!live) return;
    await addWatch("hex", live.hex, live.flight ?? live.registration ?? live.hex);
  }
</script>

{#if currentHex}
  <aside class="panel">
    <header class:has-photo={!!detail?.photo}>
      {#if detail?.photo}
        <img
          class="hero-img"
          src={detail.photo.largeUrl ?? detail.photo.thumbnailUrl}
          alt={live?.registration ?? currentHex}
        />
        <div class="hero-scrim"></div>
      {/if}
      <button class="close" on:click={close} title="Close">✕</button>
      <div class="title">
        <span class="cs">{live?.flight ?? live?.registration ?? currentHex}</span>
        {#if live?.military}<span class="tag mil">MIL</span>{/if}
        {#if live?.emergency && live.emergency !== "none"}
          <span class="tag emg">{live.emergency.toUpperCase()}</span>
        {/if}
      </div>
      {#if detail?.photo}
        <a
          class="credit"
          href={detail.photo.link ?? undefined}
          target="_blank"
          rel="noreferrer"
        >
          {#if detail.photo.source === "wikipedia"}
            representative photo — {detail.photo.photographer ?? "model"} · Wikipedia
          {:else}
            © {detail.photo.photographer ?? "unknown"} · planespotters.net
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
        <dt>Operator</dt><dd>{detail?.ownerOperator ?? "—"}</dd>
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
          <div class="arrow">→</div>
          <div>
            <strong>{detail.route.destinationIcao ?? "?"}</strong>
            <span class="muted">{detail.route.destinationName ?? ""}</span>
          </div>
        </div>
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
        <dt>Vertical rate</dt><dd>{verticalRate(live?.baroRate ?? live?.geomRate ?? null)}</dd>
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

    <section class="unavail">
      <h4>Not available (no free source)</h4>
      <p class="muted">Scheduled times · gate / stand · delay &amp; cancellation status</p>
    </section>

    <footer>
      <button on:click={watchThis}>+ Watch this aircraft</button>
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
    font-size: 14px;
    line-height: 1;
    color: var(--text);
  }
  header.has-photo .close {
    color: #fff;
    background: rgba(0, 0, 0, 0.35);
    border-radius: 4px;
    padding: 2px 6px;
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
  }
  .err {
    color: var(--emergency);
  }
  footer {
    margin-top: 12px;
  }
  footer button {
    width: 100%;
  }
</style>
