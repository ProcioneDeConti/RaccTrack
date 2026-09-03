//! Bundled Mictronics aircraft database (via wiedehopf/tar1090-db).
//! Format: `ICAO;Registration;TypeCode;Flags;Description;Year;OwnerOperator;`
//!
//! `Flags` is a positional string of '0'/'1' characters: character `j` sets
//! bit `j` (this matches readsb's parser). Resulting bits:
//! 1 = military, 2 = interesting, 4 = PIA, 8 = LADD.

use std::collections::HashMap;
use std::io::Read;

use anyhow::{Context, Result};
use flate2::read::GzDecoder;

#[derive(Debug, Clone, Default)]
pub struct AircraftMeta {
    pub registration: Option<String>,
    pub type_code: Option<String>,
    pub description: Option<String>,
    pub owner: Option<String>,
    pub year: Option<String>,
    pub military: bool,
    pub interesting: bool,
    pub pia: bool,
    pub ladd: bool,
}

pub struct AircraftDb {
    by_hex: HashMap<String, AircraftMeta>,
}

impl AircraftDb {
    /// Parse a gzip-compressed semicolon CSV (the bundled `aircraft.csv.gz`).
    pub fn from_gz_bytes(bytes: &[u8]) -> Result<Self> {
        let mut gz = GzDecoder::new(bytes);
        let mut text = String::new();
        gz.read_to_string(&mut text)
            .context("decompressing aircraft.csv.gz")?;
        Ok(Self::from_csv(&text))
    }

    pub fn from_csv(text: &str) -> Self {
        let mut by_hex = HashMap::new();
        for line in text.lines() {
            let f: Vec<&str> = line.split(';').collect();
            if f.len() < 3 {
                continue;
            }
            let hex = f[0].trim().to_lowercase();
            if hex.is_empty() || hex.len() > 6 {
                continue;
            }
            let flags = f.get(3).map(|s| parse_flags(s.trim())).unwrap_or(0);
            let meta = AircraftMeta {
                registration: non_empty(f.get(1)),
                type_code: non_empty(f.get(2)),
                description: non_empty(f.get(4)),
                year: non_empty(f.get(5)),
                owner: non_empty(f.get(6)).map(strip_miscode),
                military: flags & 1 != 0,
                interesting: flags & 2 != 0,
                pia: flags & 4 != 0,
                ladd: flags & 8 != 0,
            };
            by_hex.insert(hex, meta);
        }
        Self { by_hex }
    }

    pub fn empty() -> Self {
        Self {
            by_hex: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.by_hex.len()
    }

    pub fn get(&self, hex: &str) -> Option<&AircraftMeta> {
        self.by_hex.get(&hex.to_lowercase())
    }
}

/// Positional flag string: character `j` (from the left) sets bit `j`.
fn parse_flags(token: &str) -> u32 {
    let mut flags = 0u32;
    for (j, ch) in token.chars().take(32).enumerate() {
        if ch == '1' {
            flags |= 1 << j;
        }
    }
    flags
}

fn non_empty(s: Option<&&str>) -> Option<String> {
    s.map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

/// Owner fields sometimes read "REAL NAME (Miscode - COUNTRY)".
fn strip_miscode(s: String) -> String {
    if let Some(idx) = s.find(" (Miscode") {
        s[..idx].trim().to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rows() {
        let csv = "A835AF;N628TS;GLF6;0001;GULFSTREAM G650;2015;FALCON LANDING LLC;\n\
                   AE0000;63-8146;T38;10;NORTHROP T-38 Talon;;;\n\
                   007CC1;N26BD;ASTR;00;;1992;ARKANSAS BOLT CO (Miscode - UNITED STATES);";
        let db = AircraftDb::from_csv(csv);

        let g = db.get("a835af").unwrap();
        assert_eq!(g.registration.as_deref(), Some("N628TS"));
        assert_eq!(g.type_code.as_deref(), Some("GLF6"));
        assert!(!g.military); // "0001" -> bit 3 (LADD), not military

        let mil = db.get("ae0000").unwrap();
        assert!(mil.military); // "10" -> bit 0 (military)

        let b = db.get("007CC1").unwrap();
        assert_eq!(b.owner.as_deref(), Some("ARKANSAS BOLT CO"));
        assert!(!b.military);
    }
}
