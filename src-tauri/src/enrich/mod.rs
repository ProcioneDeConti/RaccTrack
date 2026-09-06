//! Aircraft enrichment: bundled identity DB, route lookup, photos, country.

pub mod aircraft_db;
pub mod actypes;
pub mod airlines;
pub mod airports;
pub mod country;
pub mod navaids;
pub mod photos;
pub mod routes;

use std::sync::Arc;

use arc_swap::ArcSwap;
use serde::Serialize;

use crate::ingest::model::Aircraft;
use actypes::{AcType, AcTypes};
use aircraft_db::AircraftDb;
use airlines::{Airlines, Operator};
use airports::Airports;
use photos::{Photo, PhotoLookup};
use routes::RouteLookup;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteInfo {
    pub callsign: String,
    pub origin_icao: Option<String>,
    pub origin_name: Option<String>,
    pub destination_icao: Option<String>,
    pub destination_name: Option<String>,
    pub origin_lat: Option<f64>,
    pub origin_lon: Option<f64>,
    pub destination_lat: Option<f64>,
    pub destination_lon: Option<f64>,
    /// Epoch seconds — when hexdb's record for this flight number was last touched.
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AircraftDetail {
    pub aircraft: Aircraft,
    pub owner_operator: Option<String>,
    /// Operator decoded from the callsign (airline or military/gov).
    pub operator: Option<Operator>,
    pub country: Option<String>,
    pub built: Option<String>,
    /// Engine/wake detail for the ICAO type designator.
    pub type_details: Option<AcType>,
    pub route: Option<RouteInfo>,
    /// Up to ~6 photos; first is the hero image.
    pub photos: Vec<Photo>,
    /// Epoch ms this aircraft was first seen airborne this session (set by the
    /// command from live state, not the enricher).
    pub airborne_since: Option<i64>,
    /// True when `airborne_since` is a witnessed departure, not a lower bound.
    pub saw_departure: bool,
}

pub struct Enricher {
    db: Arc<ArcSwap<AircraftDb>>,
    airports: Arc<ArcSwap<Airports>>,
    airlines: Arc<ArcSwap<Airlines>>,
    actypes: Arc<ArcSwap<AcTypes>>,
    routes: RouteLookup,
    photos: PhotoLookup,
}

impl Enricher {
    pub fn new(
        db: Arc<ArcSwap<AircraftDb>>,
        airports: Arc<ArcSwap<Airports>>,
        airlines: Arc<ArcSwap<Airlines>>,
        actypes: Arc<ArcSwap<AcTypes>>,
        routes: RouteLookup,
        photos: PhotoLookup,
    ) -> Self {
        Self {
            db,
            airports,
            airlines,
            actypes,
            routes,
            photos,
        }
    }

    /// Fill in fields the ADS-B message doesn't carry (owner, model) from the
    /// bundled database.
    pub fn fill_identity(&self, ac: &mut Aircraft) -> Option<String> {
        let db = self.db.load();
        let meta = db.get(&ac.hex)?;
        if ac.registration.is_none() {
            ac.registration = meta.registration.clone();
        }
        if ac.type_code.is_none() {
            ac.type_code = meta.type_code.clone();
        }
        if ac.description.is_none() {
            ac.description = meta.description.clone();
        }
        ac.military |= meta.military;
        ac.interesting |= meta.interesting;
        ac.pia |= meta.pia;
        ac.ladd |= meta.ladd;
        meta.owner.clone()
    }

    pub async fn detail(&self, mut ac: Aircraft, contact: &str) -> AircraftDetail {
        let owner = self.fill_identity(&mut ac);
        let built = self.db.load().get(&ac.hex).and_then(|m| m.year.clone());
        let country = country::country_for_hex(&ac.hex).map(|s| s.to_string());

        let operator = ac
            .flight
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|cs| self.airlines.load().operator_for(cs));

        let type_details = ac
            .type_code
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|t| self.actypes.load().get(t).cloned());

        let route = match ac.flight.clone() {
            Some(cs) if !cs.is_empty() => {
                let r = self.routes.get(&cs).await.unwrap_or_default();
                if r.is_empty() {
                    None
                } else {
                    Some(self.resolve_route(&cs, r))
                }
            }
            _ => None,
        };

        let photos = self
            .photos
            .get(
                &ac.hex,
                ac.registration.as_deref(),
                ac.description.as_deref(),
                contact,
            )
            .await
            .unwrap_or_default();

        AircraftDetail {
            aircraft: ac,
            owner_operator: owner,
            operator,
            country,
            built,
            type_details,
            route,
            photos,
            airborne_since: None,
            saw_departure: false,
        }
    }

    fn resolve_route(&self, callsign: &str, r: routes::Route) -> RouteInfo {
        let airports = self.airports.load();
        let orig = r.origin.as_deref().and_then(|c| airports.get(c)).cloned();
        let dest = r.dest.as_deref().and_then(|c| airports.get(c)).cloned();
        RouteInfo {
            callsign: callsign.to_string(),
            origin_icao: r.origin.clone(),
            origin_name: orig.as_ref().map(pretty),
            destination_icao: r.dest.clone(),
            destination_name: dest.as_ref().map(pretty),
            origin_lat: orig.as_ref().map(|a| a.lat),
            origin_lon: orig.as_ref().map(|a| a.lon),
            destination_lat: dest.as_ref().map(|a| a.lat),
            destination_lon: dest.as_ref().map(|a| a.lon),
            updated_at: r.updated_at,
        }
    }
}

fn pretty(a: &airports::Airport) -> String {
    match &a.municipality {
        Some(city) if !a.name.contains(city.as_str()) => format!("{} ({})", a.name, city),
        _ => a.name.clone(),
    }
}
