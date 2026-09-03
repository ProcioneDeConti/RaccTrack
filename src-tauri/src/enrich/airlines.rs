//! Callsign -> operator decoding.
//!   * Airlines: bundled OpenFlights `airlines.dat`
//!     (`id,name,alias,iata,icao,callsign,country,active`), keyed by the ICAO
//!     3-letter code — the prefix of an airline callsign like `UAL123`.
//!   * Military / government: a small curated callsign-prefix table, checked
//!     first (these prefixes aren't in the airline DB and are more specific).

use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operator {
    pub name: String,
    pub telephony: Option<String>,
    /// "airline" | "military" | "government"
    pub kind: String,
    pub country: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Airline {
    pub name: String,
    pub telephony: Option<String>,
    pub country: Option<String>,
    active: bool,
}

pub struct Airlines {
    by_icao: HashMap<String, Airline>,
}

/// (callsign prefix, operator name, telephony). Prefix match is case-insensitive
/// and anchored at the start of the callsign.
const MIL_GOV: &[(&str, &str, &str, &str)] = &[
    ("RCH", "US Air Force — Air Mobility Command", "Reach", "military"),
    ("SAM", "US Air Force — Special Air Mission", "SAM", "government"),
    ("SPAR", "US Air Force — Special Air Resource", "Spar", "government"),
    ("EXEC1F", "US executive branch", "Executive One Foxtrot", "government"),
    ("VENUS", "US Air Force VIP (89th AW)", "Venus", "government"),
    ("MARINE", "US Marine Corps VIP (HMX-1)", "Marine", "government"),
    ("NIGHTHAWK", "US Marine Corps VIP (HMX-1)", "Nighthawk", "government"),
    ("PAT", "US Army Priority Air Transport", "PAT", "military"),
    ("NAVY", "United States Navy", "Navy", "military"),
    ("CNV", "US Navy — fleet logistics (CNATRA)", "Convoy", "military"),
    ("VVLO", "US Navy", "Vault", "military"),
    ("EVAC", "US Air Force aeromedical evacuation", "Evac", "military"),
    ("GRIZ", "US Air National Guard", "Grizzly", "military"),
    ("HERKY", "US Air Force C-130", "Herky", "military"),
    ("QID", "US Air Force", "Quid", "military"),
    ("RRR", "Royal Air Force", "Ascot", "military"),
    ("CFC", "Canadian Armed Forces", "Canforce", "military"),
    ("CTM", "French Air and Space Force", "Cotam", "military"),
    ("GAF", "German Air Force", "German Air Force", "military"),
    ("IAM", "Italian Air Force", "Italian Air Force", "military"),
    ("NATO", "NATO", "NATO", "military"),
    ("FNF", "French Navy", "French Navy", "military"),
    ("ASY", "US Customs and Border Protection", "Omaha", "government"),
    ("DOJ", "US Department of Justice", "Justice", "government"),
    ("SPFA", "US Forest Service", "", "government"),
];

fn ne(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() || s == "\\N" || s == "-" || s.eq_ignore_ascii_case("n/a") {
        None
    } else {
        Some(s.to_string())
    }
}

impl Airlines {
    pub fn from_dat(text: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct Row {
            _id: String,
            name: String,
            _alias: String,
            _iata: String,
            icao: String,
            callsign: String,
            country: String,
            active: String,
        }

        let mut by_icao: HashMap<String, Airline> = HashMap::new();
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(text.as_bytes());
        for row in rdr.deserialize::<Row>() {
            let Ok(r) = row else { continue };
            let Some(icao) = ne(&r.icao) else { continue };
            let icao = icao.to_uppercase();
            if icao.len() != 3 || !icao.chars().all(|c| c.is_ascii_alphabetic()) {
                continue;
            }
            let Some(name) = ne(&r.name) else { continue };
            let active = r.active.trim().eq_ignore_ascii_case("Y");
            let entry = Airline {
                name,
                telephony: ne(&r.callsign),
                country: ne(&r.country),
                active,
            };
            match by_icao.get(&icao) {
                // Prefer an active airline, then one that has a telephony name.
                Some(existing)
                    if existing.active && (!active || existing.telephony.is_some()) => {}
                _ => {
                    by_icao.insert(icao, entry);
                }
            }
        }
        Ok(Self { by_icao })
    }

    pub fn empty() -> Self {
        Self {
            by_icao: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.by_icao.len()
    }

    pub fn get(&self, icao3: &str) -> Option<&Airline> {
        self.by_icao.get(&icao3.trim().to_uppercase())
    }

    /// Decode the operator from a live callsign (e.g. `UAL2401`, `RCH271`).
    pub fn operator_for(&self, callsign: &str) -> Option<Operator> {
        let cs = callsign.trim().to_uppercase();
        if cs.len() < 3 {
            return None;
        }

        if let Some((_, name, tel, kind)) = MIL_GOV
            .iter()
            .find(|(prefix, ..)| cs.starts_with(prefix))
        {
            return Some(Operator {
                name: (*name).to_string(),
                telephony: ne(tel),
                kind: (*kind).to_string(),
                country: None,
            });
        }

        // Airline callsigns are a 3-letter ICAO code followed by the flight
        // number. Bail if the 4th char isn't a digit (rules out registrations
        // flown as callsigns, e.g. N-numbers).
        let prefix: String = cs.chars().take(3).collect();
        if !prefix.chars().all(|c| c.is_ascii_alphabetic()) {
            return None;
        }
        if cs.len() > 3 && !cs.chars().nth(3).map(|c| c.is_ascii_digit()).unwrap_or(false) {
            return None;
        }
        let a = self.get(&prefix)?;
        Some(Operator {
            name: a.name.clone(),
            telephony: a.telephony.clone(),
            kind: "airline".to_string(),
            country: a.country.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAT: &str = "\
1,\"Private flight\",\\N,\"-\",\"N/A\",\\N,\\N,\"Y\"
5209,\"United Airlines\",\\N,\"UA\",\"UAL\",\"UNITED\",\"United States\",\"Y\"
2222,\"Old Defunct Air\",\\N,\"\",\"UAL\",\"\",\"United States\",\"N\"
137,\"Air France\",\\N,\"AF\",\"AFR\",\"AIRFRANS\",\"France\",\"Y\"
";

    #[test]
    fn parses_and_prefers_active() {
        let a = Airlines::from_dat(DAT).unwrap();
        let ual = a.get("UAL").unwrap();
        assert_eq!(ual.name, "United Airlines");
        assert_eq!(ual.telephony.as_deref(), Some("UNITED"));

        let op = a.operator_for("UAL2401").unwrap();
        assert_eq!(op.name, "United Airlines");
        assert_eq!(op.telephony.as_deref(), Some("UNITED"));
        assert_eq!(op.kind, "airline");

        assert_eq!(a.operator_for("AFR83").unwrap().name, "Air France");
    }

    #[test]
    fn military_prefix_wins() {
        let a = Airlines::empty();
        let op = a.operator_for("RCH271").unwrap();
        assert_eq!(op.kind, "military");
        assert_eq!(op.telephony.as_deref(), Some("Reach"));
    }

    #[test]
    fn ignores_registration_callsigns() {
        let a = Airlines::from_dat(DAT).unwrap();
        assert!(a.operator_for("N456CD").is_none());
        assert!(a.operator_for("DAL").is_none()); // no such entry here
    }

    #[test]
    fn bundled_asset_loads() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/airlines.dat");
        let text = std::fs::read_to_string(path).expect("read airlines.dat");
        let a = Airlines::from_dat(&text).unwrap();
        assert!(a.len() > 3000, "only {} airlines parsed", a.len());
        assert_eq!(a.operator_for("UAL2401").unwrap().name, "United Airlines");
        assert_eq!(a.get("DLH").unwrap().telephony.as_deref(), Some("LUFTHANSA"));
    }
}
