//! Bundled OurAirports data — airports, runways, frequencies. Public domain
//! (https://ourairports.com/data/). Used for route endpoints and the map's
//! airport overlay / info panel.

use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Airport {
    pub ident: String,
    pub icao: Option<String>,
    pub iata: Option<String>,
    pub name: String,
    pub municipality: Option<String>,
    pub region: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub elevation_ft: Option<f64>,
    /// large_airport / medium_airport / small_airport / heliport / seaplane_base
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Runway {
    pub name: String, // "06L/24R"
    pub length_ft: Option<f64>,
    pub width_ft: Option<f64>,
    pub surface: Option<String>,
    pub lighted: bool,
    pub closed: bool,
    pub le_heading: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frequency {
    pub kind: String, // TWR / GND / ATIS / CTAF / APP / DEP ...
    pub description: Option<String>,
    pub mhz: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AirportInfo {
    #[serde(flatten)]
    pub airport: Airport,
    pub runways: Vec<Runway>,
    pub frequencies: Vec<Frequency>,
}

pub struct Airports {
    by_code: HashMap<String, Airport>,
    /// All airports worth showing on the map (excludes closed).
    all: Vec<Airport>,
    runways: HashMap<String, Vec<Runway>>,
    freqs: HashMap<String, Vec<Frequency>>,
}

// --- CSV row shapes ---

#[derive(Deserialize)]
struct ApRow {
    ident: String,
    #[serde(rename = "type")]
    kind: String,
    name: String,
    latitude_deg: Option<f64>,
    longitude_deg: Option<f64>,
    elevation_ft: Option<f64>,
    iso_region: Option<String>,
    municipality: Option<String>,
    icao_code: Option<String>,
    iata_code: Option<String>,
    gps_code: Option<String>,
}

#[derive(Deserialize)]
struct RwRow {
    airport_ident: String,
    length_ft: Option<f64>,
    width_ft: Option<f64>,
    surface: Option<String>,
    lighted: Option<u8>,
    closed: Option<u8>,
    le_ident: Option<String>,
    he_ident: Option<String>,
    #[serde(rename = "le_heading_degT")]
    le_heading: Option<f64>,
}

#[derive(Deserialize)]
struct FqRow {
    airport_ident: String,
    #[serde(rename = "type")]
    kind: String,
    description: Option<String>,
    frequency_mhz: Option<String>,
}

fn ne(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.is_empty())
}

impl Airports {
    pub fn load(airports_csv: &[u8], runways_csv: &[u8], freqs_csv: &[u8]) -> Result<Self> {
        let mut by_code = HashMap::new();
        let mut all = Vec::new();

        let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(airports_csv);
        for row in rdr.deserialize::<ApRow>() {
            let Ok(r) = row else { continue };
            let (Some(lat), Some(lon)) = (r.latitude_deg, r.longitude_deg) else {
                continue;
            };
            if r.kind == "closed" {
                continue;
            }
            let icao = ne(r.icao_code.clone()).or_else(|| ne(r.gps_code.clone()));
            let ap = Airport {
                ident: r.ident.clone(),
                icao: icao.clone(),
                iata: ne(r.iata_code.clone()),
                name: r.name.clone(),
                municipality: ne(r.municipality.clone()),
                region: ne(r.iso_region.clone()),
                lat,
                lon,
                elevation_ft: r.elevation_ft,
                kind: r.kind.clone(),
            };
            for key in [
                icao.clone().unwrap_or_default().to_uppercase(),
                r.ident.to_uppercase(),
            ] {
                if !key.is_empty() {
                    by_code.entry(key).or_insert_with(|| ap.clone());
                }
            }
            all.push(ap);
        }

        let mut runways: HashMap<String, Vec<Runway>> = HashMap::new();
        let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(runways_csv);
        for row in rdr.deserialize::<RwRow>() {
            let Ok(r) = row else { continue };
            let name = match (ne(r.le_ident.clone()), ne(r.he_ident.clone())) {
                (Some(a), Some(b)) => format!("{a}/{b}"),
                (Some(a), None) => a,
                (None, Some(b)) => b,
                _ => continue,
            };
            runways
                .entry(r.airport_ident.to_uppercase())
                .or_default()
                .push(Runway {
                    name,
                    length_ft: r.length_ft.filter(|v| *v > 0.0),
                    width_ft: r.width_ft.filter(|v| *v > 0.0),
                    surface: ne(r.surface),
                    lighted: r.lighted == Some(1),
                    closed: r.closed == Some(1),
                    le_heading: r.le_heading,
                });
        }

        let mut freqs: HashMap<String, Vec<Frequency>> = HashMap::new();
        let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(freqs_csv);
        for row in rdr.deserialize::<FqRow>() {
            let Ok(r) = row else { continue };
            let Some(mhz) = ne(r.frequency_mhz) else { continue };
            freqs
                .entry(r.airport_ident.to_uppercase())
                .or_default()
                .push(Frequency {
                    kind: r.kind,
                    description: ne(r.description),
                    mhz,
                });
        }

        Ok(Self {
            by_code,
            all,
            runways,
            freqs,
        })
    }

    pub fn empty() -> Self {
        Self {
            by_code: HashMap::new(),
            all: Vec::new(),
            runways: HashMap::new(),
            freqs: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.all.len()
    }

    pub fn get(&self, code: &str) -> Option<&Airport> {
        self.by_code.get(&code.trim().to_uppercase())
    }

    pub fn info(&self, code: &str) -> Option<AirportInfo> {
        let ap = self.get(code)?.clone();
        let ident = ap.ident.to_uppercase();
        Some(AirportInfo {
            runways: self.runways.get(&ident).cloned().unwrap_or_default(),
            frequencies: self.freqs.get(&ident).cloned().unwrap_or_default(),
            airport: ap,
        })
    }

    /// Airports within a bbox, biggest first, capped.
    pub fn list_in(&self, w: f64, s: f64, e: f64, n: f64, limit: usize) -> Vec<Airport> {
        let mut out: Vec<&Airport> = self
            .all
            .iter()
            .filter(|a| a.lon >= w && a.lon <= e && a.lat >= s && a.lat <= n)
            .collect();
        out.sort_by_key(|a| kind_rank(&a.kind));
        out.into_iter().take(limit).cloned().collect()
    }

    /// Fuzzy find by ICAO / IATA / ident / name.
    pub fn find(&self, query: &str) -> Vec<Airport> {
        let q = query.trim().to_uppercase();
        if q.len() < 2 {
            return Vec::new();
        }
        if let Some(exact) = self.by_code.get(&q) {
            return vec![exact.clone()];
        }
        let mut out: Vec<&Airport> = self
            .all
            .iter()
            .filter(|a| {
                a.ident.to_uppercase().starts_with(&q)
                    || a.iata.as_deref().map(|i| i.eq_ignore_ascii_case(&q)).unwrap_or(false)
                    || a.name.to_uppercase().contains(&q)
            })
            .collect();
        out.sort_by_key(|a| kind_rank(&a.kind));
        out.into_iter().take(8).cloned().collect()
    }
}

fn kind_rank(kind: &str) -> u8 {
    match kind {
        "large_airport" => 0,
        "medium_airport" => 1,
        "small_airport" => 2,
        "seaplane_base" => 3,
        "heliport" => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AP: &str = "id,ident,type,name,latitude_deg,longitude_deg,elevation_ft,continent,iso_country,iso_region,municipality,scheduled_service,icao_code,iata_code,gps_code,local_code,home_link,wikipedia_link,keywords\n\
3622,KJFK,large_airport,John F Kennedy International Airport,40.639447,-73.779317,13,NA,US,US-NY,New York,yes,KJFK,JFK,KJFK,JFK,,,\n";
    const RW: &str = "id,airport_ref,airport_ident,length_ft,width_ft,surface,lighted,closed,le_ident,le_latitude_deg,le_longitude_deg,le_elevation_ft,le_heading_degT,le_displaced_threshold_ft,he_ident,he_latitude_deg,he_longitude_deg,he_elevation_ft,he_heading_degT,he_displaced_threshold_ft\n\
1,3622,KJFK,14511,200,ASP,1,0,04L,,,,42,,22R,,,,222,\n";
    const FQ: &str = "id,airport_ref,airport_ident,type,description,frequency_mhz\n\
1,3622,KJFK,TWR,Kennedy Tower,119.1\n";

    #[test]
    fn loads_and_joins() {
        let a = Airports::load(AP.as_bytes(), RW.as_bytes(), FQ.as_bytes()).unwrap();
        let info = a.info("kjfk").unwrap();
        assert_eq!(info.airport.iata.as_deref(), Some("JFK"));
        assert_eq!(info.runways[0].name, "04L/22R");
        assert_eq!(info.frequencies[0].kind, "TWR");
        assert_eq!(a.list_in(-75.0, 40.0, -73.0, 41.0, 10).len(), 1);
        assert_eq!(a.find("JFK")[0].ident, "KJFK");
    }
}
