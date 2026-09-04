<script lang="ts">
  import Panel from "../ui/Panel.svelte";
  import Icon from "../ui/Icon.svelte";
  import { geocode, updateSettings } from "../api/backend";
  import { humanizeError } from "../ui/errors";
  import { places, selectedHex, flyTo } from "../state";
  import type { GeoResult, Place } from "../api/types";

  export let onClose: () => void;

  let query = "";
  let results: GeoResult[] = [];
  let searching = false;
  let error = "";
  let expanded: string | null = null;

  async function search() {
    const q = query.trim();
    if (!q) return;
    searching = true;
    error = "";
    results = [];
    try {
      results = await geocode(q);
      if (results.length === 0) error = "No matches.";
    } catch (e) {
      error = humanizeError(e);
    } finally {
      searching = false;
    }
  }

  async function save(next: Place[]) {
    const s = await updateSettings({ places: next });
    places.set(s.places ?? []);
  }

  const newId = () =>
    crypto.randomUUID?.() ??
    `p${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;

  function addPlace(r: GeoResult) {
    const p: Place = {
      id: newId(),
      label: r.label,
      lat: r.lat,
      lon: r.lon,
      kind: r.kind,
      bbox: r.bbox,
      primary: $places.length === 0,
      alert: { enabled: false, radiusNm: 10, ceilingFt: null, notableOnly: false },
    };
    void save([...$places, p]);
    results = [];
    query = "";
    expanded = p.id;
  }

  const patch = (id: string, fn: (p: Place) => Place) =>
    save($places.map((p) => (p.id === id ? fn(p) : p)));

  function setPrimary(id: string) {
    void save($places.map((p) => ({ ...p, primary: p.id === id })));
  }
  function remove(id: string) {
    void save($places.filter((p) => p.id !== id));
  }
  function toggleAlert(id: string, on: boolean) {
    void patch(id, (p) => ({ ...p, alert: { ...p.alert, enabled: on } }));
    if (on) expanded = id;
  }
  const num = (v: string, fallback: number) => {
    const n = parseFloat(v);
    return Number.isFinite(n) ? n : fallback;
  };
</script>

<Panel title="Places & alerts" {onClose} width={330}>
  <form class="search" on:submit|preventDefault={search}>
    <input
      type="text"
      placeholder="city, ZIP, address, or lat, lon"
      bind:value={query}
    />
    <button type="submit" disabled={searching}>{searching ? "…" : "Add"}</button>
  </form>
  {#if error}<p class="err">{error}</p>{/if}
  {#if results.length}
    <ul class="results">
      {#each results as r}
        <li>
          <button on:click={() => addPlace(r)}>
            <span class="lbl">{r.label}</span>
            <span class="kind">{r.kind}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if $places.length === 0}
    <p class="muted">
      No places yet. Add one above — set it as your primary for the "go to"
      button and range rings, and switch on a proximity alert to get notified
      when aircraft pass nearby.
    </p>
  {:else}
    <ul class="places">
      {#each $places as p (p.id)}
        <li>
          <div class="head">
            <button
              class="star"
              class:on={p.primary}
              title={p.primary ? "Primary place" : "Make primary"}
              aria-label="Make primary"
              on:click={() => setPrimary(p.id)}
            >
              <Icon name="star" size={13} />
            </button>
            <button
              class="name"
              title="Fly to {p.label}"
              on:click={() => {
                selectedHex.set(null);
                flyTo.set({ lat: p.lat, lon: p.lon, zoom: p.bbox ? 8 : 11 });
              }}>{p.label}</button
            >
            <label class="al" title="Proximity alert">
              <input
                type="checkbox"
                checked={p.alert.enabled}
                on:change={(e) => toggleAlert(p.id, e.currentTarget.checked)}
              />
              alert
            </label>
            <button
              class="rm"
              title="Remove"
              aria-label="Remove"
              on:click={() => remove(p.id)}
            >
              <Icon name="x" size={12} />
            </button>
          </div>
          <div class="coord">{p.lat.toFixed(3)}, {p.lon.toFixed(3)}</div>

          {#if p.alert.enabled && expanded === p.id}
            <div class="cfg">
              <label>
                Within
                <input
                  type="number"
                  min="0.5"
                  max="250"
                  step="0.5"
                  value={p.alert.radiusNm}
                  on:change={(e) =>
                    patch(p.id, (x) => ({
                      ...x,
                      alert: { ...x.alert, radiusNm: num(e.currentTarget.value, 10) },
                    }))}
                /> nm
              </label>
              <label>
                Below
                <input
                  type="number"
                  min="0"
                  step="500"
                  placeholder="any"
                  value={p.alert.ceilingFt ?? ""}
                  on:change={(e) =>
                    patch(p.id, (x) => ({
                      ...x,
                      alert: {
                        ...x.alert,
                        ceilingFt: e.currentTarget.value
                          ? num(e.currentTarget.value, 0)
                          : null,
                      },
                    }))}
                /> ft
              </label>
              <label class="cb">
                <input
                  type="checkbox"
                  checked={p.alert.notableOnly}
                  on:change={(e) =>
                    patch(p.id, (x) => ({
                      ...x,
                      alert: { ...x.alert, notableOnly: e.currentTarget.checked },
                    }))}
                /> military / interesting only
              </label>
            </div>
          {:else if p.alert.enabled}
            <button class="cfg-open" on:click={() => (expanded = p.id)}>
              within {p.alert.radiusNm} nm{p.alert.ceilingFt
                ? ` · below ${p.alert.ceilingFt.toLocaleString()} ft`
                : ""}{p.alert.notableOnly ? " · notable only" : ""}
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</Panel>

<style>
  .search {
    display: flex;
    gap: 4px;
    margin-bottom: 6px;
  }
  .search input {
    flex: 1;
    min-width: 0;
  }
  .err {
    color: var(--emergency);
    font-size: 11px;
    margin: 0 0 6px;
  }
  .muted {
    color: var(--text-dim);
    font-size: 12px;
    line-height: 1.5;
  }
  .results {
    list-style: none;
    margin: 0 0 8px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-height: 170px;
    overflow-y: auto;
  }
  .results button {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    text-align: left;
    font-size: 12px;
    padding: 5px 8px;
    overflow: hidden;
  }
  .results .lbl {
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .results .kind {
    color: var(--text-dim);
    font-size: 10px;
    flex-shrink: 0;
  }
  .places {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .places > li {
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 6px 7px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .star {
    border: none;
    background: transparent;
    color: var(--text-dim);
    display: inline-flex;
    padding: 2px;
    flex: 0 0 auto;
  }
  .star.on {
    color: var(--accent);
  }
  .name {
    flex: 1 1 0;
    min-width: 0;
    border: none;
    background: transparent;
    text-align: left;
    padding: 0;
    font-size: 12px;
    font-weight: 600;
    /* truncation on the flex item itself — the reliable combo */
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .coord {
    font-size: 10px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
    margin: 1px 0 0 25px;
  }
  .al {
    flex: 0 0 auto;
    font-size: 10px;
    color: var(--text-dim);
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .rm {
    flex: 0 0 auto;
    border: none;
    background: transparent;
    color: var(--text-dim);
    display: inline-flex;
    padding: 3px;
  }
  .rm:hover {
    color: var(--emergency);
  }
  .cfg {
    margin-top: 6px;
    display: flex;
    flex-wrap: wrap;
    gap: 6px 10px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .cfg label {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .cfg label.cb {
    flex-basis: 100%;
  }
  .cfg input[type="number"] {
    width: 58px;
  }
  .cfg-open {
    display: block;
    max-width: 100%;
    margin-top: 4px;
    border: none;
    background: transparent;
    color: var(--accent);
    font-size: 10px;
    padding: 2px 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
