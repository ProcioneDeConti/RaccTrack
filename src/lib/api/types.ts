// Normalized aircraft shape emitted by the Rust backend. Mirrors
// `src-tauri/src/ingest/model.rs::Aircraft` (serde camelCase).

export interface Aircraft {
  hex: string;
  flight: string | null; // trimmed callsign
  registration: string | null;
  typeCode: string | null; // ICAO type designator, e.g. "B738"
  description: string | null; // human model, e.g. "Boeing 737-800"
  category: string | null; // ADS-B emitter category, e.g. "A3"
  lat: number | null;
  lon: number | null;
  altBaro: number | null; // feet, null when on ground or unknown
  altGeom: number | null;
  onGround: boolean;
  groundSpeed: number | null; // knots
  ias: number | null;
  tas: number | null;
  mach: number | null;
  track: number | null; // degrees true
  magHeading: number | null;
  trueHeading: number | null;
  baroRate: number | null; // ft/min
  geomRate: number | null;
  squawk: string | null;
  emergency: string | null; // "none" | "general" | "lifeguard" | ...
  navAltitude: number | null; // selected altitude (MCP/FMS)
  navHeading: number | null;
  navQnh: number | null;
  rssi: number | null;
  messages: number | null;
  seen: number | null; // seconds since last message
  seenPos: number | null; // seconds since last position
  positionSource: PositionSource;
  military: boolean;
  interesting: boolean;
  pia: boolean;
  ladd: boolean;
  source: string; // "adsb.lol" | "adsb.fi" | "local receiver" | "rtl-sdr"
}

export interface Preset {
  id: string;
  label: string;
  blurb: string;
}

export type PositionSource = "adsb" | "mlat" | "tisb" | "other";

export interface AircraftDiff {
  added: Aircraft[];
  updated: Aircraft[];
  removed: string[]; // hex codes
  total: number;
  generatedAt: number; // epoch ms
}

export interface TrailPoint {
  lat: number;
  lon: number;
  altBaro: number | null;
  onGround: boolean;
  t: number; // epoch ms
}

export interface RouteInfo {
  callsign: string;
  originIcao: string | null;
  originName: string | null;
  destinationIcao: string | null;
  destinationName: string | null;
  originLat: number | null;
  originLon: number | null;
  destinationLat: number | null;
  destinationLon: number | null;
  /** epoch seconds — when hexdb's record for this flight number was last updated */
  updatedAt: number | null;
}

export interface PhotoInfo {
  thumbnailUrl: string;
  largeUrl: string | null;
  photographer: string | null;
  link: string | null;
  source: "planespotters" | "wikipedia";
  /** true when the photo is of this exact airframe */
  exact: boolean;
}

export interface Operator {
  name: string;
  telephony: string | null;
  kind: "airline" | "military" | "government";
  country: string | null;
}

export interface AcType {
  designator: string;
  class: string | null; // "Landplane" | "Helicopter" | ...
  engines: number | null;
  engType: string | null; // "Jet" | "Turboprop" | "Piston" | ...
  wtc: string | null; // "Light" | "Medium" | "Heavy" | "Super"
}

export interface AircraftDetail {
  aircraft: Aircraft;
  ownerOperator: string | null;
  operator: Operator | null;
  country: string | null;
  built: string | null;
  typeDetails: AcType | null;
  route: RouteInfo | null;
  photos: PhotoInfo[];
  /** epoch ms first seen airborne this session (null if on the ground / unknown) */
  airborneSince: number | null;
  /** true when airborneSince is a witnessed departure, not just a lower bound */
  sawDeparture: boolean;
}

export interface SourceStatus {
  /** Every source that contributed aircraft this poll — can be more than
   *  one now (e.g. a direct RTL-SDR feed merged with adsb.fi). */
  activeSources: string[];
  healthy: boolean;
  lastError: string | null;
  lastSuccessAt: number | null;
  requestsLastMinute: number;
}

export interface Airport {
  ident: string;
  icao: string | null;
  iata: string | null;
  name: string;
  municipality: string | null;
  region: string | null;
  lat: number;
  lon: number;
  elevationFt: number | null;
  kind: string;
}

export interface Runway {
  name: string;
  lengthFt: number | null;
  widthFt: number | null;
  surface: string | null;
  lighted: boolean;
  closed: boolean;
  leHeading: number | null;
}

export interface Frequency {
  kind: string;
  description: string | null;
  mhz: string;
}

export interface AirportInfo extends Airport {
  runways: Runway[];
  frequencies: Frequency[];
}

