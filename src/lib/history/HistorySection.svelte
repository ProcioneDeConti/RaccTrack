<script lang="ts">
  import { onDestroy } from "svelte";
  import { aircraftHistory, onAircraftEvent } from "../api/backend";
  import type { AircraftEvent } from "../api/types";
  import Icon from "../ui/Icon.svelte";
  import Message from "../ui/Message.svelte";
  import { eventIcon, eventText, eventTime, eventIsUrgent } from "./events";

  export let hex: string;

  let events: AircraftEvent[] = [];
  let loading = false;
  let loadedFor: string | null = null;

  $: if (hex && hex !== loadedFor) void load(hex);

  async function load(h: string) {
    loading = true;
    loadedFor = h;
    try {
      const rows = await aircraftHistory(h);
      if (loadedFor === h) events = rows;
    } catch {
      if (loadedFor === h) events = [];
    } finally {
      loading = false;
    }
  }

  const stop = onAircraftEvent((e) => {
    if (e.hex === hex) events = [e, ...events].slice(0, 200);
  });
  onDestroy(() => void stop.then((f) => f()));
</script>

<section class="panel-section">
  <h4>History</h4>
  {#if loading && events.length === 0}
    <Message kind="loading">Loading…</Message>
  {:else if events.length === 0}
    <Message kind="empty">
      Nothing recorded yet — squawk / callsign / takeoff / landing changes show
      up here while the app is running.
    </Message>
  {:else}
    <ul>
      {#each events as e}
        <li class:urgent={eventIsUrgent(e.kind)}>
          <Icon name={eventIcon(e.kind)} size={12} />
          <span class="what">{eventText(e)}</span>
          <time>{eventTime(e.at)}</time>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-dim);
  }
  li :global(svg) {
    flex: 0 0 auto;
  }
  .what {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  time {
    margin-left: auto;
    flex: 0 0 auto;
    font-variant-numeric: tabular-nums;
  }
  li.urgent .what {
    color: var(--emergency);
    font-weight: 600;
  }
  li.urgent :global(svg) {
    color: var(--emergency);
  }
</style>
