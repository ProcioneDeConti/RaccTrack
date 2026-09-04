//! Aircraft datalink messages (ACARS / VDL Mode 2 / HFDL / SATCOM) from
//! airframes.io — a free community aggregator, public API, no key, CC-BY-4.0
//! data. Queried by ICAO hex, cached briefly in `kv_cache`.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::db::Db;

const TTL_MS: i64 = 45_000;
const API: &str = "https://api.airframes.io/v1/messages";
const WANT: usize = 40;
const KEEP: usize = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DlMessage {
    pub time: i64, // epoch ms; 0 if unparseable
    pub kind: String, // ACARS / VDL2 / HFDL / SATCOM
    pub label: Option<String>,
    pub label_desc: Option<String>,
    pub text: Option<String>,
    pub freq_mhz: Option<f64>,
    pub station: Option<String>,
    pub route: Option<String>, // "KORD → KDSM"
}

pub struct Datalink {
    db: Arc<Db>,
    client: reqwest::Client,
}

impl Datalink {
    pub fn new(db: Arc<Db>, client: reqwest::Client) -> Self {
        Self { db, client }
    }

    pub async fn for_hex(&self, hex: &str) -> Result<Vec<DlMessage>> {
        let hex = hex.trim().to_lowercase();
        if hex.len() < 6 {
            return Ok(Vec::new());
        }
        let key = format!("dl:{hex}");
        if let Some(json) = self.db.kv_get(&key, TTL_MS)? {
            if let Ok(v) = serde_json::from_str(&json) {
                return Ok(v);
            }
        }
        match self.fetch(&hex).await {
            Ok(list) => {
                let _ = self.db.kv_put(&key, &serde_json::to_string(&list)?);
                Ok(list)
            }
            Err(e) => {
                tracing::debug!("datalink fetch for {hex} failed: {e}");
                Ok(self
                    .db
                    .kv_get_stale(&key)?
                    .and_then(|j| serde_json::from_str(&j).ok())
                    .unwrap_or_default())
            }
        }
    }

    async fn fetch(&self, hex: &str) -> Result<Vec<DlMessage>> {
        // airframes.io only accepts lowercase hex, and its /v1 -> / rewrite
        // occasionally 404s on a cold instance — one retry clears it.
        let url = format!("{API}?icao={hex}&limit={WANT}");
        let mut resp = self.send(&url).await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            resp = self.send(&url).await?;
        }
        if !resp.status().is_success() {
            return Err(anyhow!("airframes HTTP {}", resp.status()));
        }
        let raw: Vec<RawMsg> = resp.json().await?;

        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut out: Vec<DlMessage> = Vec::new();
        for m in raw {
            let text = clean(m.text);
            let Some(text) = text else { continue }; // nothing human-readable
            let label = clean(m.label);
            if !seen.insert((label.clone().unwrap_or_default(), text.clone())) {
                continue; // same message heard by several stations
            }
            let route = match (m.departing_airport.as_deref(), m.destination_airport.as_deref()) {
                (Some(a), Some(b)) => Some(format!("{a} → {b}")),
                (Some(a), None) => Some(format!("from {a}")),
                (None, Some(b)) => Some(format!("to {b}")),
                _ => None,
            };
            out.push(DlMessage {
                time: m.timestamp.as_deref().and_then(parse_ts).unwrap_or(0),
                kind: kind_of(m.source_type.as_deref().unwrap_or_default()),
                label_desc: label
                    .as_deref()
                    .and_then(label_desc)
                    .map(str::to_string),
                label,
                text: Some(text),
                freq_mhz: m.frequency,
                station: m
                    .station
                    .and_then(|s| s.ident.or(s.nearest_airport_icao)),
                route,
            });
        }
        out.sort_by(|a, b| b.time.cmp(&a.time));
        out.truncate(KEEP);
        Ok(out)
    }

    async fn send(&self, url: &str) -> Result<reqwest::Response> {
        Ok(self
            .client
            .get(url)
            .timeout(Duration::from_secs(12))
            .send()
            .await?)
    }
}

fn clean(s: Option<String>) -> Option<String> {
    let s = s?;
    let s = s.trim().trim_end_matches(['\r', '\n', ';']).trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn parse_ts(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.timestamp_millis())
}