export interface Metar {
  icao: string;
  name: string | null;
  lat: number;
  lon: number;
  obsTime: number | null;
  raw: string;
  flightCategory: string | null;
  tempC: number | null;
  dewpointC: number | null;
  windDir: number | string | null;
  windKt: number | null;
  gustKt: number | null;
  visibility: number | string | null;
  altimeterHpa: number | null;
  wxString: string | null;
  clouds: { cover: string | null; baseFt: number | null }[];
}

export interface StationWx {
  metar: Metar | null;
  tafRaw: string | null;
}

// FAA d-TPP terminal procedure charts. Mirrors `src-tauri/src/charts.rs`.

export interface ChartRef {
  name: string; // "ILS OR LOC RWY 06R"
  code: string; // "IAP" | "APD" | "DP" | "STAR" | "MIN" | ...
  group: string; // "Approach Procedures"
  pdfName: string;
  url: string; // full aeronav.faa.gov URL
}

export interface ChartSet {
  cycle: string; // "2609"
  effective: string | null;
  expires: string | null;
  airport: string;
  charts: ChartRef[];
}

// Aircraft datalink messages from airframes.io. Mirrors `src-tauri/src/datalink.rs`.

export interface DlMessage {
  time: number; // epoch ms, 0 if unknown
  kind: string; // ACARS | VDL2 | HFDL | SATCOM
  label: string | null;
  labelDesc: string | null;
  text: string | null;
  freqMhz: number | null;
  station: string | null;
  route: string | null; // "KORD → KDSM"
}

export interface MapLayers {
  airports: boolean;
  weather: boolean;
  radar: boolean;
  airspace: boolean;
  rangeRings: boolean;
  aircraft: boolean;
}

// Self-collected flight history. Mirrors `src-tauri/src/state.rs`.

export type EventKind =
  | "squawk"
  | "emergency"
  | "emergency_clear"
  | "callsign"
  | "takeoff"
  | "landing"
  | "alert";

export interface AircraftEvent {
  hex: string;
  at: number; // epoch ms
  kind: EventKind;
  flight: string | null;
  from: string | null;
  to: string | null;
  lat: number | null;
  lon: number | null;
}

export interface Sighting {
  hex: string;
  firstSeen: number;
  lastSeen: number;
  count: number;
  flight: string | null;
  registration: string | null;
  typeCode: string | null;
  description: string | null;
  military: boolean;
  note: string | null;
  /** Whether the first-ever sighting of this airframe came straight off the
   *  user's own RTL-SDR dongle rather than a community/local feed. */
  firstSeenDirect: boolean;
}

export type WatchKind =
  | "hex"
  | "registration"
  | "type"
  | "callsign"
  | "preset";

export interface WatchEntry {
  id: number;
  kind: WatchKind;
  value: string;
  label: string | null;
  enabled: boolean;
}

export interface AlertEvent {
  hex: string;
  reason: string; // human text
  watchId: number | null;
  emergency: boolean;
  at: number;
}

export interface HomeLocation {
  label: string;
  lat: number;
  lon: number;
  /** [west, south, east, north] for area places (states, cities…). */
  bbox: [number, number, number, number] | null;
}

export interface GeoResult {
  label: string;
  lat: number;
  lon: number;
  bbox: [number, number, number, number] | null;
  kind: string;
}

export interface PlaceAlert {
  enabled: boolean;
  radiusNm: number;
  ceilingFt: number | null;
  notableOnly: boolean;
  /** User-drawn polygon geofence, `[lat, lon]` vertices (open ring). When
   *  set with >= 3 points this replaces the circular radius entirely. */
  shape: [number, number][] | null;
}

export interface Place {
  id: string;
  label: string;
  lat: number;
  lon: number;
  kind: string | null;
  bbox: [number, number, number, number] | null;
  primary: boolean;
  alert: PlaceAlert;
  /** This place is where the RTL-SDR antenna physically sits — drives the
   *  coverage-polygon calculation, independent of `primary`. */
  rtlsdrLocation: boolean;
}

export interface CoverageBearing {
  bearingDeg: number;
  distanceNm: number;
}

export interface CoverageResult {
  receiverLat: number;
  receiverLon: number;
  receiverGroundElevFt: number;
  targetAltFt: number;
  antennaHeightFt: number;
  points: CoverageBearing[];
}

export interface CoverageProgress {
  running: boolean;
  batchesDone: number;
  batchesTotal: number;
  /** Epoch ms the current compute started. */
  startedAtMs: number;
}

