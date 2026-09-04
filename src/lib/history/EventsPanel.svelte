<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import Panel from "../ui/Panel.svelte";
  import Icon from "../ui/Icon.svelte";
  import Message from "../ui/Message.svelte";
  import { recentEvents, onAircraftEvent, clearHistory } from "../api/backend";
  import { aircraft, selectedHex, flyTo } from "../state";
  import type { AircraftEvent } from "../api/types";
  import {
    eventIcon,
    eventText,
    eventTime,
    eventIsUrgent,
    matchesFilter,
    type EventFilter,
  } from "./events";

  export let onClose: () => void;

  let all: AircraftEvent[] = [];
  let loading = true;
  let filter: EventFilter = "all";

  const FILTERS: { id: EventFilter; label: string }[] = [
    { id: "all", label: "All" },
    { id: "emergency", label: "Emergency" },
    { id: "squawk", label: "Squawk" },
    { id: "movement", label: "Takeoff/land" },
    { id: "watch", label: "Watchlist" },
  ];

  $: shown = all.filter((e) => matchesFilter(e.kind, filter));

  onMount(async () => {
    try {
      all = await recentEvents(500);
    } finally {
      loading = false;
    }
  });

  const stop = onAircraftEvent((e) => {
    all = [e, ...all].slice(0, 1000);
  });
  onDestroy(() => void stop.then((f) => f()));

  function label(e: AircraftEvent): string {
    const a = $aircraft.get(e.hex);
    return (e.flight ?? a?.flight ?? a?.registration ?? e.hex).trim();
  }

  function pick(e: AircraftEvent) {
    selectedHex.set(e.hex);
    const a = $aircraft.get(e.hex);
    if (a?.lat != null && a?.lon != null) flyTo.set({ lat: a.lat, lon: a.lon });
    else if (e.lat != null && e.lon != null)
      flyTo.set({ lat: e.lat, lon: e.lon, zoom: 7 });
  }

  async function wipe() {
    if (!confirm("Delete all recorded flight history?")) return;
    await clearHistory();
    all = [];
  }
</script>

<Panel title="Events & history" {onClose} width={330} bodyPad={false}>
  <svelte:fragment slot="actions">
    <button class="btn-link" on:click={wipe} title="Delete all history">Clear</button>
  </svelte:fragment>

  <div class="chips">
    {#each FILTERS as f}
      <button class:on={filter === f.id} on:click={() => (filter = f.id)}>
        {f.label}
      </button>
    {/each}
  </div>

  <div class="scroll">
    {#if loading}
      <Message kind="loading">Loading…</Message>
    {:else if shown.length === 0}
      <Message kind="empty" mascot={all.length === 0}>
        {all.length === 0
          ? "No events recorded yet. Squawk changes, takeoffs/landings and emergency squawks show up here as they happen."
          : "Nothing matches this filter."}
      </Message>
    {:else}
      <ul>
        {#each shown as e}
          <li class:urgent={eventIsUrgent(e.kind)}>
            <button class="row" on:click={() => pick(e)}>
              <Icon name={eventIcon(e.kind)} size={13} />
              <span class="cs">{label(e)}</span>
              <span class="what">{eventText(e)}</span>
              <time>{eventTime(e.at)}</time>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</Panel>

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
  }
  .chips button {
    font-size: 11px;
    padding: 2px 7px;
  }
  .chips button.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .scroll {
    padding: 4px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 6px;
    border: none;
    background: transparent;
    text-align: left;
    padding: 5px 6px;
    border-radius: 5px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .row:hover {
    background: var(--bg-elev);
  }
  .row :global(svg) {
    flex: 0 0 auto;
  }
  .cs {
    font-weight: 600;
    color: var(--text);
    flex: 0 0 auto;
    max-width: 90px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .what {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  time {
    flex: 0 0 auto;
    font-variant-numeric: tabular-nums;
  }
  li.urgent .what {
    color: var(--emergency);
    font-weight: 600;
  }
  li.urgent .row :global(svg) {
    color: var(--emergency);
  }
</style>
