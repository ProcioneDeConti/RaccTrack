//! Raw aggregator JSON (readsb / ADSBExchange-v2 schema) and the normalized
//! [`Aircraft`] the rest of the app consumes.

use serde::{Deserialize, Serialize};

/// One aircraft as returned by adsb.lol / adsb.fi `/v2/...` endpoints.
#[derive(Debug, Clone, Deserialize)]
pub struct RawAircraft {
    pub hex: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
    pub flight: Option<String>,
    pub r: Option<String>,
    pub t: Option<String>,
    pub desc: Option<String>,
    pub category: Option<String>,

    #[serde(default)]
    pub alt_baro: Option<AltBaro>,
    pub alt_geom: Option<f64>,
    pub gs: Option<f64>,
    pub ias: Option<f64>,
    pub tas: Option<f64>,
    pub mach: Option<f64>,
    pub track: Option<f64>,
    pub mag_heading: Option<f64>,
    pub true_heading: Option<f64>,
    pub baro_rate: Option<f64>,
    pub geom_rate: Option<f64>,
    pub squawk: Option<String>,
    pub emergency: Option<String>,

    pub nav_altitude_mcp: Option<f64>,
    pub nav_altitude_fms: Option<f64>,
    pub nav_heading: Option<f64>,
    pub nav_qnh: Option<f64>,

    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub rssi: Option<f64>,
    pub messages: Option<f64>,
    pub seen: Option<f64>,
    pub seen_pos: Option<f64>,

    #[serde(default)]
    pub mlat: Vec<String>,
    #[serde(default)]
    pub tisb: Vec<String>,

    #[serde(rename = "dbFlags")]
    pub db_flags: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AltBaro {
    Num(f64),
    Str(String), // "ground"
}

impl AltBaro {
    fn feet(&self) -> Option<f64> {
        match self {
            AltBaro::Num(n) => Some(*n),
            AltBaro::Str(_) => None,
        }
    }
    fn is_ground(&self) -> bool {
        matches!(self, AltBaro::Str(s) if s.eq_ignore_ascii_case("ground"))
    }
}

/// Top-level response envelope.
#[derive(Debug, Deserialize)]
pub struct AircraftResponse {
    #[serde(default, alias = "aircraft")]
    pub ac: Vec<RawAircraft>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PositionSource {
    Adsb,
    Mlat,
    Tisb,
    Other,
}

/// Normalized aircraft. Serialized to the frontend as camelCase.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Aircraft {
    pub hex: String,
    pub flight: Option<String>,
    pub registration: Option<String>,
    pub type_code: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,

    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub alt_baro: Option<f64>,
    pub alt_geom: Option<f64>,
    pub on_ground: bool,

    pub ground_speed: Option<f64>,
    pub ias: Option<f64>,
    pub tas: Option<f64>,
    pub mach: Option<f64>,
    pub track: Option<f64>,
    pub mag_heading: Option<f64>,
    pub true_heading: Option<f64>,
    pub baro_rate: Option<f64>,
    pub geom_rate: Option<f64>,

    pub squawk: Option<String>,
    pub emergency: Option<String>,
    pub nav_altitude: Option<f64>,
    pub nav_heading: Option<f64>,
    pub nav_qnh: Option<f64>,

    pub rssi: Option<f64>,
    pub messages: Option<f64>,
    pub seen: Option<f64>,
    pub seen_pos: Option<f64>,
    pub position_source: PositionSource,
    pub military: bool,
    pub source: String,

    /// epoch millis when this snapshot was observed by us.
    #[serde(skip)]
    pub observed_at: i64,
}

fn clean(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

impl Aircraft {
    pub fn from_raw(raw: RawAircraft, source: &str, now_ms: i64) -> Option<Self> {
        let hex = clean(raw.hex)?.to_lowercase();

        let on_ground = raw
            .alt_baro
            .as_ref()
            .map(|a| a.is_ground())
            .unwrap_or(false);
        let alt_baro = raw.alt_baro.as_ref().and_then(|a| a.feet());

        let position_source = if !raw.mlat.is_empty() {
            PositionSource::Mlat
        } else if raw.r#type.as_deref() == Some("tisb") || !raw.tisb.is_empty() {
            // `tisb` also appears as a hint list on ADS-B targets; the type field
            // is the reliable signal.
            if raw.r#type.as_deref().map(|t| t.contains("tisb")).unwrap_or(false) {
                PositionSource::Tisb
            } else {
                PositionSource::Adsb
            }
        } else if raw
            .r#type
            .as_deref()
            .map(|t| t.starts_with("adsb"))
            .unwrap_or(false)
        {
            PositionSource::Adsb
        } else {
            PositionSource::Other
        };

        let military = raw.db_flags.map(|f| f & 1 == 1).unwrap_or(false);

        Some(Aircraft {
            hex,
            flight: clean(raw.flight),
            registration: clean(raw.r),
            type_code: clean(raw.t),
            description: clean(raw.desc),
            category: clean(raw.category),
            lat: raw.lat,
            lon: raw.lon,
            alt_baro,
            alt_geom: raw.alt_geom,
            on_ground,
            ground_speed: raw.gs,
            ias: raw.ias,
            tas: raw.tas,
            mach: raw.mach,
            track: raw.track,
            mag_heading: raw.mag_heading,
            true_heading: raw.true_heading,
            baro_rate: raw.baro_rate,
            geom_rate: raw.geom_rate,
            squawk: clean(raw.squawk),
            emergency: clean(raw.emergency),
            nav_altitude: raw.nav_altitude_mcp.or(raw.nav_altitude_fms),
            nav_heading: raw.nav_heading,
            nav_qnh: raw.nav_qnh,
            rssi: raw.rssi,
            messages: raw.messages,
            seen: raw.seen,
            seen_pos: raw.seen_pos,
            position_source,
            military,
            source: source.to_string(),
            observed_at: now_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(json: &str) -> RawAircraft {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn parses_ground_aircraft() {
        let a = Aircraft::from_raw(
            raw(r#"{"hex":"AABBCC","alt_baro":"ground","flight":"TEST123 "}"#),
            "adsb.lol",
            0,
        )
        .unwrap();
        assert!(a.on_ground);
        assert_eq!(a.alt_baro, None);
        assert_eq!(a.flight.as_deref(), Some("TEST123"));
        assert_eq!(a.hex, "aabbcc");
    }

    #[test]
    fn parses_numeric_altitude_and_mlat() {
        let a = Aircraft::from_raw(
            raw(r#"{"hex":"abc123","alt_baro":31000,"mlat":["lat","lon"]}"#),
            "adsb.fi",
            5,
        )
        .unwrap();
        assert_eq!(a.alt_baro, Some(31000.0));
        assert!(!a.on_ground);
        assert_eq!(a.position_source, PositionSource::Mlat);
        assert_eq!(a.observed_at, 5);
    }

    #[test]
    fn drops_aircraft_without_hex() {
        assert!(Aircraft::from_raw(raw(r#"{"alt_baro":1000}"#), "x", 0).is_none());
    }

    #[test]
    fn envelope_accepts_ac_and_aircraft_keys() {
        let r: AircraftResponse =
            serde_json::from_str(r#"{"ac":[{"hex":"a"}],"total":1}"#).unwrap();
        assert_eq!(r.ac.len(), 1);
        let r2: AircraftResponse =
            serde_json::from_str(r#"{"aircraft":[{"hex":"a"}]}"#).unwrap();
        assert_eq!(r2.ac.len(), 1);
    }
}
