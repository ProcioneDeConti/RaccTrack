//! Bundled OurAirports data — ICAO/GPS code -> name + coordinates, for
//! resolving route endpoints. Public domain (https://ourairports.com/data/).

use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Airport {
    pub name: String,
    pub municipality: Option<String>,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Deserialize)]
struct Row {
    ident: String,
    name: String,
    latitude_deg: Option<f64>,
    longitude_deg: Option<f64>,
    municipality: Option<String>,
    icao_code: Option<String>,
    gps_code: Option<String>,
}

pub struct Airports {
    by_code: HashMap<String, Airport>,
}

impl Airports {
    pub fn from_csv_bytes(bytes: &[u8]) -> Result<Self> {
        let mut rdr = csv::ReaderBuilder::new()
            .flexible(true)
            .from_reader(bytes);
        let mut by_code = HashMap::new();

        for row in rdr.deserialize::<Row>() {
            let Ok(row) = row else { continue };
            let (Some(lat), Some(lon)) = (row.latitude_deg, row.longitude_deg) else {
                continue;
            };
            let primary = row
                .icao_code
                .clone()
                .filter(|s| !s.is_empty())
                .or_else(|| row.gps_code.clone().filter(|s| !s.is_empty()))
                .unwrap_or_else(|| row.ident.clone());

            let ap = Airport {
                name: row.name.clone(),
                municipality: row.municipality.clone().filter(|s| !s.is_empty()),
                lat,
                lon,
            };

            for key in [primary.to_uppercase(), row.ident.to_uppercase()] {
                by_code.entry(key).or_insert_with(|| ap.clone());
            }
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

    pub fn get(&self, code: &str) -> Option<&Airport> {
        self.by_code.get(&code.to_uppercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_by_icao_and_ident() {
        let csv = "id,ident,type,name,latitude_deg,longitude_deg,elevation_ft,continent,iso_country,iso_region,municipality,scheduled_service,icao_code,iata_code,gps_code,local_code,home_link,wikipedia_link,keywords\n\
3622,KJFK,large_airport,John F Kennedy International Airport,40.639447,-73.779317,13,NA,US,US-NY,New York,yes,KJFK,JFK,KJFK,JFK,,,\n";
        let ap = Airports::from_csv_bytes(csv.as_bytes()).unwrap();
        assert_eq!(ap.get("kjfk").unwrap().name, "John F Kennedy International Airport");
        assert_eq!(ap.get("KJFK").unwrap().municipality.as_deref(), Some("New York"));
    }
}
