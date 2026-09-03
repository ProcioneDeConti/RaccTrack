//! Callsign -> origin/destination lookup via hexdb.io, cached in SQLite.
//! hexdb returns `{"flight":"UAL947","route":"EHAM-KIAD","updatetime":...}`.
//! Multi-leg routes ("A-B-C") are reduced to first and last.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;

use crate::db::Db;
use crate::util::now_ms;

const POSITIVE_TTL_MS: i64 = 7 * 24 * 3600 * 1000;
const NEGATIVE_TTL_MS: i64 = 12 * 3600 * 1000;

#[derive(Debug, Clone, Default)]
pub struct Route {
    pub origin: Option<String>,
    pub dest: Option<String>,
    /// When hexdb's record for this flight number was last updated (epoch secs).
    /// Old ≠ wrong — stable flight numbers keep valid routes for years — but a
    /// very old value plus an off-path aircraft is a strong "stale" signal.
    pub updated_at: Option<i64>,
}

impl Route {
    pub fn is_empty(&self) -> bool {
        self.origin.is_none() && self.dest.is_none()
    }
}

#[derive(Deserialize)]
struct HexdbRoute {
    route: Option<String>,
    updatetime: Option<i64>,
}

pub struct RouteLookup {
    db: Arc<Db>,
    client: reqwest::Client,
}

impl RouteLookup {
    pub fn new(db: Arc<Db>, client: reqwest::Client) -> Self {
        Self { db, client }
    }

    pub async fn get(&self, callsign: &str) -> Result<Route> {
        let cs = callsign.trim().to_uppercase();
        if cs.is_empty() {
            return Ok(Route::default());
        }

        if let Some((route, fetched_at)) = self.cached(&cs)? {
            let ttl = if route.is_empty() {
                NEGATIVE_TTL_MS
            } else {
                POSITIVE_TTL_MS
            };
            if now_ms() - fetched_at < ttl {
                return Ok(route);
            }
        }

        let route = match self.fetch(&cs).await {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!("route lookup for {cs} failed: {e}");
                // Serve stale cache if we have any.
                return Ok(self.cached(&cs)?.map(|(r, _)| r).unwrap_or_default());
            }
        };
        self.store(&cs, &route)?;
        Ok(route)
    }

    async fn fetch(&self, callsign: &str) -> Result<Route> {
        let url = format!("https://hexdb.io/api/v1/route/icao/{callsign}");
        let resp = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Ok(Route::default());
        }
        let body: HexdbRoute = resp
            .json()
            .await
            .unwrap_or(HexdbRoute { route: None, updatetime: None });
        let mut route = parse_route(body.route.as_deref());
        route.updated_at = body.updatetime.filter(|t| *t > 0);
        Ok(route)
    }

    fn cached(&self, callsign: &str) -> Result<Option<(Route, i64)>> {
        self.db.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT origin, dest, updated_at, fetched_at FROM route_cache WHERE callsign = ?1",
            )?;
            let mut rows = stmt.query([callsign])?;
            if let Some(r) = rows.next()? {
                let origin: Option<String> = r.get(0)?;
                let dest: Option<String> = r.get(1)?;
                let updated_at: Option<i64> = r.get(2)?;
                let fetched_at: i64 = r.get(3)?;
                Ok(Some((
                    Route {
                        origin,
                        dest,
                        updated_at,
                    },
                    fetched_at,
                )))
            } else {
                Ok(None)
            }
        })
    }

    fn store(&self, callsign: &str, route: &Route) -> Result<()> {
        self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO route_cache(callsign, origin, dest, updated_at, fetched_at)
                 VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(callsign) DO UPDATE SET
                   origin = excluded.origin, dest = excluded.dest,
                   updated_at = excluded.updated_at, fetched_at = excluded.fetched_at",
                rusqlite::params![
                    callsign,
                    route.origin,
                    route.dest,
                    route.updated_at,
                    now_ms()
                ],
            )?;
            Ok(())
        })
    }
}

fn parse_route(route: Option<&str>) -> Route {
    let Some(route) = route else {
        return Route::default();
    };
    let parts: Vec<&str> = route
        .split('-')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    match parts.as_slice() {
        [] => Route::default(),
        [one] => Route {
            origin: Some(one.to_string()),
            ..Route::default()
        },
        [first, .., last] => Route {
            origin: Some(first.to_string()),
            dest: Some(last.to_string()),
            ..Route::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_and_multileg() {
        let r = parse_route(Some("EHAM-KIAD"));
        assert_eq!(r.origin.as_deref(), Some("EHAM"));
        assert_eq!(r.dest.as_deref(), Some("KIAD"));

        let r = parse_route(Some("KLAX-KDEN-KJFK"));
        assert_eq!(r.origin.as_deref(), Some("KLAX"));
        assert_eq!(r.dest.as_deref(), Some("KJFK"));

        assert!(parse_route(None).is_empty());
        assert!(parse_route(Some("")).is_empty());
    }

    #[test]
    fn reads_updatetime_from_hexdb_json() {
        let body: HexdbRoute =
            serde_json::from_str(r#"{"flight":"ASA289","route":"MMSD-KSMF","updatetime":1532552589}"#)
                .unwrap();
        assert_eq!(body.updatetime, Some(1_532_552_589));

        let body: HexdbRoute = serde_json::from_str(r#"{"route":"A-B"}"#).unwrap();
        assert_eq!(body.updatetime, None);
    }
}
