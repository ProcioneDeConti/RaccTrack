# RaccTrack (ADS-B)

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](./LICENSE)

A free, no-paywall ADS-B flight tracker for **North America**. Live aircraft map
plus the maximum publicly available detail per aircraft, using the free community
ADS-B aggregator networks — no subscription, no API key.

Built with **Tauri 2 + Rust** (backend) and **Svelte + MapLibre GL** (frontend).

## What it does

- **Live map** of aircraft in view, refreshed every 1–10 s, hard-locked to a North
  America bounding box. A yellow/black caution-tape frame marks that there is no
  data beyond the map.
- **Aircraft detail panel**: registration, type, operator, country, full broadcast
  telemetry (altitude, speed, track, vertical rate, squawk + decoded meaning, nav
  selected altitude, signal), route (origin → destination with airport names and a
  great-circle line), and a photo from planespotters.net.
- **Flight trails** for the selected aircraft, altitude-coloured, built live while
  the app runs (not persisted).
- **Filters**: altitude band, aircraft type, military only, on-ground, emergency
  squawk only.
- **Watchlist alerts**: desktop notification when a watched hex / registration /
  type / callsign appears, or when any emergency squawk (7500 / 7600 / 7700)
  shows up.
- **Places & proximity alerts**: save any number of locations; each can fire an
  alert when an aircraft passes within a set radius (and optionally below a set
  altitude, or only for military / interesting airframes).
- **Spotter logbook**: every airframe that's been in view, with first/last seen,
  a sighting count, your notes, and CSV export.
- **Offline basemap cache**: map tiles are cached locally as you pan; "Download
  current area" pre-fetches a region so it keeps rendering offline.
- **Local receiver** (optional): point it at your own dump1090-fa / readsb /
  tar1090 `aircraft.json` — no rate limits, sub-second updates, MLAT, your own
  coverage. Falls back to the community feeds if the receiver is unreachable.

## What it can't do (no free data source exists)

- Scheduled times, gate/stand, delay or cancellation status.
- Coverage over oceans and remote areas — the community feeds are volunteer
  ground receivers only.

See [`FEASIBILITY.md`](./FEASIBILITY.md) for the full research writeup.

## Data sources

| Purpose | Source | Notes |
|---|---|---|
| Live ADS-B | [adsb.lol](https://adsb.lol) (primary), [adsb.fi](https://adsb.fi) (fallback) | No key. ODbL. Viewport-scoped polling, ≤ ~1 req/s. |
| Live ADS-B (optional) | your own **dump1090-fa / readsb / tar1090** `aircraft.json` | Settings → Local ADS-B receiver. Tried first, community feeds fall back if unreachable. |
| Aircraft identity | Mictronics DB via [tar1090-db](https://github.com/wiedehopf/tar1090-db) | Bundled `src-tauri/assets/aircraft.csv.gz`. ODC-BY. |
| Airports | [OurAirports](https://ourairports.com/data/) | Bundled `src-tauri/assets/airports.csv` (+ `runways.csv`, `airport-frequencies.csv`). Public domain. |
| Navaids (VOR / DME / NDB) | [OurAirports](https://ourairports.com/data/) | Bundled `src-tauri/assets/navaids.csv`. Public domain. Map overlay only. |
| Airline callsigns | [OpenFlights](https://openflights.org/data.html) | Bundled `src-tauri/assets/airlines.dat`. ODbL. |
| Aircraft types | [Mictronics](https://www.mictronics.de/) `types.json` | Bundled `src-tauri/assets/actypes.json`. Engine/wake by ICAO type. |
| Routes | [hexdb.io](https://hexdb.io) | Cached in SQLite. |
| Photos | [planespotters.net](https://www.planespotters.net) | Cached 30 days; up to 6 per airframe. Attribution shown. |
| Basemap | [OpenFreeMap](https://openfreemap.org) "dark" | No key. © OpenStreetMap. |

## Development

Prerequisites: Node 20+, Rust (stable, MSVC on Windows), the Tauri v2 system deps,
WebView2 (bundled with Windows 11).

```bash
npm install
npm run tauri dev      # run the app
npm run tauri build    # produce a Windows installer (NSIS)
```

Checks:

```bash
npm run check                       # svelte-check (frontend types)
cargo test --manifest-path src-tauri/Cargo.toml
```

To refresh the bundled aircraft database:

```bash
curl -L -o src-tauri/assets/aircraft.csv.gz \
  https://github.com/wiedehopf/tar1090-db/raw/refs/heads/csv/aircraft.csv.gz
curl -L -o src-tauri/assets/airports.csv \
  https://davidmegginson.github.io/ourairports-data/airports.csv
curl -L -o src-tauri/assets/navaids.csv \
  https://davidmegginson.github.io/ourairports-data/navaids.csv
curl -L -o src-tauri/assets/airlines.dat \
  https://raw.githubusercontent.com/jpatokal/openflights/master/data/airlines.dat
curl -L -o src-tauri/assets/actypes.json \
  https://raw.githubusercontent.com/Mictronics/readsb/master/webapp/src/db/types.json
```

## Layout

```
src/                     Svelte frontend
  lib/map/               MapLibre view, NA region, coverage boundary, icons, tile proxy
  lib/panel/             aircraft detail panel (photo hero header)
  lib/filters/           filter bar + predicate
  lib/watchlist/         watchlist manager + alert toast/log
  lib/settings/          settings, home-location search, tile cache controls
src-tauri/src/
  ingest/                AircraftSource trait; adsb.lol / adsb.fi HTTP + local receiver (dump1090/readsb)
  state.rs               live aircraft map, diffing, trail buffers
  enrich/                identity DB, airports, routes, photos, country
  geocode.rs             home-location search (Photon + coordinate parsing)
  alerts.rs              watchlist storage + alert evaluation
  tiles.rs               SQLite tile cache + custom URI scheme + area download
  poller.rs              background polling loop
  commands.rs            Tauri command surface
```

## Etiquette

The community APIs are run by volunteers. This app polls only the current
viewport, rate-limits itself, caches aggressively, and sends a descriptive
User-Agent. Attribution for adsb.lol / adsb.fi (ODbL), CARTO / OpenFreeMap /
OpenStreetMap, and photo credits are shown in-app. Intended for personal,
non-commercial use.

`planespotters.net` requires a contact URL or email in the User-Agent to serve
photos — set one in **Settings → Aircraft photos**. Left blank, the app shows
representative model photos from Wikipedia instead.

## Versioning

[Semantic Versioning](https://semver.org/); see [`CHANGELOG.md`](./CHANGELOG.md)
for what changed in each release. `package.json` is the source of truth for
the version number — `tauri.conf.json` reads it directly, and
`src-tauri/Cargo.toml` is kept in sync automatically by `npm version`.

To cut a release: `npm version patch|minor|major`. This bumps `package.json`,
syncs `Cargo.toml`/`Cargo.lock` via `scripts/sync-cargo-version.mjs`, and (by
npm's default behavior) creates a commit and a matching `vX.Y.Z` git tag.

## License

[Apache License 2.0](./LICENSE). Third-party data and service attributions are in
[`NOTICE`](./NOTICE).

## Disclaimer

RaccTrack is informational/entertainment only — not for real-world
navigation, ATC, or aircraft separation, and data accuracy from community
ADS-B feeds isn't guaranteed. See [`DISCLAIMER.md`](./DISCLAIMER.md) for the
full safety and liability disclaimer (shown in-app on first launch).
