<script lang="ts">
  import { onMount } from "svelte";
  import MapView from "./lib/map/MapView.svelte";
  import DetailPanel from "./lib/panel/DetailPanel.svelte";
  import LayersPanel from "./lib/map/LayersPanel.svelte";
  import AirportPanel from "./lib/map/AirportPanel.svelte";
  import ChartViewer from "./lib/charts/ChartViewer.svelte";
  import FiltersPanel from "./lib/filters/FiltersPanel.svelte";
  import WatchlistPanel from "./lib/watchlist/WatchlistPanel.svelte";
  import PlacesPanel from "./lib/places/PlacesPanel.svelte";
  import PassesPanel from "./lib/passes/PassesPanel.svelte";
  import EventsPanel from "./lib/history/EventsPanel.svelte";
  import LogbookPanel from "./lib/history/LogbookPanel.svelte";
  import SettingsPanel from "./lib/settings/SettingsPanel.svelte";
  import AboutPanel from "./lib/about/AboutPanel.svelte";
  import DisclaimerModal from "./lib/about/DisclaimerModal.svelte";
  import StatusBar from "./lib/StatusBar.svelte";
  import AlertToast from "./lib/watchlist/AlertToast.svelte";
  import AircraftList from "./lib/aircraftlist/AircraftList.svelte";
  import SearchBox from "./lib/search/SearchBox.svelte";
  import PinnedBar from "./lib/panel/PinnedBar.svelte";
  import HorizonView from "./lib/horizon/HorizonView.svelte";
  import Rail from "./lib/ui/Rail.svelte";
  import {
    applyDiff,
    resetAircraft,
    sourceStatus,
    pinned,
    places,
    mapColors,
    uiTheme,
    emergencyCount,
    disclaimerOpen,
    coverageEnabled,
    coverageResult,
  } from "./lib/state";
  import {
    onDiff,
    onAlert,
    onSourceStatus,
    onEmergencyCount,
    getSnapshot,
    getSourceStatus,
    getSettings,
    computeCoverage,
  } from "./lib/api/backend";
  import { pushAlert, refreshWatch } from "./lib/watchlist/watchStore";
  import { horizonOpen } from "./lib/horizon";
  import { units } from "./lib/format";
  import type { Bbox } from "./lib/map/region";
  import iconUrl from "./assets/icon.png";

  type PanelId =
    | "none"
    | "list"
    | "filters"
    | "layers"
    | "watchlist"
    | "places"
    | "passes"
    | "events"
    | "logbook"
    | "settings"
    | "about";

  let panel: PanelId = "none";
  let notificationsEnabled = true;
  let mapView: MapView;
  /** True while the disclaimer is up because it's never been acknowledged —
   *  vs. reopened later from About, where a plain close (no re-ack) is fine. */
  let disclaimerFirstLaunch = false;

  function currentBbox(): Bbox | null {
    return mapView?.currentBounds?.() ?? null;
  }

  function select(e: CustomEvent<string>) {
    const id = e.detail as PanelId;
    panel = panel === id ? "none" : id;
  }
  const close = () => (panel = "none");

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
        places.set(s.places ?? []);
        mapColors.set(s.colors ?? { airspace: {}, geofenceFill: null, geofenceLine: null });
        uiTheme.set(s.uiTheme ?? "auto");
        if (!s.disclaimerAcknowledged) {
          disclaimerFirstLaunch = true;
          disclaimerOpen.set(true);
        }
        coverageEnabled.set(s.coverageEnabled ?? false);
        if (s.coverageEnabled) {
          // Backend caches the terrain calculation ~30 days, so this is
          // normally an instant cache hit, not a fresh external-API run.
          computeCoverage()
            .then((r) => coverageResult.set(r))
            .catch(() => {
              /* no RTL-SDR location set yet, or a fetch hiccup — fine, the
                 Settings panel surfaces the real error if the user asks */
            });
        }
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
</script>

<main>
  <div class="toolbar">
    <div class="brand">
      <img src={iconUrl} alt="" /> RaccTrack <span class="sub">(ADS-B)</span>
    </div>
    <SearchBox />
  </div>

  <div class="stage">
    <MapView bind:this={mapView} />

    <Rail active={panel} on:select={select} />

    {#if panel === "list"}
      <AircraftList onClose={close} />
    {:else if panel === "filters"}
      <FiltersPanel onClose={close} />
    {:else if panel === "layers"}
      <LayersPanel onClose={close} />
    {:else if panel === "watchlist"}
      <WatchlistPanel onClose={close} />
    {:else if panel === "places"}
      <PlacesPanel onClose={close} />
    {:else if panel === "passes"}
      <PassesPanel onClose={close} />
    {:else if panel === "events"}
      <EventsPanel onClose={close} />
    {:else if panel === "logbook"}
      <LogbookPanel onClose={close} />
    {:else if panel === "settings"}
      <SettingsPanel onClose={close} {currentBbox} />
    {:else if panel === "about"}
      <AboutPanel onClose={close} />
    {/if}

    <DetailPanel />
    <AirportPanel />
    <ChartViewer />
    <PinnedBar />
    {#if $horizonOpen}<HorizonView />{/if}
    <AlertToast />
  </div>
  <StatusBar />
  {#if $disclaimerOpen}
    <DisclaimerModal firstLaunch={disclaimerFirstLaunch} />
  {/if}
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
    gap: 10px;
    padding: 0 12px;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border);
  }
  .brand {
    font-weight: 700;
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
