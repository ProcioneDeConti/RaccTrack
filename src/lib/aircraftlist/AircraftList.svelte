<script lang="ts">
  import {
    visibleAircraft,
    selectedHex,
    hoveredHex,
    home,
    pinned,
    togglePin,
    followHex,
    flyTo,
  } from "../state";
  import type { Aircraft } from "../api/types";
  import { altitude, speed } from "../format";
  import { distanceNm, fmtDistanceNm } from "../geo";
  import Icon from "../ui/Icon.svelte";
  import Panel from "../ui/Panel.svelte";

  export let onClose: () => void;

  type Col = "cs" | "type" | "alt" | "gs" | "vs" | "sq" | "dist";
  const COLS: { c: Col; label: string }[] = [
    { c: "cs", label: "Callsign" },
    { c: "type", label: "Type" },
    { c: "alt", label: "Alt" },
    { c: "gs", label: "GS" },
    { c: "vs", label: "V/S" },
    { c: "sq", label: "Sqk" },
    { c: "dist", label: "Dist" },
  ];
  let sortCol: Col = "dist";
  let sortAsc = true;

  function setSort(c: Col) {
    if (sortCol === c) sortAsc = !sortAsc;
    else {
      sortCol = c;
      sortAsc = c === "cs" || c === "type" || c === "dist";
    }
  }

  function distOf(a: Aircraft): number {
    const h = $home;
    if (!h || a.lat === null || a.lon === null) return Infinity;
    return distanceNm(h.lat, h.lon, a.lat, a.lon);
  }

  function key(a: Aircraft): number | string {
    switch (sortCol) {
      case "cs":
        return (a.flight ?? a.registration ?? a.hex).trim();
      case "type":
        return a.typeCode ?? "zzz";
      case "alt":
        return a.onGround ? -1 : (a.altBaro ?? -2);
      case "gs":
        return a.groundSpeed ?? -1;
      case "vs":
        return a.baroRate ?? 0;
      case "sq":
        return a.squawk ?? "";
      case "dist":
        return distOf(a);
    }
  }

  $: rows = [...$visibleAircraft].sort((a, b) => {
    const ka = key(a);
    const kb = key(b);
    const c = ka < kb ? -1 : ka > kb ? 1 : 0;
    return sortAsc ? c : -c;
  });

  function pick(a: Aircraft) {
    selectedHex.set(a.hex);
    if (a.lat !== null && a.lon !== null)
      flyTo.set({ lat: a.lat, lon: a.lon });
  }

  function vsDir(a: Aircraft): "up" | "down" | null {
    const r = a.baroRate ?? a.geomRate ?? 0;
    if (r > 100) return "up";
    if (r < -100) return "down";
    return null;
  }
</script>

<Panel title="{rows.length} in view" {onClose} width={340} bodyPad={false}>
    <table>
      <thead>
        <tr>
          <th class="pin"></th>
          {#each COLS as col}
            <th
              class:sorted={sortCol === col.c}
              on:click={() => setSort(col.c)}
            >
              {col.label}{#if sortCol === col.c}<span class="caret"><Icon name={sortAsc ? "chevron-up" : "chevron-down"} size={11} /></span>{/if}
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each rows.slice(0, 400) as a (a.hex)}
          <tr
            class:sel={$selectedHex === a.hex}
            class:follow={$followHex === a.hex}
            on:click={() => pick(a)}
            on:mouseenter={() => hoveredHex.set(a.hex)}
            on:mouseleave={() => hoveredHex.set(null)}
          >
            <td class="pin">
              <button
                class:on={$pinned.includes(a.hex)}
                title="Pin"
                aria-label="Pin"
                on:click|stopPropagation={() => togglePin(a.hex)}><Icon name="pin" size={13} /></button
              >
            </td>
            <td class="cs">
              {(a.flight ?? a.registration ?? a.hex).trim()}
              {#if a.military}<span class="mil">M</span>{/if}
              {#if a.emergency && a.emergency !== "none"}<span class="emg">!</span>{/if}
            </td>
            <td>{a.typeCode ?? "—"}</td>
            <td class="num">{a.onGround ? "GND" : altitude(a.altBaro ?? null)}</td>
            <td class="num">{a.groundSpeed != null ? speed(a.groundSpeed) : "—"}</td>
            <td class="num vs">
              {#if vsDir(a) === "up"}<Icon name="arrow-up" size={11} />
              {:else if vsDir(a) === "down"}<Icon name="arrow-down" size={11} />{/if}
            </td>
            <td class="num">{a.squawk ?? "—"}</td>
            <td class="num">
              {distOf(a) === Infinity ? "—" : fmtDistanceNm(distOf(a))}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if rows.length === 0}
      <p class="empty">No aircraft in view.</p>
    {/if}
</Panel>

<style>
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 11px;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--bg-panel);
    text-align: left;
    padding: 5px 6px;
    color: var(--text-dim);
    cursor: pointer;
    white-space: nowrap;
    border-bottom: 1px solid var(--border);
  }
  th.sorted {
    color: var(--text);
  }
  .caret {
    display: inline-flex;
    vertical-align: middle;
    margin-left: 2px;
  }
  td {
    padding: 4px 6px;
    white-space: nowrap;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }
  tbody tr {
    cursor: pointer;
  }
  tbody tr:hover {
    background: var(--bg-elev);
  }
  tr.sel {
    background: rgba(76, 155, 232, 0.18);
  }
  tr.follow td.cs {
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  td.vs {
    padding-top: 0;
    padding-bottom: 0;
  }
  td.vs :global(svg) {
    display: inline-block;
    vertical-align: middle;
    color: var(--text-dim);
  }
  td.cs {
    font-weight: 600;
  }
  .mil {
    color: #7ee2b8;
    font-size: 9px;
    margin-left: 3px;
  }
  .emg {
    color: var(--emergency);
    font-weight: 700;
    margin-left: 3px;
  }
  td.pin,
  th.pin {
    width: 22px;
    padding-left: 4px;
    padding-right: 0;
  }
  td.pin button {
    border: none;
    background: transparent;
    padding: 0;
    display: inline-flex;
    align-items: center;
    color: var(--text-dim);
    opacity: 0.5;
  }
  td.pin button:hover {
    opacity: 1;
  }
  td.pin button.on {
    color: var(--accent);
    opacity: 1;
  }
  .empty {
    padding: 16px;
    text-align: center;
    color: var(--text-dim);
    font-size: 12px;
  }
</style>
