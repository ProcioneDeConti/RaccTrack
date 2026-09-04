//! FAA d-TPP terminal procedure charts (approach plates, airport diagrams,
//! SIDs/STARs, minimums). Public, no key. The per-cycle metafile
//! (`d-TPP_Metafile.xml`, ~16 MB) is fetched once, parsed to a per-airport
//! index, and cached in `kv_cache`. Individual PDFs are fetched on demand and
//! cached (LRU-capped) in the `chart_pdf` table.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use arc_swap::ArcSwapOption;
use chrono::{Datelike, Duration as ChronoDuration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::util::now_ms;

const AERONAV: &str = "https://aeronav.faa.gov/d-tpp";
const INDEX_TTL_MS: i64 = 30 * 24 * 3600 * 1000;
const PDF_CACHE_MAX_BYTES: i64 = 300 * 1024 * 1024;

// --- 28-day charting cycle -------------------------------------------------

/// d-TPP / AIRAC 28-day charting-cycle label ("YYNN"). Anchor: cycle 2501 was
/// effective 2025-01-23; cycles run every 28 days. `NN` is the cycle's ordinal
/// within its effective year (usually 1..=13, occasionally 14).
pub fn cycle_for(today: NaiveDate) -> String {
    let anchor = NaiveDate::from_ymd_opt(2025, 1, 23).unwrap();
    let n = (today - anchor).num_days().div_euclid(28);
    let eff = anchor + ChronoDuration::days(n * 28);
    let year = eff.year();
    let jan1 = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let to_jan1 = (jan1 - anchor).num_days();
    // Index (from the anchor) of the first cycle effective on/after Jan 1.
    let first = to_jan1.div_euclid(28) + i64::from(to_jan1.rem_euclid(28) != 0);
    format!("{:02}{:02}", year % 100, n - first + 1)
}

/// The cycle for today plus the neighbours on either side, to probe when the
/// computed cycle 404s around a rollover.
fn candidate_cycles() -> Vec<String> {
    let t = Utc::now().date_naive();
    let mut v = vec![cycle_for(t)];
    for d in [-28i64, 28] {
        let c = cycle_for(t + ChronoDuration::days(d));
        if !v.contains(&c) {
            v.push(c);
        }
    }
    v
}

fn group_for(code: &str) -> &'static str {
    match code.trim().to_ascii_uppercase().as_str() {
        "APD" => "Airport Diagram",
        "IAP" => "Approach Procedures",
        "DP" => "Departure Procedures",
        "DPO" | "ODP" => "Obstacle Departures",
        "STAR" => "Arrival Procedures",
        "MIN" => "Takeoff / Alternate Minimums",
        "LAH" => "Hot Spots / LAHSO",
        "HOT" => "Hot Spots / LAHSO",
        _ => "Other",
    }
}

// --- public shapes -------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartRef {
    pub name: String,     // "ILS Y OR LOC Y RWY 23"
    pub code: String,     // "IAP"
    pub group: String,    // "Approach Procedures"
    pub pdf_name: String, // "01244IYLY23.PDF"
    pub url: String,      // full aeronav.faa.gov URL incl. cycle
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSet {
    pub cycle: String,
    pub effective: Option<String>, // "0901Z  09/03/26"
    pub expires: Option<String>,
    pub airport: String, // the ident we resolved against
    pub charts: Vec<ChartRef>,
}

type Index = HashMap<String, Vec<ChartRef>>;

#[derive(Serialize, Deserialize)]
struct CachedIndex {
    cycle: String,
    from_edate: String,
    to_edate: String,
    index: Index,
}

// --- metafile XML (quick-xml serde) ------------------------------------

#[derive(Deserialize)]
struct DigitalTpp {
    #[serde(rename = "@cycle")]
    cycle: String,
    #[serde(rename = "@from_edate", default)]
    from_edate: String,
    #[serde(rename = "@to_edate", default)]
    to_edate: String,
    #[serde(rename = "state_code", default)]
    states: Vec<StateCode>,
}

#[derive(Deserialize)]
struct StateCode {
    #[serde(rename = "city_name", default)]
    cities: Vec<CityName>,
}

