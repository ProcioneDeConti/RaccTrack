<script lang="ts">
  import { onDestroy } from "svelte";
  import Icon from "../ui/Icon.svelte";
  import { primaryPlace, selectedHex, hoveredHex, flyTo, aircraft } from "../state";
  import { compass } from "../geo";
  import {
    horizonOpen,
    horizonRangeNm,
    horizonCenterBearing,
    horizonTargets,
    horizonBodies,
    bearingToX,
    bearingDelta,
    elevationToFrac,
    wrap360,
    HORIZON_FOV,
  } from "../horizon";

  const RIBBON_H = 20;
  const WIN_H = 130;
  const PAD = 8;
  /** Degrees of arc the compass ribbon spans — a zoomed-out view of the window.
   *  Bodies further round than half this just scroll off; drag to bring them in. */
  const RIBBON_ARC = 260;

  let width = 720;

  $: center = $horizonCenterBearing;

  // --- panning ---
  // Window-level listeners (not pointer capture) so clicks on target dots still
  // land on their own <g> handler.
  let drag: { startX: number; startCenter: number; moved: number } | null = null;

  function startDrag(e: PointerEvent) {
    drag = { startX: e.clientX, startCenter: center, moved: 0 };
    window.addEventListener("pointermove", onDrag);
    window.addEventListener("pointerup", endDrag);
  }
  function onDrag(e: PointerEvent) {
    if (!drag) return;
    const dx = e.clientX - drag.startX;
    drag.moved = Math.max(drag.moved, Math.abs(dx));
    horizonCenterBearing.set(
      wrap360(drag.startCenter - dx * (HORIZON_FOV / width)),
    );
  }
  function endDrag() {
    window.removeEventListener("pointermove", onDrag);
    window.removeEventListener("pointerup", endDrag);
    setTimeout(() => (drag = null), 0); // let a trailing click read `moved`
  }
  onDestroy(endDrag);

  function pick(hex: string) {
    if (drag && drag.moved > 4) return;
    selectedHex.set(hex);
    const a = $aircraft.get(hex);
    if (a?.lat != null && a?.lon != null) flyTo.set({ lat: a.lat, lon: a.lon });
  }

  const yFor = (elevDeg: number) =>
    RIBBON_H + PAD + (WIN_H - 6) * (1 - elevationToFrac(elevDeg));

  const SUN_RAYS = [0, 45, 90, 135, 180, 225, 270, 315];

  /** Ribbon x for an azimuth, or null once it scrolls past the ribbon's arc. */
  const ribbonX = (azimuth: number) =>
    bearingToX(azimuth, center, width, RIBBON_ARC);

  /** SVG path for the Moon's lit portion, radius `r`, illuminated fraction `k`,
   *  lit limb on the right when `litRight`. Centred on the origin. */
  function moonPath(r: number, k: number, litRight: boolean): string {
    const kk = Math.max(0.03, Math.min(0.97, k));
    const ax = (r * Math.abs(1 - 2 * kk)).toFixed(2);
    const gibbous = kk > 0.5;
    const limb = litRight ? 1 : 0;
    const term = litRight ? (gibbous ? 1 : 0) : gibbous ? 0 : 1;
    return `M 0 ${-r} A ${r} ${r} 0 0 ${limb} 0 ${r} A ${ax} ${r} 0 0 ${term} 0 ${-r} Z`;
  }

  $: litRight =
    $horizonBodies != null &&
    bearingDelta($horizonBodies.sun.azimuth, $horizonBodies.moon.azimuth) > 0;

  /** Azimuth ticks every 15° across an arc `arcDeg` wide, centred on the view. */
  function ticksFor(arcDeg: number, c: number, w: number) {
    const out: { b: number; x: number; label: string | null }[] = [];
    const half = arcDeg / 2;
    const from = Math.floor((c - half - 15) / 15) * 15;
    for (let b = from; b <= c + half + 15; b += 15) {
      const x = bearingToX(wrap360(b), c, w, arcDeg);
      if (x == null) continue;
      const wb = wrap360(b);
      out.push({ b: wb, x, label: wb % 45 === 0 ? compass(wb) : null });
    }
    return out;
  }

  $: winTicks = ticksFor(HORIZON_FOV, center, width);
  $: ribbonTicks = ticksFor(RIBBON_ARC, center, width);
  // window extent drawn on the ribbon, so the two scales relate visibly
  $: winEdgeL = bearingToX(center - HORIZON_FOV / 2, center, width, RIBBON_ARC);
  $: winEdgeR = bearingToX(center + HORIZON_FOV / 2, center, width, RIBBON_ARC);

  const ELEV_LINES = [5, 15, 30, 60];

  const num = (v: string, fallback: number) => {
    const n = parseFloat(v);
    return Number.isFinite(n) ? n : fallback;
  };
</script>

