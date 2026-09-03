//! Bundled ICAO aircraft type-designator table (Mictronics `types.json`).
//! Format: `{"B738":{"desc":"L2J","wtc":"M"}, ...}` where `desc` is the ICAO
//! DOC 8643 code — class char, engine count digit, engine-type char — and
//! `wtc` is the wake-turbulence category.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcType {
    pub designator: String,
    /// "Landplane" / "Seaplane" / "Amphibian" / "Helicopter" / "Tiltrotor" / "Gyrocopter"
    pub class: Option<String>,
    pub engines: Option<u8>,
    /// "Jet" / "Turboprop" / "Piston" / "Electric" / "Rocket"
    pub eng_type: Option<String>,
    /// "Light" / "Medium" / "Heavy" / "Super"
    pub wtc: Option<String>,
}

#[derive(Deserialize)]
struct Raw {
    desc: Option<String>,
    wtc: Option<String>,
}

pub struct AcTypes {
    by_code: HashMap<String, AcType>,
}

impl AcTypes {
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        let raw: HashMap<String, Raw> =
            serde_json::from_slice(bytes).context("parsing actypes.json")?;
        let mut by_code = HashMap::with_capacity(raw.len());
        for (code, r) in raw {
            let code = code.trim().to_uppercase();
            if code.is_empty() {
                continue;
            }
            let (class, engines, eng_type) = parse_desc(r.desc.as_deref());
            let wtc = parse_wtc(&code, r.wtc.as_deref());
            by_code.insert(
                code.clone(),
                AcType {
                    designator: code,
                    class,
                    engines,
                    eng_type,
                    wtc,
                },
            );
        }
        Ok(Self { by_code })
    }

    pub fn empty() -> Self {
        Self {
            by_code: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.by_code.len()
    }

    pub fn get(&self, code: &str) -> Option<&AcType> {
        self.by_code.get(&code.trim().to_uppercase())
    }
}

fn parse_desc(desc: Option<&str>) -> (Option<String>, Option<u8>, Option<String>) {
    let Some(d) = desc.map(str::trim).filter(|s| !s.is_empty()) else {
        return (None, None, None);
    };
    let chars: Vec<char> = d.chars().collect();
    let class = chars.first().and_then(|c| {
        Some(
            match c.to_ascii_uppercase() {
                'L' => "Landplane",
                'S' => "Seaplane",
                'A' => "Amphibian",
                'H' => "Helicopter",
                'T' => "Tiltrotor",
                'G' => "Gyrocopter",
                _ => return None,
            }
            .to_string(),
        )
    });
    let engines = chars
        .get(1)
        .and_then(|c| c.to_digit(10))
        .filter(|n| *n > 0)
        .map(|n| n as u8);
    let eng_type = chars.get(2).and_then(|c| {
        Some(
            match c.to_ascii_uppercase() {
                'J' => "Jet",
                'T' => "Turboprop",
                'P' => "Piston",
                'E' => "Electric",
                'R' => "Rocket",
                _ => return None,
            }
            .to_string(),
        )
    });
    (class, engines, eng_type)
}

fn parse_wtc(code: &str, wtc: Option<&str>) -> Option<String> {
    // The A380 and An-225 are ICAO "Super"; the source table only carries L/M/H.
    if matches!(code, "A388" | "A225") {
        return Some("Super".to_string());
    }
    match wtc.map(str::trim).unwrap_or("") {
        "L" => Some("Light".to_string()),
        "M" => Some("Medium".to_string()),
        "H" => Some("Heavy".to_string()),
        "J" | "S" => Some("Super".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_types() {
        let json = br#"{"B738":{"desc":"L2J","wtc":"M"},"C172":{"desc":"L1P","wtc":"L"},
            "H60":{"desc":"H2T","wtc":"M"},"A388":{"desc":"L4J","wtc":"H"},
            "TWR":{"desc":"V0-","wtc":"-"}}"#;
        let t = AcTypes::from_json(json).unwrap();

        let b = t.get("b738").unwrap();
        assert_eq!(b.class.as_deref(), Some("Landplane"));
        assert_eq!(b.engines, Some(2));
        assert_eq!(b.eng_type.as_deref(), Some("Jet"));
        assert_eq!(b.wtc.as_deref(), Some("Medium"));

        let c = t.get("C172").unwrap();
        assert_eq!(c.engines, Some(1));
        assert_eq!(c.eng_type.as_deref(), Some("Piston"));

        assert_eq!(t.get("H60").unwrap().class.as_deref(), Some("Helicopter"));
        assert_eq!(t.get("A388").unwrap().wtc.as_deref(), Some("Super"));

        let tower = t.get("TWR").unwrap();
        assert_eq!(tower.class, None);
        assert_eq!(tower.engines, None);
        assert_eq!(tower.wtc, None);
    }

    #[test]
    fn bundled_asset_loads() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/actypes.json");
        let bytes = std::fs::read(path).expect("read actypes.json");
        let t = AcTypes::from_json(&bytes).unwrap();
        assert!(t.len() > 2000, "only {} types parsed", t.len());
        let b = t.get("B77W").unwrap();
        assert_eq!(b.engines, Some(2));
        assert_eq!(b.eng_type.as_deref(), Some("Jet"));
        assert_eq!(b.wtc.as_deref(), Some("Heavy"));
    }
}
