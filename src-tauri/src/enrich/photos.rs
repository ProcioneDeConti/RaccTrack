//! Aircraft imagery. Tries, in order:
//!   1. planespotters.net photo of the exact airframe (by ICAO hex)
//!   2. planespotters.net photo of the exact airframe (by registration)
//!   3. a representative photo of the same model, via Wikipedia page images
//! Results are cached in SQLite.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::util::now_ms;

const TTL_MS: i64 = 30 * 24 * 3600 * 1000;
const NEGATIVE_TTL_MS: i64 = 3 * 24 * 3600 * 1000;
const MAX_PHOTOS: usize = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    pub thumbnail_url: String,
    pub large_url: Option<String>,
    pub photographer: Option<String>,
    pub link: Option<String>,
    /// "planespotters" | "wikipedia"
    pub source: String,
    /// true when the photo is of this exact airframe.
    pub exact: bool,
}

pub struct PhotoLookup {
    db: Arc<Db>,
    client: reqwest::Client,
}

impl PhotoLookup {
    pub fn new(db: Arc<Db>, client: reqwest::Client) -> Self {
        Self { db, client }
    }

    pub async fn get(
        &self,
        hex: &str,
        registration: Option<&str>,
        model: Option<&str>,
        contact: &str,
    ) -> Result<Vec<Photo>> {
        let hex = hex.to_lowercase();

        let have_contact = !contact.trim().is_empty();
        if let Some((photos, fetched_at)) = self.cached(&hex)? {
            let ttl = if photos.is_empty() { NEGATIVE_TTL_MS } else { TTL_MS };
            // Don't serve a stale non-exact (model) photo if we now have a
            // planespotters contact and could fetch the real airframe.
            let stale_model = have_contact
                && photos.first().map(|p| !p.exact).unwrap_or(false);
            if now_ms() - fetched_at < ttl && !stale_model {
                return Ok(photos);
            }
        }

        let found = match self.resolve(&hex, registration, model, contact).await {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("image lookup for {hex} failed: {e}");
                return Ok(self.cached(&hex)?.map(|(p, _)| p).unwrap_or_default());
            }
        };
        self.store(&hex, &found)?;
        Ok(found)
    }

    async fn resolve(
        &self,
        hex: &str,
        registration: Option<&str>,
        model: Option<&str>,
        contact: &str,
    ) -> Result<Vec<Photo>> {
        // planespotters.net requires a contact URL/email in the User-Agent.
        let contact = contact.trim();
        if !contact.is_empty() {
            let ua = format!(
                "RaccTrack-ADSB/{} (+{contact})",
                env!("CARGO_PKG_VERSION")
            );
            let by_hex = self.planespotters(&format!("hex/{hex}"), &ua).await?;
            if !by_hex.is_empty() {
                return Ok(by_hex);
            }
            if let Some(reg) = registration.map(|r| r.trim()).filter(|r| !r.is_empty()) {
                let by_reg = self.planespotters(&format!("reg/{reg}"), &ua).await?;
                if !by_reg.is_empty() {
                    return Ok(by_reg);
                }
            }
        }
        if let Some(model) = model.map(|m| m.trim()).filter(|m| !m.is_empty()) {
            if let Some(p) = self.wikipedia(model).await? {
                return Ok(vec![p]);
            }
        }
        Ok(Vec::new())
    }

    /// Up to `MAX_PHOTOS` photos of the exact airframe.
    async fn planespotters(&self, path: &str, ua: &str) -> Result<Vec<Photo>> {
        let url = format!("https://api.planespotters.net/pub/photos/{path}");
        let resp = self
            .client
            .get(&url)
            .header(reqwest::header::USER_AGENT, ua)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }
        let body: PsResponse = resp.json().await?;
        let photos = body
            .photos
            .into_iter()
            .filter_map(|p| {
                let thumb = p
                    .thumbnail
                    .or_else(|| p.thumbnail_large.clone())
                    .map(|s| s.src)?;
                Some(Photo {
                    thumbnail_url: thumb,
                    large_url: p.thumbnail_large.map(|s| s.src),
                    photographer: p.photographer,
                    link: p.link,
                    source: "planespotters".into(),
                    exact: true,
                })
            })
            .take(MAX_PHOTOS)
            .collect();
        Ok(photos)
    }

    async fn wikipedia(&self, model: &str) -> Result<Option<Photo>> {
        let url = format!(
            "https://en.wikipedia.org/w/api.php?action=query&format=json&redirects=1\
             &generator=search&gsrsearch={}&gsrlimit=1&gsrnamespace=0\
             &prop=pageimages|info&inprop=url&piprop=thumbnail|original&pithumbsize=1000",
            urlencode(model)
        );
        let resp = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Ok(None);
        }
        let body: WikiResponse = resp.json().await?;
        let Some(page) = body.query.and_then(|q| q.pages.into_values().next()) else {
            return Ok(None);
        };
        let Some(thumb) = page.thumbnail.map(|t| t.source) else {
            return Ok(None);
        };
        Ok(Some(Photo {
            thumbnail_url: strip_query(&thumb),
            large_url: page.original.map(|o| strip_query(&o.source)),
            photographer: page.title.clone(),
            link: page.fullurl,
            source: "wikipedia".into(),
            exact: false,
        }))
    }

    fn cached(&self, hex: &str) -> Result<Option<(Vec<Photo>, i64)>> {
        self.db.with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT json, fetched_at FROM image_cache WHERE hex = ?1")?;
            let mut rows = stmt.query([hex])?;
            if let Some(r) = rows.next()? {
                let json: String = r.get(0)?;
                let fetched_at: i64 = r.get(1)?;
                // Tolerate the pre-gallery cache shape (a bare object / null).
                let photos = serde_json::from_str::<Vec<Photo>>(&json)
                    .or_else(|_| {
                        serde_json::from_str::<Option<Photo>>(&json)
                            .map(|o| o.into_iter().collect())
                    })
                    .unwrap_or_default();
                Ok(Some((photos, fetched_at)))
            } else {
                Ok(None)
            }
        })
    }

    fn store(&self, hex: &str, photos: &[Photo]) -> Result<()> {
        let json = serde_json::to_string(photos)?;
        self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO image_cache(hex, json, fetched_at) VALUES(?1, ?2, ?3)
                 ON CONFLICT(hex) DO UPDATE SET json = excluded.json, fetched_at = excluded.fetched_at",
                rusqlite::params![hex, json, now_ms()],
            )?;
            Ok(())
        })
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Wikimedia thumbnail URLs carry tracking query params; drop them.
fn strip_query(url: &str) -> String {
    url.split('?').next().unwrap_or(url).to_string()
}

#[derive(Deserialize)]
struct PsResponse {
    #[serde(default)]
    photos: Vec<PsPhoto>,
}
#[derive(Deserialize)]
struct PsPhoto {
    thumbnail: Option<PsSrc>,
    thumbnail_large: Option<PsSrc>,
    link: Option<String>,
    photographer: Option<String>,
}
#[derive(Deserialize, Clone)]
struct PsSrc {
    src: String,
}

#[derive(Deserialize)]
struct WikiResponse {
    query: Option<WikiQuery>,
}
#[derive(Deserialize)]
struct WikiQuery {
    #[serde(default)]
    pages: std::collections::HashMap<String, WikiPage>,
}
#[derive(Deserialize)]
struct WikiPage {
    title: Option<String>,
    fullurl: Option<String>,
    thumbnail: Option<WikiImg>,
    original: Option<WikiImg>,
}
#[derive(Deserialize)]
struct WikiImg {
    source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_wikimedia_query() {
        assert_eq!(
            strip_query("https://upload.wikimedia.org/a/b.jpg?utm_source=x"),
            "https://upload.wikimedia.org/a/b.jpg"
        );
    }
}
