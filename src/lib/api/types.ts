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
  source: string; // "adsb.lol" | "adsb.fi" | "local"
}

export interface Preset {
  id: string;
  label: string;
  blurb: string;
}

export interface Geofence {
  id: number;
  label: string;
  lat: number;
  lon: number;
  radiusNm: number;
  maxAltFt: number | null;
  milOnly: boolean;
  enabled: boolean;
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
}

export interface SourceStatus {
  activeSource: string;
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

export interface MapLayers {
  airports: boolean;
  weather: boolean;
  airspace: boolean;
  rangeRings: boolean;
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

export interface AppSettings {
  pollIntervalMs: number;
  sourceOrder: string[];
  basemap: string;
  home: HomeLocation | null;
  contact: string;
  layers: MapLayers;
  rangeRingsNm: number[];
  pinned: string[];
  emergencyWatchEnabled: boolean;
  tileCacheEnabled: boolean;
  tileCacheMaxMb: number;
  units: "imperial" | "metric";
  notificationsEnabled: boolean;
  showAllTrails: boolean;
}
