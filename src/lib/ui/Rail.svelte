<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./Icon.svelte";
  import { filters, layers, home, goHomeSignal, visibleAircraft } from "../state";
  import { isDefault } from "../filters/filters";

  /** Which panel is currently open ("none" when closed). */
  export let active: string;

  const dispatch = createEventDispatcher<{ select: string }>();

  $: filtersOn = !isDefault($filters);
  $: layersOn =
    $layers.airports ||
    $layers.weather ||
    $layers.radar ||
    $layers.airspace ||
    $layers.rangeRings;

  type Item = {
    id: string;
    icon: IconName;
    label: string;
    dot?: boolean;
    badge?: number;
  };
  $: top = [
    {
      id: "list",
      icon: "list",
      label: "Aircraft in view",
      badge: $visibleAircraft.length || undefined,
    },
    { id: "filters", icon: "filter", label: "Filters", dot: filtersOn },
    { id: "layers", icon: "layers", label: "Map layers", dot: layersOn },
    { id: "watchlist", icon: "star", label: "Watchlist" },
    { id: "events", icon: "activity", label: "Events & history" },
  ] satisfies Item[];
</script>

<nav class="rail" aria-label="Panels">
  <div class="group">
    {#each top as it}
      <button
        class:active={active === it.id}
        title={it.label}
        aria-label={it.label}
        aria-pressed={active === it.id}
        on:click={() => dispatch("select", it.id)}
      >
        <Icon name={it.icon} size={19} />
        {#if it.dot}<span class="dot"></span>{/if}
        {#if it.badge}<span class="badge">{it.badge > 99 ? "99+" : it.badge}</span>{/if}
      </button>
    {/each}
  </div>

  <div class="group">
    <button
      class="home"
      title={$home ? `Go to home — ${$home.label}` : "Set a home location in Settings"}
      aria-label="Go to home location"
      disabled={!$home}
      on:click={() => goHomeSignal.update((n) => n + 1)}
    >
      <Icon name="home" size={19} />
    </button>
    <button
      class:active={active === "settings"}
      title="Settings"
      aria-label="Settings"
      aria-pressed={active === "settings"}
      on:click={() => dispatch("select", "settings")}
    >
      <Icon name="settings" size={19} />
    </button>
    <button
      class:active={active === "about"}
      title="About RaccTrack"
      aria-label="About RaccTrack"
      aria-pressed={active === "about"}
      on:click={() => dispatch("select", "about")}
    >
      <Icon name="info" size={19} />
    </button>
  </div>
</nav>

<style>
  .rail {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: var(--rail-w);
    z-index: 12;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    align-items: center;
    padding: 8px 0;
    background: var(--bg-panel);
    border-right: 1px solid var(--border);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 100%;
    align-items: center;
  }
  button {
    position: relative;
    width: 34px;
    height: 34px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text-dim);
  }
  button:hover:not(:disabled) {
    background: var(--bg-elev);
    color: var(--text);
  }
  button.active {
    background: var(--accent);
    color: #fff;
  }
  button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .dot {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  button.active .dot {
    background: #fff;
  }
  .badge {
    position: absolute;
    bottom: 2px;
    right: 1px;
    min-width: 14px;
    height: 13px;
    padding: 0 3px;
    border-radius: 7px;
    background: var(--bg-elev);
    color: var(--text-dim);
    font-size: 9px;
    line-height: 13px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  button.active .badge {
    background: rgba(0, 0, 0, 0.25);
    color: #fff;
  }
</style>
