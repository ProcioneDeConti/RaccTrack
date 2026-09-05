<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import Panel from "../ui/Panel.svelte";
  import Message from "../ui/Message.svelte";
  import { modeAcContacts } from "../api/backend";
  import type { GhostContact } from "../api/types";
  import { altitude } from "../format";

  export let onClose: () => void;

  let contacts: GhostContact[] = [];
  let loading = true;
  let timer: ReturnType<typeof setInterval>;

  async function load() {
    try {
      contacts = await modeAcContacts();
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
    timer = setInterval(load, 2000);
  });
  onDestroy(() => clearInterval(timer));

  const age = (ms: number) => {
    const s = Math.round((Date.now() - ms) / 1000);
    if (s < 60) return `${s}s ago`;
    return `${Math.round(s / 60)}m ago`;
  };
</script>

<Panel title="Mode A/C contacts" {onClose} width={330} bodyPad={false}>
  <div class="note">
    Legacy ATCRBS transponder replies detected nearby — these carry no ICAO
    address and no position, so they can't be plotted or identified, only
    listed. Each reply's bits could mean a squawk <em>or</em> an altitude
    (a passive receiver can't tell which); both possible readings are shown.
    Needs "Direct RTL-SDR" enabled in Settings.
  </div>

  <div class="scroll">
    {#if loading && contacts.length === 0}
      <Message kind="loading">Loading…</Message>
    {:else if contacts.length === 0}
      <Message kind="empty">
        Nothing detected recently. Legacy Mode A/C transponders are
        increasingly rare — most modern aircraft use Mode S instead.
      </Message>
    {:else}
      <ul>
        {#each contacts as c (c.possibleSquawk + c.firstSeenMs)}
          <li>
            <div class="row">
              <span class="squawk">{c.possibleSquawk}</span>
              <span class="alt">
                {c.possibleAltitudeFt != null ? altitude(c.possibleAltitudeFt) : "—"}
              </span>
              <span class="replies">{c.replies}×</span>
            </div>
            <div class="sub">last seen {age(c.lastSeenMs)}</div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</Panel>

<style>
  .note {
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.5;
  }
  .scroll {
    padding: 4px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    padding: 6px 8px;
    border-radius: 5px;
    font-size: 11px;
  }
  li:hover {
    background: var(--bg-elev);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .squawk {
    font-weight: 600;
    font-family: ui-monospace, monospace;
    color: var(--text);
    min-width: 40px;
  }
  .alt {
    flex: 1 1 auto;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .replies {
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .sub {
    margin-top: 1px;
    color: var(--text-dim);
    font-size: 10px;
  }
</style>