fn kind_of(source_type: &str) -> String {
    match source_type.to_lowercase().as_str() {
        "acars" => "ACARS".into(),
        "vdl" | "vdl2" | "vdlm2" => "VDL2".into(),
        "hfdl" => "HFDL".into(),
        "satcom" | "aero-acars" | "aero-adsc" | "iridium-acars" => "SATCOM".into(),
        "" => "—".into(),
        other => other.to_uppercase(),
    }
}

/// Friendly gloss for the common ACARS message labels.
fn label_desc(label: &str) -> Option<&'static str> {
    Some(match label {
        "H1" => "Airline / performance data",
        "SA" => "Link status advisory",
        "5U" | "5Z" | "80" | "81" | "82" | "83" | "8D" => "Airline-defined",
        "10" | "11" | "12" | "13" | "14" | "15" | "16" | "17" => "Air-to-ground data",
        "20" | "21" | "22" | "23" | "24" | "25" | "26" | "27" => "Ground-to-air data",
        "30" | "31" => "Message part",
        "40" | "41" | "42" | "43" => "Voice-circuit request",
        "44" | "45" | "46" => "TWIP / weather",
        "4M" | "4N" => "Flight-plan data",
        "83M" | "7A" | "7B" | "7C" => "Aircraft-terminated",
        "A0" | "A1" | "A2" | "A3" | "A4" | "A5" | "A6" | "A7" | "A8" | "A9" | "AA" | "AB" => {
            "ATS / CPDLC"
        }
        "B1" | "B2" | "B3" | "B4" | "B5" | "B6" | "B7" | "B8" | "B9" | "BA" => "ATC communication",
        "C1" | "CA" | "CC" => "Printer uplink",
        "Q0" => "Link test",
        "QA" | "QB" | "QC" | "QD" | "QE" | "QF" | "QG" | "QH" | "QK" | "QL" | "QM" | "QN" | "QP"
        | "QQ" | "QR" | "QS" | "QT" | "QU" | "QX" => "OOOI / position report",
        "RA" | "RB" => "Request / response",
        "_d" | "_j" => "Link management",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_airframes_json_into_messages() {
        let json = r#"[
          {"timestamp":"2026-09-04T00:47:18.161Z","sourceType":"vdl","label":"H1",
           "text":"POSN34002W117139,WADUP,210501\r\n","frequency":136.775,
           "departingAirport":"KORD","destinationAirport":"KDSM",
           "station":{"ident":"DG-KGAI-VDL2","nearestAirportIcao":"KGAI"}},
          {"timestamp":"2026-09-04T00:47:18.100Z","sourceType":"vdl","label":"H1",
           "text":"POSN34002W117139,WADUP,210501","frequency":136.975,
           "station":{"ident":"OTHER"}},
          {"timestamp":"2026-09-04T00:46:00Z","sourceType":"acars","label":"_d",
           "text":"   ","station":{"ident":"X"}}
        ]"#;
        let raw: Vec<RawMsg> = serde_json::from_str(json).unwrap();
        // reuse the dedup/clean path by hand
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for m in raw {
            let Some(text) = clean(m.text) else { continue };
            let label = clean(m.label);
            if !seen.insert((label.clone().unwrap_or_default(), text.clone())) {
                continue;
            }
            out.push((kind_of(m.source_type.as_deref().unwrap_or("")), label, text));
        }
        assert_eq!(out.len(), 1); // dup dropped, blank-text dropped
        assert_eq!(out[0].0, "VDL2");
        assert_eq!(out[0].1.as_deref(), Some("H1"));
        assert!(out[0].2.ends_with("210501")); // trailing CRLF trimmed
    }

    #[test]
    fn label_and_kind_maps() {
        assert_eq!(kind_of("hfdl"), "HFDL");
        assert_eq!(kind_of("iridium-acars"), "SATCOM");
        assert_eq!(kind_of("weird"), "WEIRD");
        assert_eq!(label_desc("QT"), Some("OOOI / position report"));
        assert_eq!(label_desc("ZZ"), None);
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMsg {
    timestamp: Option<String>,
    source_type: Option<String>,
    label: Option<String>,
    text: Option<String>,
    frequency: Option<f64>,
    departing_airport: Option<String>,
    destination_airport: Option<String>,
    station: Option<RawStation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStation {
    ident: Option<String>,
    nearest_airport_icao: Option<String>,
}
