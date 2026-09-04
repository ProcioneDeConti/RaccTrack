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

  // window azimuth ticks: every 15°, spanning a little past the edges
  $: winTicks = (() => {
    const out: { b: number; x: number; label: string | null }[] = [];
    const from = Math.floor((center - HORIZON_FOV / 2 - 15) / 15) * 15;
    for (let b = from; b <= center + HORIZON_FOV / 2 + 15; b += 15) {
      const x = bearingToX(wrap360(b), center, width);
      if (x == null) continue;
      const wb = wrap360(b);
      out.push({ b: wb, x, label: wb % 45 === 0 ? compass(wb) : null });
    }
    return out;
  })();

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
        <!-- 360° compass ribbon, centred on the view direction -->
        <g class="ribbon">
          <rect x="0" y="0" width={width} height={RIBBON_H} class="ribbon-bg" />
          {#each Array(24) as _, k}
            {@const b = k * 15}
            {@const x = width / 2 + (bearingDelta(b, center) / 360) * width}
            <line x1={x} y1={b % 45 === 0 ? 4 : 10} x2={x} y2={RIBBON_H} class="tick" />
            {#if b % 45 === 0}
              <text {x} y="10" dominant-baseline="middle" class="rlabel">
                {compass(b)}
              </text>
            {/if}
          {/each}
          {#each $horizonTargets as t (t.hex + "r")}
            {@const x = width / 2 + (bearingDelta(t.bearingDeg, center) / 360) * width}
            <circle cx={x} cy={RIBBON_H - 3} r="2" fill={t.color} />
          {/each}
          {#if $horizonBodies && $horizonBodies.sun.elevation > -2}
            <circle
              cx={width / 2 + (bearingDelta($horizonBodies.sun.azimuth, center) / 360) * width}
              cy={RIBBON_H / 2}
              r="3"
              class="sun"
            />
          {/if}
          {#if $horizonBodies && $horizonBodies.moon.elevation > -2}
            <circle
              cx={width / 2 + (bearingDelta($horizonBodies.moon.azimuth, center) / 360) * width}
              cy={RIBBON_H / 2}
              r="3"
              class="moon"
            />
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
              <circle cx={sx} cy={yFor($horizonBodies.sun.elevation)} r="7" class="sun" />
            {/if}
          {/if}
          {#if $horizonBodies && $horizonBodies.moon.elevation > -1}
            {@const mx = bearingToX($horizonBodies.moon.azimuth, center, width)}
            {#if mx != null}
              <circle
                cx={mx}
                cy={yFor($horizonBodies.moon.elevation)}
                r="6"
                class="moon"
                style="opacity:{0.3 + 0.6 * $horizonBodies.moonIllum}"
              />
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
    left: 0;
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
  .sun {
    fill: #f5c518;
  }
  .moon {
    fill: #cdd6e0;
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
