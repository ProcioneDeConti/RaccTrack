# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and versions follow [Semantic Versioning](https://semver.org/) — while the
app is pre-1.0, a minor bump may still include breaking changes.

## [Unreleased]

### Added

- Mode S enhanced-surveillance decode: DF20/21 Comm-B replies from the direct
  RTL-SDR feed are parsed for BDS 4,0 / 5,0 / 6,0 — bank angle, turn rate,
  IAS, Mach, magnetic heading, selected altitude and QNH — cross-checked
  against ADS-B-tracked aircraft. The detail panel gains "Bank" and "Turn
  rate" rows, and the existing IAS/Mach/heading fields now populate for
  RTL-SDR and local-receiver users, not just the community feeds.

- VOR navigation (rail → "VOR navigation", RTL-SDR only): tune a nearby VOR and
  decode its radial and Morse ident straight off the dongle, shown on a compass
  dial next to the geometric radial from your primary place (with the
  difference), plus a signal bar and an ident check. The received and geometric
  radials are also drawn on the map from the station. Shares the ADS-B dongle
  (pauses decoding for the session) unless a second one is picked in Settings.
  Experimental — the DSP is validated only against synthetic signals so far,
  and it needs a VHF antenna.
- VOR position fix: "Fix my position" auto-picks a few well-spread nearby VORs,
  tunes each in turn to hold a radial, then crosses them into a lat/lon with an
  uncertainty estimate and a bearing/distance from your saved place. The fix,
  its "cocked hat", and the uncertainty ring are drawn on the map.
- Navaid map layer (Layers → "Navaids"): VOR / VOR-DME / VORTAC / TACAN / DME
  and NDB stations from the bundled OurAirports data, drawn with chart-style
  symbols. Click one for its ident (with Morse), frequency, name, station
  declination and DME channel; clicking a VOR also draws a compass rose
  oriented to that station's magnetic variation. Groundwork for RTL-SDR VOR
  decoding.
- Update check: on startup (once every 20 h at most) the app asks GitHub for
  the latest release and shows a dismissible banner if a newer version is
  out, linking straight to the matching download (installer or portable zip).
  Nothing is downloaded or installed automatically. There's a "Check for
  updates" button in the About panel, and the startup check can be turned
  off under Settings → Updates.

## [0.3.2] - 2026-09-05

### Changed

- ATC listen/scan, ACARS messages, and Mode A/C contacts — all RTL-SDR-only
  features — are now hidden until a dongle is actually plugged in, instead
  of appearing as dead controls with nothing to back them.

## [0.3.1] - 2026-09-05

### Added

- Portable mode: dropping a `portable.txt` file next to the executable
  makes it store all persistent data (settings, database, tile cache) in a
  `data` folder beside itself instead of the normal per-user AppData
  location, for a zip that can run from a USB stick with no footprint on
  the host machine. A normal installed copy is unaffected.

## [0.3.0] - 2026-09-05

### Added

- ATC voice audio via RTL-SDR: tune to a VHF airband frequency (from an
  airport's frequency list) or scan across several, with session recording
  to WAV. Shares a dongle with direct ADS-B decoding via a pause/resume
  handoff when only one is available.
- ACARS message decoding via RTL-SDR — reuses the ATC voice AM demod path,
  adding MSK bit recovery and ARINC 618 message framing.
- Direct UAT (978MHz) reception via RTL-SDR — a second ADS-B band used by
  US GA aircraft below 18,000ft, merged into the aircraft list like the
  existing 1090ES decoder. Aircraft-transmitted messages only.
- Legacy ATCRBS Mode A/C detection via RTL-SDR: a "Mode A/C contacts" panel
  listing nearby unidentified transponder replies. These carry no ICAO
  address or position, so they're a plain list (possible squawk, possible
  altitude, reply count), not map markers.
- PIA / interesting / LADD badges on the aircraft detail panel, alongside
  the existing MIL badge.

### Changed

- The aircraft info-chip (callsign + altitude map label) now renders via a
  single deck.gl overlay instead of several independent MapLibre symbol
  layers, fixing intermittent orphaned-badge/collision glitches.

### Fixed

- Direct RTL-SDR decode could misread an in-progress ACAS/TCAS resolution
  advisory broadcast as a false emergency squawk.
- Emergency-squawk alerts could repeat indefinitely for one standing
  emergency, traced to three independent causes: the NA-wide squawk watch's
  dedup didn't survive an app restart, the in-viewport check's dedup reset
  on ordinary viewport churn, and per-aircraft transition detection reset
  whenever a reception gap briefly dropped the aircraft from tracking. All
  three now share one persisted, restart- and gap-resistant record.

## [0.2.0] - 2026-09-05

### Added

- Direct RTL-SDR dongle support: a from-spec, GPL-free Mode S/ADS-B
  demodulator and decoder, so aircraft can be received straight off a local
  USB dongle instead of only through community feeds.
- Multi-source merge: online feeds, a local dump1090/readsb receiver, and the
  RTL-SDR now combine by aircraft instead of failing over, and locally-owned
  sources always take priority over community data for the same aircraft.
- A master toggle to disable online community feeds entirely and run on
  local sources only.
- RTL-SDR reception coverage polygon: a terrain line-of-sight estimate
  (effective-earth-radius method) around a user-designated receiver
  location, with a selectable target altitude band.
- "Fix USB driver" helper in Settings — downloads and launches Zadig for the
  WinUSB driver association the RTL-SDR needs on Windows.
- A distinguishing badge on the map for aircraft received directly via
  RTL-SDR, versus community-sourced aircraft.
- First-launch disclaimer / end-user agreement, with `DISCLAIMER.md` and an
  About-panel link to reopen it.
- Independent light/dark UI theme setting, separate from the basemap's own
  light/dark tiles.

### Changed

- Settings panel reorganized into collapsible sections (Data sources,
  Appearance, Aircraft photos, Alerts & history, Offline maps).
- Overlapping aircraft symbols and labels now stack deterministically by
  altitude instead of flickering between arbitrary collision winners.
- Reworded the ambiguous "nm great-circle" route distance label.

## [0.1.0] - Initial versioned release

Baseline feature set — map view, live aircraft tracking, watchlist/alerts,
aircraft detail panel with photos/routes, airport reference layers, offline
tile caching, and flight history logging. See `git log` for the detailed
history predating formal versioning.

[Unreleased]: https://github.com/ProcioneDeConti/RaccTrack/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/ProcioneDeConti/RaccTrack/releases/tag/v0.3.1
[0.3.0]: https://github.com/ProcioneDeConti/RaccTrack/releases/tag/v0.3.0
[0.2.0]: https://github.com/ProcioneDeConti/RaccTrack/releases/tag/v0.2.0
