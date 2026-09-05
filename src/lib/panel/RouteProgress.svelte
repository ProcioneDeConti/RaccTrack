<script lang="ts">
  import { onDestroy } from "svelte";
  import type { AircraftDetail } from "../api/types";
  import Icon from "../ui/Icon.svelte";
  import { fmtDistanceNm, fmtDuration } from "../geo";

  export let detail: AircraftDetail;
  /** The progress object computed in DetailPanel (`computeProgress`); null when
   *  the route's airport coordinates aren't known. */
  export let prog:
    | {
        total: number;
        known: boolean;
        stale: boolean;
        flown: number;
        toGo: number;
        pct: number;
        etaHrs: number | null;
        updatedYear: number | null;
      }
    | null;

  // `now` drives the "elapsed" / ETA clock. Refresh it on a slow tick so the
  // panel doesn't need a full reload to stay roughly current.
  let now = Date.now();
  const t = setInterval(() => (now = Date.now()), 20_000);
  onDestroy(() => clearInterval(t));

  $: r = detail.route;
  $: pct = prog ? Math.max(0, Math.min(100, prog.pct)) : 0;
  $: dep = detail.airborneSince;
  $: elapsedHrs = dep != null ? (now - dep) / 3_600_000 : null;
  $: etaMs =
    prog?.etaHrs != null ? now + prog.etaHrs * 3_600_000 : null;

  function code(icao: string | null | undefined): string {
    if (!icao) return "???";
    // US "K" prefix → drop it for a cleaner 3-letter look, like FR24.
    return icao.length === 4 && icao.startsWith("K") ? icao.slice(1) : icao;
  }
  function city(name: string | null | undefined): string {
    if (!name) return "";
    return (
      name
        .replace(
          /\s+(International|Regional|Municipal|County|Airport|Airfield|Air Force Base|AFB|Field)\b.*$/i,
          "",
        )
        .trim() || name
    );
  }
  function clock(ms: number): string {
    return new Date(ms).toLocaleTimeString([], {
      hour: "numeric",
      minute: "2-digit",
    });
  }
</script>

<div class="fp">
  <div class="ends">
    <div class="end">
      <div class="code">{code(r?.originIcao)}</div>
      <div class="city" title={r?.originName ?? ""}>{city(r?.originName)}</div>
    </div>
    <span class="mid"><Icon name="plane" size={18} /></span>
    <div class="end r">
      <div class="code">{code(r?.destinationIcao)}</div>
      <div class="city" title={r?.destinationName ?? ""}>
        {city(r?.destinationName)}
      </div>
    </div>
  </div>

  <div class="track" class:dim={!prog?.known}>
    <span class="orig"></span>
    <div class="flown" style="width:{pct}%"></div>
    <span class="ac" style="left:{pct}%"><Icon name="plane" size={15} /></span>
  </div>

  {#if prog?.known}
    <div class="grid">
      <div class="cell">
        <div class="lbl">{detail.sawDeparture ? "Departed" : "Tracked since"}</div>
        <div class="val">{dep != null ? clock(dep) : "—"}</div>
        <div class="sub">
          {elapsedHrs != null ? `${fmtDuration(elapsedHrs)} ago` : ""} ·
          {fmtDistanceNm(prog.flown)} flown
        </div>
      </div>
      <div class="cell r">
        <div class="lbl">ETA<span class="est" title="estimated from ground speed"></span></div>
        <div class="val">{etaMs != null ? clock(etaMs) : "—"}</div>
        <div class="sub">
          {#if prog.etaHrs != null}in {fmtDuration(prog.etaHrs)} · {/if}{fmtDistanceNm(
            prog.toGo,
          )} to go
        </div>
      </div>
    </div>
  {:else if prog}
    <p class="note">
      {fmtDistanceNm(prog.total)} direct distance{#if prog.stale} · aircraft isn't on
        this route{/if}
    </p>
  {/if}

  <p class="caveat">
    great-circle estimate — no schedule data{#if prog?.updatedYear}
      · route data from {prog.updatedYear}{/if}
  </p>
</div>

<style>
  .fp {
    display: flex;
    flex-direction: column;
    gap: 6px;
    /* Lucide's plane points up-right; nudge to point along the track (east). */
    --plane-rot: 45deg;
  }
  .ends {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .end {
    flex: 1 1 0;
    min-width: 0;
  }
  .end.r {
    text-align: right;
  }
  .code {
    font-size: 22px;
    font-weight: 700;
    line-height: 1;
  }
  .city {
    font-size: 10px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-dim);
    margin-top: 3px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mid {
    flex: 0 0 auto;
    color: var(--accent);
    transform: rotate(var(--plane-rot));
  }
  .track {
    position: relative;
    height: 3px;
    margin: 12px 6px 8px;
    background: var(--border);
    border-radius: 2px;
  }
  .track.dim {
    opacity: 0.45;
  }
  .orig {
    position: absolute;
    left: 0;
    top: 50%;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
    transform: translate(-50%, -50%);
  }
  .flown {
    position: absolute;
    left: 0;
    top: 0;
    height: 100%;
    background: var(--accent);
    border-radius: 2px;
  }
  .ac {
    position: absolute;
    top: 50%;
    color: var(--accent);
    transform: translate(-50%, -50%) rotate(var(--plane-rot));
  }
  .grid {
    display: flex;
    gap: 8px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 10px;
  }
  .cell {
    flex: 1 1 0;
    min-width: 0;
  }
  .cell.r {
    text-align: right;
  }
  .lbl {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-dim);
  }
  .val {
    font-size: 15px;
    font-weight: 600;
    margin-top: 1px;
  }
  .sub {
    font-size: 10px;
    color: var(--text-dim);
    margin-top: 2px;
  }
  .est {
    display: inline-block;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: #3fb950;
    margin-left: 4px;
    vertical-align: middle;
  }
  .note,
  .caveat {
    margin: 0;
    color: var(--text-dim);
  }
  .note {
    font-size: 11px;
  }
  .caveat {
    font-size: 10px;
  }
</style>