<section class="horizon" bind:clientWidth={width} class:empty={!$primaryPlace}>
    <header>
      <span class="ttl">
        <Icon name="crosshair" size={12} />
        {#if $primaryPlace}Horizon — {$primaryPlace.label}{:else}Horizon view{/if}
      </span>
      {#if $primaryPlace}
        <label class="rng">
          range
          <input
            type="number"
            min="2"
            max="120"
            step="1"
            value={$horizonRangeNm}
            on:change={(e) =>
              horizonRangeNm.set(
                Math.min(120, Math.max(2, num(e.currentTarget.value, 40))),
              )}
          /> nm
        </label>
        <button
          class="snap"
          title="Face north"
          on:click={() => horizonCenterBearing.set(0)}>N</button
        >
      {/if}
      <button class="x" aria-label="Close" on:click={() => horizonOpen.set(false)}>
        <Icon name="x" size={13} />
      </button>
    </header>

    {#if !$primaryPlace}
      <p class="hint">
        Set a primary place in Places &amp; alerts to use the horizon view.
      </p>
    {:else}
      <svg
        class="stagesvg"
        viewBox="0 0 {width} {RIBBON_H + PAD + WIN_H + PAD}"
        on:pointerdown={startDrag}
        role="presentation"
      >
        <!-- compass ribbon: a scrolling arc (RIBBON_ARC wide) centred on the view -->
        <g class="ribbon">
          <rect x="0" y="0" width={width} height={RIBBON_H} class="ribbon-bg" />
          {#if winEdgeL != null && winEdgeR != null}
            <rect
              x={winEdgeL}
              y="0"
              width={winEdgeR - winEdgeL}
              height={RIBBON_H}
              class="win-extent"
            />
          {/if}
          {#each ribbonTicks as tk}
            <line
              x1={tk.x}
              y1={tk.label ? 4 : 10}
              x2={tk.x}
              y2={RIBBON_H}
              class="tick"
            />
            {#if tk.label}
              <text x={tk.x} y="10" dominant-baseline="middle" class="rlabel">
                {tk.label}
              </text>
            {/if}
          {/each}
          {#each $horizonTargets as t (t.hex + "r")}
            {@const rx = ribbonX(t.bearingDeg)}
            {#if rx != null}
              <circle cx={rx} cy={RIBBON_H - 3} r="2" fill={t.color} />
            {/if}
          {/each}
          {#if $horizonBodies && $horizonBodies.sun.elevation > -2}
            {@const rx = ribbonX($horizonBodies.sun.azimuth)}
            {#if rx != null}
              <g class="rbody sun" transform="translate({rx} {RIBBON_H / 2})">
                <circle r="6.5" class="rbody-bg" />
                {#each SUN_RAYS as a}
                  <line class="ray" x1="0" y1="-3.6" x2="0" y2="-5" transform="rotate({a})" />
                {/each}
                <circle r="2.8" />
              </g>
            {/if}
          {/if}
          {#if $horizonBodies && $horizonBodies.moon.elevation > -2}
            {@const rx = ribbonX($horizonBodies.moon.azimuth)}
            {#if rx != null}
              <g class="rbody moon" transform="translate({rx} {RIBBON_H / 2})">
                <circle r="6" class="rbody-bg" />
                <circle r="3.4" class="moon-dark" />
                <path class="moon-lit" d={moonPath(3.4, $horizonBodies.moonIllum, litRight)} />
                <circle r="3.4" class="moon-ring" />
              </g>
            {/if}
          {/if}
          <path
            d="M{width / 2 - 5} 0 L{width / 2 + 5} 0 L{width / 2} 6 Z"
            class="viewmark"
          />
        </g>

        <!-- magnified window: azimuth × elevation -->
        <g class="window">
          {#each ELEV_LINES as e}
            <line x1="0" y1={yFor(e)} x2={width} y2={yFor(e)} class="grid" />
            <text x="3" y={yFor(e) - 2} class="elab">{e}°</text>
          {/each}
          <line
            x1="0"
            y1={yFor(0)}
            x2={width}
            y2={yFor(0)}
            class="horizonline"
          />

          {#each winTicks as tk}
            <line x1={tk.x} y1={yFor(0)} x2={tk.x} y2={yFor(0) + 4} class="grid" />
            {#if tk.label}
              <text x={tk.x} y={yFor(0) + 14} class="azlab">{tk.label}</text>
            {/if}
          {/each}

          {#if $horizonBodies && $horizonBodies.sun.elevation > -1}
            {@const sx = bearingToX($horizonBodies.sun.azimuth, center, width)}
            {#if sx != null}
              <g
                class="body sun"
                transform="translate({sx} {yFor($horizonBodies.sun.elevation)})"
              >
                {#each SUN_RAYS as a}
                  <line class="ray" x1="0" y1="-8" x2="0" y2="-11" transform="rotate({a})" />
                {/each}
                <circle r="5.5" />
                <text y="-15">Sun</text>
              </g>
            {/if}
          {/if}
          {#if $horizonBodies && $horizonBodies.moon.elevation > -1}
            {@const mx = bearingToX($horizonBodies.moon.azimuth, center, width)}
            {#if mx != null}
              <g
                class="body moon"
                transform="translate({mx} {yFor($horizonBodies.moon.elevation)})"
              >
                <circle r="6" class="moon-dark" />
                <path class="moon-lit" d={moonPath(6, $horizonBodies.moonIllum, litRight)} />
                <circle r="6" class="moon-ring" />
                <text y="-15">Moon</text>
              </g>
            {/if}
          {/if}

          {#each $horizonTargets as t (t.hex)}
            {@const x = bearingToX(t.bearingDeg, center, width)}
            {#if x != null}
              {@const y = yFor(t.elevationDeg)}
              {@const ax =
                t.aheadBearingDeg != null
                  ? bearingToX(t.aheadBearingDeg, center, width)
                  : null}
              <!-- svelte-ignore a11y-no-static-element-interactions a11y-click-events-have-key-events -->
              <g
                class="tgt"
                class:sel={$selectedHex === t.hex}
                class:hov={$hoveredHex === t.hex}
                on:click={() => pick(t.hex)}
                on:mouseenter={() => hoveredHex.set(t.hex)}
                on:mouseleave={() => hoveredHex.set(null)}
              >
                {#if ax != null && t.aheadElevationDeg != null}
                  <line
                    x1={x}
                    y1={y}
                    x2={ax}
                    y2={yFor(t.aheadElevationDeg)}
                    stroke={t.color}
                    class="streak"
                  />
                {/if}
                <circle
                  cx={x}
                  cy={y}
                  r={$selectedHex === t.hex ? 5.5 : 4}
                  fill={t.emergency ? "var(--emergency)" : t.color}
                  class="dot"
                />
                {#if $selectedHex === t.hex || $hoveredHex === t.hex}
                  <text x={x} y={y - 8} class="tlab">{t.callsign}</text>
                {/if}
              </g>
            {/if}
          {/each}
        </g>
      </svg>

      <div class="foot">
        <span>{$horizonTargets.length} in range · drag to look around</span>
        <span class="cdir">{compass(center)} {Math.round(center)}°</span>
      </div>
    {/if}
</section>

<style>
  .horizon {
    position: absolute;
    left: var(--rail-w);
    right: 0;
    bottom: 0;
    z-index: 8;
    background: var(--bg-panel);
    border-top: 1px solid var(--border);
    box-shadow: 0 -6px 20px rgba(0, 0, 0, 0.28);
    padding: 6px 10px 4px;
    user-select: none;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 4px;
  }
  .ttl {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
  }
  .rng {
    font-size: 10px;
    color: var(--text-dim);
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .rng input {
    width: 44px;
  }
  .snap {
    font-size: 10px;
    font-weight: 700;
    padding: 2px 7px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-elev);
    color: var(--text);
  }
  .x {
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--text-dim);
    display: inline-flex;
    padding: 2px;
  }
  .x:hover {
    color: var(--text);
  }
  .hint {
    margin: 4px 2px 8px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .stagesvg {
    display: block;
    width: 100%;
    height: auto;
    cursor: grab;
    touch-action: none;
  }
  .stagesvg:active {
    cursor: grabbing;
  }
  .ribbon-bg {
    fill: var(--bg-elev);
  }
  .win-extent {
    fill: var(--sel);
  }
  .tick {
    stroke: var(--text-dim);
    stroke-width: 1;
    opacity: 0.5;
  }
  .rlabel {
    fill: var(--text-dim);
    font-size: 8px;
    text-anchor: middle;
  }
  .viewmark {
    fill: var(--accent);
  }
  .grid {
    stroke: var(--border);
    stroke-width: 1;
  }
  .horizonline {
    stroke: var(--text-dim);
    stroke-width: 1;
  }
  .elab,
  .azlab {
    fill: var(--text-dim);
    font-size: 8px;
  }
  .azlab {
    text-anchor: middle;
  }
  .body text {
    fill: var(--text);
    font-size: 9px;
    font-weight: 600;
    text-anchor: middle;
    paint-order: stroke;
    stroke: var(--bg-panel);
    stroke-width: 3;
  }
  .sun circle {
    fill: #f5c518;
  }
  .sun .ray {
    stroke: #f5c518;
    stroke-width: 1.6;
    stroke-linecap: round;
  }
  .moon-dark {
    fill: #2b3442;
  }
  .moon-lit {
    fill: #eef2f7;
  }
  .moon-ring {
    fill: none;
    stroke: #8b96a5;
    stroke-width: 0.75;
  }
  .rbody-bg {
    fill: var(--bg-elev);
  }
  .tgt {
    cursor: pointer;
  }
  .streak {
    stroke-width: 1.5;
    opacity: 0.45;
  }
  .dot {
    stroke: var(--bg-panel);
    stroke-width: 1;
  }
  .tgt.hov .dot,
  .tgt.sel .dot {
    stroke: var(--text);
    stroke-width: 1.5;
  }
  .tlab {
    fill: var(--text);
    font-size: 9px;
    font-weight: 600;
    text-anchor: middle;
    paint-order: stroke;
    stroke: var(--bg-panel);
    stroke-width: 3;
  }
  .foot {
    display: flex;
    justify-content: space-between;
    font-size: 9px;
    color: var(--text-dim);
    margin-top: 2px;
    font-variant-numeric: tabular-nums;
  }
</style>