/** Vocabulary shared by every pattern-fillable map area. */
export type FillPattern = "solid" | "stripe" | "hash" | "dot" | "check";

export interface MapColors {
  /** Airspace category (e.g. "CLASS_B", "RESTRICTED") -> hex color override. */
  airspace: Record<string, string>;
  geofenceFill: string | null;
  geofenceLine: string | null;
  geofencePattern: FillPattern | null;
  coverageFill: string | null;
  coverageLine: string | null;
  coveragePattern: FillPattern | null;
}

export interface AppSettings {
  pollIntervalMs: number;
  sourceOrder: string[];
  localReceiverEnabled: boolean;
  localReceiverUrl: string;
  /** Decode ADS-B directly from a USB RTL-SDR dongle — outranks even the
   *  local receiver when on. */
  rtlsdrEnabled: boolean;
  rtlsdrDeviceIndex: number;
  /** Manual gain in tenths of dB (e.g. 297 = 29.7 dB); null = auto gain. */
  rtlsdrGainTenthsDb: number | null;
  /** Which dongle ATC voice listening uses — independent of
   *  rtlsdrDeviceIndex so a second physical dongle can run ADS-B and ATC
   *  audio at once; same index as the ADS-B dongle pauses ADS-B for the
   *  session instead. */
  atcDeviceIndex: number;
  /** Which dongle ACARS decoding uses — same independence rationale as
   *  atcDeviceIndex. */
  acarsDeviceIndex: number;
  /** VHF frequencies (MHz) ACARS listens on/scans across. */
  acarsFreqs: number[];
  /** Master switch for the community aggregators (adsb.lol / adsb.fi). Off
   *  means local sources (RTL-SDR / local receiver) only — no online lookups. */
  onlineSourcesEnabled: boolean;
  /** Show the estimated RTL-SDR reception polygon, computed from terrain
   *  line-of-sight around whichever place has `rtlsdrLocation` set. */
  coverageEnabled: boolean;
  coverageTargetAltFt: number;
  /** Antenna height above *ground* (not sea level) at the receiver, feet. */
  coverageAntennaHeightFt: number;
  basemap: string;
  /** App chrome theme, independent of the basemap's own light/dark tiles.
   *  "auto" follows the basemap (legacy behavior). */
  uiTheme: "auto" | "light" | "dark";
  home: HomeLocation | null;
  places: Place[];
  contact: string;
  layers: MapLayers;
  colors: MapColors;
  rangeRingsNm: number[];
  pinned: string[];
  emergencyWatchEnabled: boolean;
  historyEnabled: boolean;
  historyRetentionDays: number;
  logbookEnabled: boolean;
  tileCacheEnabled: boolean;
  tileCacheMaxMb: number;
  units: "imperial" | "metric";
  notificationsEnabled: boolean;
  showAllTrails: boolean;
  /** First-launch safety/data disclaimer (DISCLAIMER.md) has been dismissed. */
  disclaimerAcknowledged: boolean;
}

export interface LocalReceiverProbe {
  aircraft: number;
  withPosition: number;
}

export interface RtlSdrStatus {
  enabled: boolean;
  deviceOpen: boolean;
  messagesDecoded: number;
  rawCandidates: number;
  framesParsed: number;
  adsbFrames: number;
  aircraftTracked: number;
  lastError: string | null;
}

export interface AtcStatus {
  running: boolean;
  deviceOpen: boolean;
  tunedMhz: number | null;
  /** True while listening to more than one frequency — tunedMhz is whichever one it's parked on. */
  scanning: boolean;
  /** True for the brief gap while a scan hop is closing and reopening the device. */
  retuning: boolean;
  /** True while a transmission is being heard — like a scanner's squelch light. */
  squelchOpen: boolean;
  /** True while this session has paused ADS-B decoding to borrow its dongle. */
  adsbPaused: boolean;
  recording: boolean;
  lastError: string | null;
}

export interface AcarsStatus {
  running: boolean;
  deviceOpen: boolean;
  tunedMhz: number | null;
  scanning: boolean;
  retuning: boolean;
  squelchOpen: boolean;
  adsbPaused: boolean;
  messageCount: number;
  lastError: string | null;
}

export interface AcarsMessage {
  mode: string;
  tail: string;
  techAck: string;
  label: string;
  blockId: string;
  text: string | null;
  /** See `acars/frame.rs` — `false` may mean a field-boundary assumption in
   *  the decoder is wrong rather than the message being garbage. */
  bccOk: boolean;
  parityErrors: number;
  freqMhz: number;
  timestampMs: number;
}
