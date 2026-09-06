//! Bundled OurAirports navaid data — VORs, VOR-DME/VORTAC, TACAN, DME, NDBs.
//! Public domain (https://ourairports.com/data/). Powers the map's navaid
//! overlay and, later, the RTL-SDR VOR decoder (bearing/ident checks need the
//! station position + commissioned declination).
//!
//! Filtered to the North America box on load — the app is region-locked, and
//! it halves the in-memory set.

use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::region::Area;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Navaid {
    pub ident: String,
    pub name: String,
    /// VOR / VOR-DME / VORTAC / TACAN / DME / NDB / NDB-DME.
    pub kind: String,
    /// Raw published frequency in kHz — VOR-family ~108000–117950, NDB ~190–1750.
    pub freq_khz: f64,
    pub lat: f64,
    pub lon: f64,
    pub elevation_ft: Option<f64>,
    pub country: Option<String>,
    /// Station declination that aligns its radials to magnetic north
    /// (`slaved_variation_deg` for VORs, else `magnetic_variation_deg`).
    /// magnetic_radial = true_bearing_from_station − station_variation_deg.
    pub station_variation_deg: Option<f64>,
    /// FAA usage class as published — HI / LO / BOTH / TERMINAL / RNAV.
    pub usage_type: Option<String>,
    /// Transmitter power class — HIGH / MEDIUM / LOW.
    pub power: Option<String>,
    /// TACAN channel (e.g. "071X") for VORTAC / VOR-DME / TACAN.
    pub dme_channel: Option<String>,
    /// A co-located DME/TACAN is present (slant-range ranging alongside bearing).
    pub has_dme: bool,
    /// DME antenna position when it is *not* co-located with the VOR (usually
    /// it is, and then these are None).
    pub dme_lat: Option<f64>,
    pub dme_lon: Option<f64>,
    pub associated_airport: Option<String>,
    /// Rough published service radius (nm) from the power/usage class — for an
    /// optional range ring. Approximate; not a reception guarantee.
    pub service_range_nm: f64,
}

/// A navaid plus where it sits relative to a query point.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavaidNear {
    #[serde(flatten)]
    pub navaid: Navaid,
    pub distance_nm: f64,
    /// True bearing from the query point to the station.
    pub bearing_deg: f64,
}

pub struct Navaids {
    by_ident: HashMap<String, Navaid>,
    all: Vec<Navaid>,
}

#[derive(Deserialize)]
struct NavRow {
    ident: String,
    name: String,
    #[serde(rename = "type")]
    kind: String,
    frequency_khz: Option<f64>,
    latitude_deg: Option<f64>,
    longitude_deg: Option<f64>,
    elevation_ft: Option<f64>,
    iso_country: Option<String>,
    dme_channel: Option<String>,
    dme_latitude_deg: Option<f64>,
    dme_longitude_deg: Option<f64>,
    slaved_variation_deg: Option<f64>,
    magnetic_variation_deg: Option<f64>,
    #[serde(rename = "usageType")]
    usage_type: Option<String>,
    power: Option<String>,
    associated_airport: Option<String>,
}

fn ne(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.is_empty())
}

/// Approximate FAA standard service volume radius, nm, from the published
/// power/usage class. NDBs vary far more than VOR SSVs — these are only for a
/// rough "expected coverage" ring.
fn service_range_nm(kind: &str, power: Option<&str>, usage: Option<&str>) -> f64 {
    let ndb = kind.starts_with("NDB");
    match (usage, power) {
        (Some("TERMINAL"), _) => if ndb { 15.0 } else { 25.0 },
        (Some("HI"), _) | (_, Some("HIGH")) => if ndb { 75.0 } else { 130.0 },
        (Some("LO"), _) | (_, Some("LOW")) => if ndb { 25.0 } else { 40.0 },
        (_, Some("MEDIUM")) => if ndb { 50.0 } else { 40.0 },
        _ => 40.0,
    }
}

fn kind_rank(kind: &str) -> u8 {
    match kind {
        "VORTAC" => 0,
        "VOR-DME" => 1,
        "VOR" => 2,
        "TACAN" => 3,
        "DME" => 4,
        "NDB-DME" => 5,
        "NDB" => 6,
        _ => 7,
    }
}

