# RaccTrack (ADS-B) — Feasibility Report

*(project originally scoped as "free ADS-B flight tracker")*

*Prepared 2026-09-03*

## Verdict

**Feasible** for a genuinely free, no-paywall desktop flight tracker — with two honest
limitations you should decide about up front:

1. **Coverage is ground-station-limited.** Free data comes from volunteer receivers.
   Land coverage in North America and Europe is excellent; oceans, polar routes, Africa,
   and parts of Asia/South America are patchy to nonexistent. The paid services partly
   fill this with satellite ADS-B (Aireon/Spire), which has no free equivalent.
2. **Scheduled-flight data (gates, scheduled times, delay status, cancellations) is not
   publicly available for free.** That is airline/airport/GDS data and is the core of what
   FlightStats sells. You can show everything the aircraft *broadcasts* and everything in
   open reference databases, but "flight AA100 is delayed 40 min, now boarding gate B12"
   is out of reach without a paid feed.

Everything else — a live map, per-aircraft detail (registration, type, operator, route,
photos, altitude/speed/heading/vertical rate, squawk, emergency flags, military/blocked
status) — is achievable with free sources.

---

## Free raw ADS-B feed sources

### Tier 1 — Community aggregator APIs (recommended primary source)

These are unfiltered global feeds from volunteer receiver networks. They do **not** honor
aircraft blocking lists, so you get more than FlightAware/FR24 show publicly.

| Source | Key required? | Notes |
|---|---|---|
| **adsb.lol** | No (today) | ODbL-licensed. REST API at `api.adsb.lol`, documented at `/docs`. Explicitly a **drop-in replacement for the ADSBexchange RapidAPI** (same JSON shape). Endpoints for radius search (`/v2/lat/{lat}/lon/{lon}/dist/{nm}`), by hex, by callsign, by registration, military-only, `/v2/all`. Dynamic rate limiting under load. **Also publishes free daily historical trace dumps to GitHub** (~38M traces/day, ~1 GB compressed). *Future:* API key obtainable by running a feeder. |
| **adsb.fi** | No | Same community model, free public API, ~1 request/second. |
| **airplanes.live** | **Now yes** | As of 2026 the free public API was **restricted to feeder IPs** after abuse (largely AI scrapers). Still fully usable — you just have to run a receiver and feed them. 1 req/s. |
| **ADSB IQ** | Feeder account | Free authenticated REST snapshot **plus a real-time WebSocket stream**, adsb.lol-v2 compatible, if you feed. |
| **ADSBHub** | Share-to-get | Raw data exchange, requires feeding to pull. |

**Risk to note:** the no-key services (adsb.lol, adsb.fi) have all signaled they *may*
move to feeder-gated keys, following airplanes.live. The durable answer is to run a feeder
(see Tier 3), which unlocks all of them permanently and adds MLAT.

### Tier 2 — OpenSky Network

- Genuinely free **REST API** for **non-commercial / research use only**.
- **Auth change (enforced since 18 March 2026):** OAuth2 client-credentials flow only;
  username/password is dead. You register an API client, get `client_id`/`client_secret`,
  exchange for a ~30-min bearer token.
- **Credit system:** ~4,000 credits/day authenticated, ~8,000/day if you feed. A small
  bounding-box query costs 1 credit; a global query costs 4. Enough for a regional live
  view refreshed every few seconds; not enough for continuous global polling.
- State vectors updateable ~every 5–10 s, 1-hour historical lookback via API, plus a
  larger historical archive for registered researchers.
- Coverage outside Europe/North America is uneven; no uptime SLA.
- **License:** non-commercial. If this app is ever monetized, OpenSky is out without a
  separate agreement.

### Tier 3 — Run your own receiver (the foundation, ~$30–60 one-time)

- RTL-SDR USB dongle + 1090 MHz antenna + `dump1090` / `readsb` + `tar1090`.
- Gives **unlimited, unthrottled, real-time raw data** for everything in line of sight
  (typ. 200–400 km radius, altitude-dependent).
