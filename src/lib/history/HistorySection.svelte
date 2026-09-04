<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    aircraftHistory,
    onAircraftEvent,
    sighting,
    setSightingNote,
  } from "../api/backend";
  import type { AircraftEvent, Sighting } from "../api/types";
  import Icon from "../ui/Icon.svelte";
  import Message from "../ui/Message.svelte";
  import { eventIcon, eventText, eventTime, eventIsUrgent } from "./events";

  export let hex: string;

  let events: AircraftEvent[] = [];
  let seen: Sighting | null = null;
  let note = "";
  let loading = false;
  let loadedFor: string | null = null;

  $: if (hex && hex !== loadedFor) void load(hex);

  async function load(h: string) {
    loading = true;
    loadedFor = h;
    try {
      const [rows, s] = await Promise.all([aircraftHistory(h), sighting(h)]);
      if (loadedFor === h) {
        events = rows;
        seen = s;
        note = s?.note ?? "";
      }
    } catch {
      if (loadedFor === h) {
        events = [];
        seen = null;
      }
    } finally {
      loading = false;
    }
  }

  async function saveNote() {
    if (!hex) return;
    await setSightingNote(hex, note);
  }

  const dayFmt = (ms: number) =>
    new Date(ms).toLocaleDateString([], { month: "short", day: "numeric", year: "numeric" });

  const stop = onAircraftEvent((e) => {
    if (e.hex === hex) events = [e, ...events].slice(0, 200);
  });
  onDestroy(() => void stop.then((f) => f()));
</script>

<section class="panel-section">
  <h4>History &amp; logbook</h4>

  {#if seen}
    <p class="seen">
      Seen <b>{seen.count}×</b> · first {dayFmt(seen.firstSeen)}
    </p>
  {/if}
  {#if seen}
    <input
      class="note"
      type="text"
      placeholder="Add a note…"
      bind:value={note}
      on:change={saveNote}
      on:blur={saveNote}
    />
  {/if}

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
  .seen {
    margin: 0 0 6px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .note {
    width: 100%;
    margin-bottom: 8px;
    font-size: 11px;
  }
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
