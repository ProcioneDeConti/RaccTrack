//! Curated "notable aircraft" preset packs — one-click watches matched by
//! callsign prefix and/or ICAO type designator.

use crate::ingest::model::Aircraft;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub blurb: &'static str,
}

struct Rule {
    id: &'static str,
    label: &'static str,
    blurb: &'static str,
    callsigns: &'static [&'static str],
    types: &'static [&'static str],
}

const RULES: &[Rule] = &[
    Rule {
        id: "gov",
        label: "US Government / VIP",
        blurb: "Air Force One/Two (SAM, VENUS, EXEC1F), Marine One (MARINE), and other special air mission flights.",
        callsigns: &["SAM", "VENUS", "EXEC1F", "AF1", "AF2", "MARINE", "NIGHTHAWK", "TALON"],
        types: &["VC25", "C32", "C32A", "C40", "GLF5", "GLF6"],
    },
    Rule {
        id: "isr",
        label: "ISR / Recon",
        blurb: "Signals & imagery intelligence — RC-135, U-2, RQ-4 Global Hawk, MQ-9, E-3 AWACS, E-8 JSTARS, P-8, EP-3.",
        callsigns: &["HOMER", "JAKE", "SLAM", "YETI", "TOGA", "COBRA"],
        types: &[
            "RC135", "R135", "U2", "RQ4", "MQ9", "E3TF", "E3CF", "E3", "E8", "P8",
            "EP3", "RC12", "C560", "MC12", "DHC8",
        ],
    },
    Rule {
        id: "tanker",
        label: "Aerial refueling",
        blurb: "KC-135, KC-10, KC-46, and MRTT tankers.",
        callsigns: &["PACK", "GOLD", "ESSO", "PETRO", "SHELL", "TOGA", "QID"],
        types: &["K35R", "KC135", "KC10", "KC46", "A310", "A332"],
    },
    Rule {
        id: "fire",
        label: "Aerial firefighting",
        blurb: "Air tankers, lead planes and bird-dogs on wildfire missions.",
        callsigns: &["TANKER", "BOMBER", "BIRDDOG", "LEADPLANE", "LEAD", "GUARD"],
        types: &["AT8T", "AT802", "S2T", "C130", "DC10", "B744", "CL2T", "CL41", "CL5T", "P2"],
    },
    Rule {
        id: "noaa",
        label: "NOAA / Hurricane Hunters",
        blurb: "NOAA research flights and the 53rd WRS 'Hurricane Hunters' (TEAL).",
        callsigns: &["NOAA", "TEAL"],
        types: &["WP3D", "P3", "WC130", "C130", "GLF4", "LJ35"],
    },
    Rule {
        id: "doomsday",
        label: "Airborne command post",
        blurb: "E-4B 'Nightwatch' and E-6B 'Mercury' — nuclear command & control.",
        callsigns: &["ORDER", "GORDO"],
        types: &["E4", "E4B", "E6", "E6B"],
    },
    Rule {
        id: "lifeguard",
        label: "Lifeguard / Medevac",
        blurb: "Flights declared LIFEGUARD or MEDEVAC — priority medical transport.",
        callsigns: &["LIFEGUARD", "MEDEVAC", "MERCY", "ANGEL"],
        types: &[],
    },
];

pub fn presets() -> Vec<Preset> {
    RULES
        .iter()
        .map(|r| Preset {
            id: r.id,
            label: r.label,
            blurb: r.blurb,
        })
        .collect()
}

pub fn matches(id: &str, ac: &Aircraft) -> bool {
    let Some(rule) = RULES.iter().find(|r| r.id == id) else {
        return false;
    };
    let cs = ac.flight.as_deref().unwrap_or("").to_uppercase();
    if !cs.is_empty() && rule.callsigns.iter().any(|p| cs.starts_with(p)) {
        return true;
    }
    let t = ac.type_code.as_deref().unwrap_or("").to_uppercase();
    if !t.is_empty() && rule.types.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
        return true;
    }
    if id == "lifeguard" {
        if let Some(e) = ac.emergency.as_deref() {
            if e.eq_ignore_ascii_case("lifeguard") {
                return true;
            }
        }
    }
    false
}

pub fn label_for(id: &str) -> Option<&'static str> {
    RULES.iter().find(|r| r.id == id).map(|r| r.label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::model::PositionSource;

    fn ac(cs: Option<&str>, t: Option<&str>) -> Aircraft {
        let mut a = Aircraft::from_raw(
            serde_json::from_str(r#"{"hex":"abc123"}"#).unwrap(),
            "t",
            0,
        )
        .unwrap();
        a.flight = cs.map(|s| s.into());
        a.type_code = t.map(|s| s.into());
        a
    }

    #[test]
    fn matches_by_callsign_and_type() {
        assert!(matches("gov", &ac(Some("SAM472"), None)));
        assert!(matches("isr", &ac(None, Some("RC135"))));
        assert!(matches("noaa", &ac(Some("NOAA42"), None)));
        assert!(!matches("gov", &ac(Some("UAL123"), Some("B738"))));
    }
}