- Feeding that to adsb.lol / adsb.fi / airplanes.live / OpenSky unlocks their full APIs,
  raises rate limits, and adds **MLAT** (multilateration positions for non-ADS-B Mode-S
  aircraft).
- The desktop app can read directly from a local/LAN receiver via the `readsb` JSON
  (`/data/aircraft.json`) or Beast/raw TCP (port 30005/30003) — zero network dependency
  for local traffic.

### Not free (for reference)

- **ADSBexchange API:** post-JETNET acquisition it's paid via RapidAPI, ~$10/mo for 10k calls.
- **FlightAware AeroAPI, FR24 API:** commercial pricing, effectively out.
- **AviationStack / AeroDataBox / aviation-edge:** freemium, but free tiers are hundreds of
  calls/month and focused on schedules, not live positions.

---

## Free enrichment / reference data

| Data | Source | License |
|---|---|---|
| Aircraft registration, type, operator, mil/civ (by ICAO hex) | **Mictronics DB**, distributed as `aircraft.csv.gz` via `wiedehopf/tar1090-db` | ODC-BY (attribution) |
| Same, as an API | **hexdb.io** (`/api/v1/aircraft/{hex}`) | free, avgeek project |
| Callsign → origin/destination airport (route) | **hexdb.io** route API, **adsb.lol** route API, VRS standing data | free |
| Aircraft photos | **planespotters.net** API | free, attribution required |
| Airports, runways, frequencies | **OurAirports** open data | public domain |
| Airline names/codes | OpenFlights, Mictronics | open, somewhat stale |

Gap: no good free source for **live scheduled-flight status**. OpenFlights schedules are
stale. This is the one FlightStats feature that can't be replicated.

---

## Legal / ToS considerations

- ADS-B is an unencrypted broadcast; receiving and displaying it is legal in the US and
  most countries (a few jurisdictions restrict *redistribution* — worth a note in docs).
- adsb.lol data is **ODbL** — if you redistribute the data or a derived database you must
  attribute and share-alike. Displaying it in an app is fine; re-publishing a bulk feed
  triggers obligations.
- OpenSky is **non-commercial only** and wants attribution.
- Respect per-service rate limits (1 req/s community, credit budget for OpenSky). Cache
  aggressively; never hammer.
- The community networks are *unfiltered* — you'll surface aircraft that owners have asked
  FAA LADD / PIA to hide. That's legal and is a feature for many users, but call it out.

---

## Recommended architecture (for the planning phase)

- **Shell:** Tauri (Rust core + web UI) for a small, fast, cross-platform binary, or
  Electron if web-stack velocity matters more than footprint.
- **Data layer:**
  - Primary: adsb.lol public API (bounding box around map viewport), 1–5 s poll.
  - Optional: user-configured local `readsb` feed (LAN) — takes priority for local traffic.
  - Optional: OpenSky OAuth client for wider-area fill-in, budgeted against credits.
  - Enrichment: bundle the Mictronics CSV at build time + refresh weekly; hexdb.io /
    planespotters on-demand with a local cache (SQLite).
- **Map:** MapLibre GL + free tiles (OpenFreeMap / Protomaps / self-hosted), or offline
  MBTiles.
- **Features that are free-data-achievable:** live map with trails, per-aircraft panel
  (photo, reg, type, operator, route, live telemetry), filters (mil, emergency squawks,
  altitude bands, type), alerts ("notify when hex X / type Y / squawk 7700 appears"),
  flight trail recording, local history DB, "interesting aircraft" feed.
- **Set expectations in-app:** no gate/schedule/delay data; coverage map showing where
  volunteer receivers exist.

---

## Bottom line

A free desktop tracker that **matches FlightAware/FR24's live map and aircraft-detail
experience over land in NA/Europe is very doable**. It will **not** match their ocean
coverage or FlightStats' schedule/delay product, because no free source for those exists.
The most robust design assumes the community APIs may become feeder-gated and therefore
supports (and gently encourages) the user running a ~$30 receiver.