#[derive(Deserialize)]
struct CityName {
    #[serde(rename = "airport_name", default)]
    airports: Vec<AirportName>,
}

#[derive(Deserialize)]
struct AirportName {
    #[serde(rename = "@apt_ident", default)]
    apt_ident: String,
    #[serde(rename = "@icao_ident", default)]
    icao_ident: String,
    #[serde(rename = "record", default)]
    records: Vec<ChartRecord>,
}

#[derive(Deserialize)]
struct ChartRecord {
    #[serde(default)]
    chart_code: String,
    #[serde(default)]
    chart_name: String,
    #[serde(default)]
    pdf_name: String,
}

fn parse_metafile(xml: &str) -> Result<CachedIndex> {
    let xml = xml.trim_start_matches('\u{feff}');
    let doc: DigitalTpp = quick_xml::de::from_str(xml).context("parse d-TPP metafile")?;
    let cycle = doc.cycle.trim().to_string();

    let mut index: Index = HashMap::new();
    for st in &doc.states {
        for city in &st.cities {
            for apt in &city.airports {
                let charts: Vec<ChartRef> = apt
                    .records
                    .iter()
                    .filter_map(|r| {
                        let pdf = r.pdf_name.trim();
                        if pdf.is_empty() || !pdf.to_ascii_uppercase().ends_with(".PDF") {
                            return None; // "DELETED" / blank rows
                        }
                        let code = r.chart_code.trim().to_ascii_uppercase();
                        Some(ChartRef {
                            name: r.chart_name.trim().to_string(),
                            group: group_for(&code).to_string(),
                            code,
                            url: format!("{AERONAV}/{cycle}/{pdf}"),
                            pdf_name: pdf.to_string(),
                        })
                    })
                    .collect();
                if charts.is_empty() {
                    continue;
                }
                for key in [apt.icao_ident.trim(), apt.apt_ident.trim()] {
                    if !key.is_empty() {
                        index
                            .entry(key.to_ascii_uppercase())
                            .or_insert_with(|| charts.clone());
                    }
                }
            }
        }
    }

    Ok(CachedIndex {
        cycle,
        from_edate: doc.from_edate.trim().to_string(),
        to_edate: doc.to_edate.trim().to_string(),
        index,
    })
}

// --- service -----------------------------------------------------------

pub struct Charts {
    db: Arc<Db>,
    client: reqwest::Client,
    mem: ArcSwapOption<CachedIndex>,
    fetch_lock: tokio::sync::Mutex<()>,
    since_sweep: AtomicI64,
}

impl Charts {
    pub fn new(db: Arc<Db>, client: reqwest::Client) -> Self {
        Self {
            db,
            client,
            mem: ArcSwapOption::empty(),
            fetch_lock: tokio::sync::Mutex::new(()),
            since_sweep: AtomicI64::new(0),
        }
    }

    /// Charts available for an airport, by ICAO or FAA ident.
    pub async fn charts_for(&self, airport: &str) -> Result<ChartSet> {
        let ci = self.index().await?;
        let q = airport.trim().to_ascii_uppercase();

        let charts = self
            .lookup(&ci.index, &q)
            .or_else(|| {
                // KJFK -> JFK (US ICAO to FAA ident) and vice versa
                if q.len() == 4 && q.starts_with('K') {
                    self.lookup(&ci.index, &q[1..])
                } else if q.len() == 3 {
                    self.lookup(&ci.index, &format!("K{q}"))
                } else {
                    None
                }
            })
            .unwrap_or_default();

        Ok(ChartSet {
            cycle: ci.cycle.clone(),
            effective: opt(&ci.from_edate),
            expires: opt(&ci.to_edate),
            airport: q,
            charts,
        })
    }

    fn lookup(&self, index: &Index, key: &str) -> Option<Vec<ChartRef>> {
        index.get(key).cloned()
    }

