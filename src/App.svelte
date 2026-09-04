<script lang="ts">
  import { onMount } from "svelte";
  import MapView from "./lib/map/MapView.svelte";
  import DetailPanel from "./lib/panel/DetailPanel.svelte";
  import LayersControl from "./lib/map/LayersControl.svelte";
  import AirportPanel from "./lib/map/AirportPanel.svelte";
  import ChartViewer from "./lib/charts/ChartViewer.svelte";
  import FilterBar from "./lib/filters/FilterBar.svelte";
  import WatchlistPanel from "./lib/watchlist/WatchlistPanel.svelte";
  import SettingsPanel from "./lib/settings/SettingsPanel.svelte";
  import StatusBar from "./lib/StatusBar.svelte";
  import AlertToast from "./lib/watchlist/AlertToast.svelte";
  import AircraftList from "./lib/aircraftlist/AircraftList.svelte";
  import SearchBox from "./lib/search/SearchBox.svelte";
  import PinnedBar from "./lib/panel/PinnedBar.svelte";
  import {
    applyDiff,
    resetAircraft,
    sourceStatus,
    home,
    goHomeSignal,
    pinned,
    visibleAircraft,
    emergencyCount,
  } from "./lib/state";
  import {
    onDiff,
    onAlert,
    onSourceStatus,
    onEmergencyCount,
    getSnapshot,
    getSourceStatus,
    getSettings,
  } from "./lib/api/backend";
  import { pushAlert, refreshWatch } from "./lib/watchlist/watchStore";
  import { units } from "./lib/format";
  import type { Bbox } from "./lib/map/region";
  import iconUrl from "./assets/icon.png";

  let panel: "none" | "watchlist" | "settings" = "none";
  let showList = false;
  let notificationsEnabled = true;
  let mapView: MapView;

  function currentBbox(): Bbox | null {
    return mapView?.currentBounds?.() ?? null;
  }

  onMount(() => {
    const unlisteners: Array<Promise<() => void>> = [];

    unlisteners.push(onDiff(applyDiff));
    unlisteners.push(
      onAlert((a) => {
        pushAlert(a);
        if (notificationsEnabled) void notify(a.reason, a.hex);
      }),
    );
    unlisteners.push(onSourceStatus((s) => sourceStatus.set(s)));
    unlisteners.push(onEmergencyCount((n) => emergencyCount.set(n)));

    (async () => {
      try {
        const s = await getSettings();
        notificationsEnabled = s.notificationsEnabled;
        units.set(s.units);
        pinned.set(s.pinned ?? []);
      } catch {
        /* backend still starting */
      }
      try {
        const snap = await getSnapshot();
        resetAircraft([...snap.added, ...snap.updated], snap.total);
      } catch {
        /* ignore */
      }
      try {
        sourceStatus.set(await getSourceStatus());
      } catch {
        /* ignore */
      }
      await refreshWatch();
    })();

    return () => {
      unlisteners.forEach((p) => p.then((u) => u()));
    };
  });

  async function notify(body: string, title: string) {
    try {
      const n = await import("@tauri-apps/plugin-notification");
      let granted = await n.isPermissionGranted();
      if (!granted) granted = (await n.requestPermission()) === "granted";
      if (granted) n.sendNotification({ title: `Alert: ${title}`, body });
    } catch {
      /* notifications unavailable */
    }
  }

  function toggle(p: "watchlist" | "settings") {
    panel = panel === p ? "none" : p;
    if (panel !== "none") showList = false;
  }
  $: if (showList) panel = "none";
</script>

<main>
  <div class="toolbar">
    <div class="brand"><img src={iconUrl} alt="" /> RaccTrack <span class="sub">(ADS-B)</span></div>
    <SearchBox />
    <button class:active={showList} on:click={() => (showList = !showList)}>
      List{#if $visibleAircraft.length} ({$visibleAircraft.length}){/if}
    </button>
    <button class:active={panel === "watchlist"} on:click={() => toggle("watchlist")}>
      Watchlist
    </button>
    <button class:active={panel === "settings"} on:click={() => toggle("settings")}>
      Settings
    </button>
    <button
      class="home-btn"
      title={$home ? `Go to home — ${$home.label}` : "Set a home location in Settings"}
      disabled={!$home}
      on:click={() => goHomeSignal.update((n) => n + 1)}
    >
      ⌂ Home
    </button>
  </div>

  <div class="stage">
    <MapView bind:this={mapView} />
    <FilterBar />
    <LayersControl />
    {#if showList}
      <AircraftList onClose={() => (showList = false)} />
    {/if}
    {#if panel === "watchlist"}
      <WatchlistPanel onClose={() => (panel = "none")} />
    {:else if panel === "settings"}
      <SettingsPanel onClose={() => (panel = "none")} {currentBbox} />
    {/if}
    <DetailPanel />
    <AirportPanel />
    <ChartViewer />
    <PinnedBar />
    <AlertToast />
  </div>
  <StatusBar />
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .toolbar {
    height: 40px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border);
  }
  .brand {
    font-weight: 700;
    margin-right: 8px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .brand img {
    width: 20px;
    height: 20px;
    border-radius: 4px;
  }
  .brand .sub {
    font-weight: 400;
    color: var(--text-dim);
  }
  .stage {
    position: relative;
    flex: 1 1 0;
    min-height: 0;
  }
</style>
