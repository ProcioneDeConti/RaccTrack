# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and versions follow [Semantic Versioning](https://semver.org/) — while the
app is pre-1.0, a minor bump may still include breaking changes.

## [Unreleased]

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

[Unreleased]: https://github.com/ProcioneDeConti/RaccTrack/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/ProcioneDeConti/RaccTrack/releases/tag/v0.2.0