    async fn index(&self) -> Result<Arc<CachedIndex>> {
        if let Some(ci) = self.mem.load_full() {
            return Ok(ci);
        }
        let _guard = self.fetch_lock.lock().await;
        if let Some(ci) = self.mem.load_full() {
            return Ok(ci);
        }

        // Persisted index for the current or an adjacent cycle.
        for c in candidate_cycles() {
            if let Some(json) = self.db.kv_get(&format!("charts:index:{c}"), INDEX_TTL_MS)? {
                if let Ok(ci) = serde_json::from_str::<CachedIndex>(&json) {
                    let ci = Arc::new(ci);
                    self.mem.store(Some(ci.clone()));
                    return Ok(ci);
                }
            }
        }

        let (cycle, xml) = self.fetch_metafile().await?;
        let ci = Arc::new(parse_metafile(&xml)?);
        tracing::info!(
            "d-TPP index: cycle {} ({} airports)",
            ci.cycle,
            ci.index.len()
        );
        if let Ok(json) = serde_json::to_string(&*ci) {
            let _ = self.db.kv_put(&format!("charts:index:{cycle}"), &json);
        }
        self.mem.store(Some(ci.clone()));
        Ok(ci)
    }

    async fn fetch_metafile(&self) -> Result<(String, String)> {
        let mut last_err: Option<anyhow::Error> = None;
        for cycle in candidate_cycles() {
            let url = format!("{AERONAV}/{cycle}/xml_data/d-TPP_Metafile.xml");
            match self
                .client
                .get(&url)
                .timeout(Duration::from_secs(60))
                .send()
                .await
            {
                Ok(r) if r.status().is_success() => match r.text().await {
                    Ok(text) if text.contains("<digital_tpp") => return Ok((cycle, text)),
                    Ok(_) => last_err = Some(anyhow!("cycle {cycle}: unexpected body")),
                    Err(e) => last_err = Some(e.into()),
                },
                Ok(r) => last_err = Some(anyhow!("cycle {cycle}: HTTP {}", r.status())),
                Err(e) => last_err = Some(e.into()),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("no reachable d-TPP cycle")))
    }

    // --- PDF fetch + cache (chart_pdf table, LRU by last_used) ---

    pub async fn pdf(&self, url: &str) -> Result<Vec<u8>> {
        if !is_allowed_pdf_url(url) {
            return Err(anyhow!("refusing to fetch non-d-TPP URL"));
        }
        if let Some(bytes) = self.pdf_get(url)? {
            self.pdf_touch(url);
            return Ok(bytes);
        }
        let resp = self
            .client
            .get(url)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .with_context(|| format!("fetch {url}"))?;
        if !resp.status().is_success() {
            return Err(anyhow!("chart PDF HTTP {} for {url}", resp.status()));
        }
        let bytes = resp.bytes().await?.to_vec();
        if !bytes.starts_with(b"%PDF") {
            return Err(anyhow!("response for {url} is not a PDF"));
        }
        self.pdf_put(url, &bytes)?;
        self.maybe_sweep(bytes.len() as i64);
        Ok(bytes)
    }

    fn pdf_get(&self, url: &str) -> Result<Option<Vec<u8>>> {
        self.db.with_conn(|c| {
            let mut stmt = c.prepare("SELECT data FROM chart_pdf WHERE url = ?1")?;
            let mut rows = stmt.query([url])?;
            Ok(rows.next()?.map(|r| r.get::<_, Vec<u8>>(0)).transpose()?)
        })
    }

    fn pdf_put(&self, url: &str, bytes: &[u8]) -> Result<()> {
        let now = now_ms();
        self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO chart_pdf(url, data, bytes, fetched_at, last_used)
                 VALUES(?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(url) DO UPDATE SET
                   data = excluded.data, bytes = excluded.bytes,
                   fetched_at = excluded.fetched_at, last_used = excluded.last_used",
                rusqlite::params![url, bytes, bytes.len() as i64, now],
            )?;
            Ok(())
        })
    }

    fn pdf_touch(&self, url: &str) {
        let _ = self.db.with_conn(|c| {
            c.execute(
                "UPDATE chart_pdf SET last_used = ?2 WHERE url = ?1",
                rusqlite::params![url, now_ms()],
            )?;
            Ok(())
        });
    }

    fn maybe_sweep(&self, added: i64) {
        let n = self.since_sweep.fetch_add(added, Ordering::Relaxed) + added;
        if n > 16 * 1024 * 1024 {
            self.since_sweep.store(0, Ordering::Relaxed);
            let _ = self.enforce_pdf_limit();
        }
    }

    fn enforce_pdf_limit(&self) -> Result<()> {
        self.db.with_conn(|c| {
            let total: i64 =
                c.query_row("SELECT COALESCE(SUM(bytes),0) FROM chart_pdf", [], |r| r.get(0))?;
            let mut over = total - PDF_CACHE_MAX_BYTES;
            if over <= 0 {
                return Ok(());
            }
            let mut stmt =
                c.prepare("SELECT url, bytes FROM chart_pdf ORDER BY last_used ASC")?;
            let victims: Vec<(String, i64)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .filter_map(|x| x.ok())
                .collect();
            for (url, bytes) in victims {
                if over <= 0 {
                    break;
                }
                c.execute("DELETE FROM chart_pdf WHERE url = ?1", [&url])?;
                over -= bytes;
            }
            Ok(())
        })
    }
}