impl Navaids {
    pub fn load(csv: &[u8]) -> Result<Self> {
        let mut all = Vec::new();
        let mut by_ident = HashMap::new();

        let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(csv);
        for row in rdr.deserialize::<NavRow>() {
            let Ok(r) = row else { continue };
            let (Some(lat), Some(lon), Some(freq)) =
                (r.latitude_deg, r.longitude_deg, r.frequency_khz)
            else {
                continue;
            };
            if !Area::NORTH_AMERICA.contains(lat, lon) {
                continue;
            }
            let dme_channel = ne(r.dme_channel);
            let has_dme = matches!(r.kind.as_str(), "VORTAC" | "VOR-DME" | "TACAN" | "NDB-DME")
                || dme_channel.is_some();
            // OurAirports leaves dme_lat/lon blank when co-located with the VOR.
            let (dme_lat, dme_lon) = match (r.dme_latitude_deg, r.dme_longitude_deg) {
                (Some(dl), Some(dn)) if (dl - lat).abs() > 1e-4 || (dn - lon).abs() > 1e-4 => {
                    (Some(dl), Some(dn))
                }
                _ => (None, None),
            };
            let power = ne(r.power);
            let usage_type = ne(r.usage_type);
            let nav = Navaid {
                ident: r.ident.trim().to_string(),
                name: r.name.trim().to_string(),
                service_range_nm: service_range_nm(
                    &r.kind,
                    power.as_deref(),
                    usage_type.as_deref(),
                ),
                kind: r.kind,
                freq_khz: freq,
                lat,
                lon,
                elevation_ft: r.elevation_ft,
                country: ne(r.iso_country),
                // A VOR carries its declination in slaved_variation_deg; NDBs
                // (and a few VORs missing it) only have magnetic_variation_deg.
                station_variation_deg: r.slaved_variation_deg.or(r.magnetic_variation_deg),
                usage_type,
                power,
                dme_channel,
                has_dme,
                dme_lat,
                dme_lon,
                associated_airport: ne(r.associated_airport),
            };
            if !nav.ident.is_empty() {
                by_ident
                    .entry(nav.ident.to_uppercase())
                    .or_insert_with(|| nav.clone());
            }
            all.push(nav);
        }

        Ok(Self { by_ident, all })
    }

    pub fn empty() -> Self {
        Self {
            by_ident: HashMap::new(),
            all: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.all.len()
    }

    /// First navaid with this ident (idents aren't globally unique, but within
    /// the NA box collisions are rare and a Phase-2 tune always targets one the
    /// user can actually hear).
    pub fn get(&self, ident: &str) -> Option<&Navaid> {
        self.by_ident.get(&ident.trim().to_uppercase())
    }

    /// Navaids inside a bbox, most-significant kind first, capped.
    pub fn list_in(&self, w: f64, s: f64, e: f64, n: f64, limit: usize) -> Vec<Navaid> {
        let mut out: Vec<&Navaid> = self
            .all
            .iter()
            .filter(|a| a.lon >= w && a.lon <= e && a.lat >= s && a.lat <= n)
            .collect();
        out.sort_by_key(|a| kind_rank(&a.kind));
        out.into_iter().take(limit).cloned().collect()
    }

    /// Up to 4 nearby VOR-family stations picked for a decent position fix:
    /// nearest first, but each new pick must sit ≥ 25° off — in bearing from
    /// the point — every station already chosen, so the radials cross at
    /// usable angles rather than nearly parallel.
    pub fn fix_candidates(&self, lat: f64, lon: f64) -> Vec<Navaid> {
        let mut chosen: Vec<NavaidNear> = Vec::new();
        for n in self.nearest(lat, lon, 250.0, true) {
            if n.distance_nm < 3.0 {
                continue; // sitting on top of it — no usable line of position
            }
            let spread_ok = chosen.iter().all(|c| {
                let d = (n.bearing_deg - c.bearing_deg).rem_euclid(360.0);
                d.min(360.0 - d) >= 25.0
            });
            if spread_ok {
                chosen.push(n);
                if chosen.len() >= 4 {
                    break;
                }
            }
        }
        chosen.into_iter().map(|n| n.navaid).collect()
    }

    /// Navaids within `max_nm` of a point, nearest first. `vor_only` keeps just
    /// the bearing-capable VOR family (what the Phase-3 position fix needs).
    pub fn nearest(&self, lat: f64, lon: f64, max_nm: f64, vor_only: bool) -> Vec<NavaidNear> {
        let mut out: Vec<NavaidNear> = self
            .all
            .iter()
            .filter(|a| !vor_only || matches!(a.kind.as_str(), "VOR" | "VOR-DME" | "VORTAC"))
            .filter_map(|a| {
                let d = haversine_nm(lat, lon, a.lat, a.lon);
                (d <= max_nm).then(|| NavaidNear {
                    navaid: a.clone(),
                    distance_nm: d,
                    bearing_deg: bearing_deg(lat, lon, a.lat, a.lon),
                })
            })
            .collect();
        out.sort_by(|a, b| a.distance_nm.total_cmp(&b.distance_nm));
        out
    }
}

fn haversine_nm(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R_NM: f64 = 3440.065;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * R_NM * a.sqrt().asin()
}

/// Initial true bearing from point 1 to point 2, degrees 0–360.
fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dl = (lon2 - lon1).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // id,filename,ident,name,type,frequency_khz,latitude_deg,longitude_deg,elevation_ft,iso_country,dme_frequency_khz,dme_channel,dme_latitude_deg,dme_longitude_deg,dme_elevation_ft,slaved_variation_deg,magnetic_variation_deg,usageType,power,associated_airport
    const CSV: &str = "id,filename,ident,name,type,frequency_khz,latitude_deg,longitude_deg,elevation_ft,iso_country,dme_frequency_khz,dme_channel,dme_latitude_deg,dme_longitude_deg,dme_elevation_ft,slaved_variation_deg,magnetic_variation_deg,usageType,power,associated_airport\n\
85211,Abilene_VORTAC_US,ABI,Abilene,VORTAC,113700,32.4813,-99.8635,1810,US,113700,084X,,,,10.012,6.246,BOTH,HIGH,KDYS\n\
85051,Sable_Island_NDB_CA,1B,Sable Island,NDB,277,43.9306,-60.0229,,CA,,,,,,,-19.1,,MEDIUM,\n\
99999,Faraway_VOR_NZ,FAR,Faraway,VOR,113000,-40.0,174.0,100,NZ,,,,,,1.0,1.0,BOTH,HIGH,\n";

    #[test]
    fn loads_and_filters_to_na() {
        let n = Navaids::load(CSV.as_bytes()).unwrap();
        assert_eq!(n.len(), 2); // NZ row dropped
        let abi = n.get("abi").unwrap();
        assert_eq!(abi.kind, "VORTAC");
        assert!(abi.has_dme);
        assert_eq!(abi.dme_channel.as_deref(), Some("084X"));
        assert_eq!(abi.station_variation_deg, Some(10.012));
        assert_eq!(abi.service_range_nm, 130.0);
        let ndb = n.get("1B").unwrap();
        assert!(!ndb.has_dme);
        assert_eq!(ndb.station_variation_deg, Some(-19.1)); // fell back to magnetic_variation
    }

    #[test]
    fn list_in_ranks_vortac_before_ndb() {
        let n = Navaids::load(CSV.as_bytes()).unwrap();
        let list = n.list_in(-180.0, 0.0, -20.0, 80.0, 10);
        assert_eq!(list[0].ident, "ABI");
    }

    // Four VORs around (40, -100): N, E, S, plus a second one close to N.
    const SPREAD_CSV: &str = "id,filename,ident,name,type,frequency_khz,latitude_deg,longitude_deg,elevation_ft,iso_country,dme_frequency_khz,dme_channel,dme_latitude_deg,dme_longitude_deg,dme_elevation_ft,slaved_variation_deg,magnetic_variation_deg,usageType,power,associated_airport\n\
1,N_VOR_US,NOR,North,VOR,112000,41.0,-100.0,0,US,,,,,,0,0,BOTH,HIGH,\n\
2,N2_VOR_US,NO2,North2,VOR,112100,41.05,-99.9,0,US,,,,,,0,0,BOTH,HIGH,\n\
3,E_VOR_US,EAS,East,VOR,112200,40.0,-98.7,0,US,,,,,,0,0,BOTH,HIGH,\n\
4,S_VOR_US,SOU,South,VOR,112300,39.0,-100.0,0,US,,,,,,0,0,BOTH,HIGH,\n";

    #[test]
    fn fix_candidates_spread_by_bearing() {
        let n = Navaids::load(SPREAD_CSV.as_bytes()).unwrap();
        let picks: Vec<String> = n
            .fix_candidates(40.0, -100.0)
            .into_iter()
            .map(|c| c.ident)
            .collect();
        // NOR (nearest) is taken; NO2 is nearly the same bearing so it's
        // skipped; EAS and SOU add angular spread.
        assert!(picks.contains(&"NOR".to_string()));
        assert!(!picks.contains(&"NO2".to_string()));
        assert!(picks.contains(&"EAS".to_string()) && picks.contains(&"SOU".to_string()));
    }

    #[test]
    fn nearest_vor_only_skips_ndb_and_sorts() {
        let n = Navaids::load(CSV.as_bytes()).unwrap();
        let near = n.nearest(32.0, -100.0, 400.0, true);
        assert_eq!(near.len(), 1);
        assert_eq!(near[0].navaid.ident, "ABI");
        assert!(near[0].distance_nm > 0.0 && near[0].distance_nm < 120.0);
    }
}