fn is_allowed_pdf_url(url: &str) -> bool {
    url.starts_with("https://aeronav.faa.gov/d-tpp/") && url.to_ascii_uppercase().ends_with(".PDF")
}

fn opt(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn cycle_numbers_match_known_dates() {
        // Anchor and neighbours (28-day steps).
        assert_eq!(cycle_for(d(2025, 1, 23)), "2501");
        assert_eq!(cycle_for(d(2025, 2, 19)), "2501"); // day before 2502
        assert_eq!(cycle_for(d(2025, 2, 20)), "2502");
        assert_eq!(cycle_for(d(2024, 12, 26)), "2413");
        // Confirmed against the live metafile header.
        assert_eq!(cycle_for(d(2026, 9, 3)), "2609");
        assert_eq!(cycle_for(d(2026, 9, 30)), "2609");
        // Year rollover: 2513 stays in effect into January 2026.
        assert_eq!(cycle_for(d(2025, 12, 25)), "2513");
        assert_eq!(cycle_for(d(2026, 1, 21)), "2513");
        assert_eq!(cycle_for(d(2026, 1, 22)), "2601");
    }

    #[test]
    fn parses_metafile_shape() {
        let xml = r#"<?xml version="1.0"?>
<digital_tpp cycle="2609" from_edate="0901Z  09/03/26" to_edate="0901Z  10/01/26">
  <state_code ID="OH" state_fullname="OHIO">
    <city_name ID="CLEVELAND" volume="NC-3">
      <airport_name ID="CLEVELAND-HOPKINS INTL" military="N" apt_ident="CLE" icao_ident="KCLE">
        <record>
          <chartseq>10000</chartseq><chart_code>APD</chart_code>
          <chart_name>AIRPORT DIAGRAM</chart_name><pdf_name>00013AD.PDF</pdf_name>
        </record>
        <record>
          <chartseq>52000</chartseq><chart_code>IAP</chart_code>
          <chart_name>ILS OR LOC RWY 06R</chart_name><pdf_name>00013IL6R.PDF</pdf_name>
        </record>
        <record>
          <chartseq>99999</chartseq><chart_code>IAP</chart_code>
          <chart_name>DELETED</chart_name><pdf_name></pdf_name>
        </record>
      </airport_name>
    </city_name>
  </state_code>
</digital_tpp>"#;
        let ci = parse_metafile(xml).unwrap();
        assert_eq!(ci.cycle, "2609");
        let cle = ci.index.get("KCLE").unwrap();
        assert_eq!(cle.len(), 2); // blank pdf_name row dropped
        assert_eq!(cle[0].group, "Airport Diagram");
        assert_eq!(
            cle[1].url,
            "https://aeronav.faa.gov/d-tpp/2609/00013IL6R.PDF"
        );
        assert_eq!(ci.index.get("CLE").unwrap().len(), 2); // also keyed by FAA ident
    }

    #[test]
    fn pdf_url_guard() {
        assert!(is_allowed_pdf_url(
            "https://aeronav.faa.gov/d-tpp/2609/00013IL6R.PDF"
        ));
        assert!(!is_allowed_pdf_url("https://evil.example/x.PDF"));
        assert!(!is_allowed_pdf_url(
            "https://aeronav.faa.gov/d-tpp/2609/../../etc/passwd"
        ));
    }
}
