//! VOR listener: tunes the RTL-SDR to a VOR frequency, continuously decodes
//! the radial + Morse ident (`vor_dsp`), and exposes them alongside the
//! geometric radial from a saved place. Single-dongle tune/handoff skeleton is
//! the same one `atc.rs` / `acars/mod.rs` use (open with retries, pause ADS-B
//! if sharing the device) — duplicated for the reasons in the ACARS module doc.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use rs_rtl::{DeviceId, RtlSdr};
use serde::Serialize;

use crate::config::AppSettings;
use crate::enrich::navaids::Navaid;
use crate::ingest::rtlsdr::demod;
use crate::ingest::RtlSdrSource;
use crate::nav::fix::{position_fix, Lop, PositionFix};
use crate::nav::geo;
use crate::nav::vor_dsp::{estimate_radial, IdentDecoder};

use std::collections::VecDeque;
use std::time::Instant;

const CAPTURE_HZ: u32 = 240_000;
/// Decimate the AM envelope to roughly this before decoding — comfortably
/// above the 10.5 kHz VOR composite while keeping the 30 Hz work cheap.
const TARGET_HZ: f64 = 24_000.0;
const GAIN_TENTHS_DB: i32 = 400;
/// Envelope seconds per radial estimate (longer = steadier, slower to settle).
const WINDOW_SECS: f64 = 0.5;
const VOR_KINDS: [&str; 3] = ["VOR", "VOR-DME", "VORTAC"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VorStatus {
    pub running: bool,
    pub device_open: bool,
    pub tuned_khz: u32,
    pub freq_mhz: Option<f64>,
    pub station_ident: Option<String>,
    pub station_name: Option<String>,
    pub station_kind: Option<String>,
    pub station_lat: Option<f64>,
    pub station_lon: Option<f64>,
    pub station_variation_deg: Option<f64>,
    pub has_dme: bool,
    /// Ident the station is published as (what the decoder should hear).
    pub expected_ident: Option<String>,
    pub decoded_ident: Option<String>,
    /// `None` until the decoder has produced a group to compare.
    pub ident_ok: Option<bool>,
    pub received_radial_deg: Option<f64>,
    pub geometric_radial_deg: Option<f64>,
    /// received − geometric, in (−180, 180].
    pub radial_delta_deg: Option<f64>,
    /// Ground distance (nm) from the saved place to the station.
    pub distance_nm: Option<f64>,
    /// Rough 0–1 confidence from the recovered tone levels.
    pub signal: f64,
    pub adsb_paused: bool,
    pub last_error: Option<String>,
    /// Present while a multi-station position fix is running or just finished.
    pub fix: Option<VorFixStatus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VorFixStatus {
    /// "tuning" | "done" | "failed"
    pub phase: String,
    pub station_index: usize,
    pub station_count: usize,
    pub current_ident: Option<String>,
    pub collected: Vec<CollectedLopInfo>,
    pub result: Option<VorFixResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectedLopInfo {
    pub ident: String,
    pub radial_mag_deg: Option<f64>,
    pub ident_ok: Option<bool>,
    pub signal: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VorFixResult {
    pub lat: f64,
    pub lon: f64,
    pub uncertainty_nm: f64,
    /// Pairwise crossing points `[lat, lon]`, for drawing the cocked hat.
    pub crossings: Vec<[f64; 2]>,
    pub lop_count: usize,
    /// Offset from the primary place, if one is set.
    pub distance_from_place_nm: Option<f64>,
    pub bearing_from_place_deg: Option<f64>,
}

struct Session {
    ident: String,
    name: String,
    kind: String,
    lat: f64,
    lon: f64,
    variation_deg: f64,
    has_dme: bool,
    freq_khz: f64,
    /// Saved-place observer position, if one is set.
    obs: Option<(f64, f64)>,
    received_radial: Option<f64>,
    decoded_ident: Option<String>,
    signal: f64,
}

impl Session {
    fn status_fields(&self, s: &mut VorStatus) {
        s.freq_mhz = Some(self.freq_khz / 1000.0);
        s.station_ident = Some(self.ident.clone());
        s.station_name = Some(self.name.clone());
        s.station_kind = Some(self.kind.clone());
        s.station_lat = Some(self.lat);
        s.station_lon = Some(self.lon);
        s.station_variation_deg = Some(self.variation_deg);
        s.has_dme = self.has_dme;
        s.expected_ident = Some(self.ident.clone());
        s.decoded_ident = self.decoded_ident.clone();
        s.ident_ok = self
            .decoded_ident
            .as_ref()
            .map(|d| d.eq_ignore_ascii_case(&self.ident));
        s.signal = self.signal;
        s.received_radial_deg = self.received_radial;

        if let Some((olat, olon)) = self.obs {
            let geo_r = geo::geometric_radial(self.lat, self.lon, olat, olon, self.variation_deg);
            s.geometric_radial_deg = Some(geo_r);
            s.distance_nm = Some(geo::haversine_nm(self.lat, self.lon, olat, olon));
            if let Some(recv) = self.received_radial {
                s.radial_delta_deg = Some(geo::angle_diff(recv, geo_r));
            }
        }
    }
}

enum FixPhase {
    Tuning,
    Done,
    Failed,
}

struct Collected {
    ident: String,
    radial_mag: Option<f64>,
    ident_ok: Option<bool>,
    signal: f64,
}

struct FixJob {
    stations: Vec<Navaid>,
    obs: Option<(f64, f64)>,
    current: usize,
    phase: FixPhase,
    collected: Vec<Collected>,
    result: Option<PositionFix>,
    error: Option<String>,
}

impl FixJob {
    fn to_status(&self) -> VorFixStatus {
        let result = self.result.as_ref().map(|f| {
            let (dist, brg) = match self.obs {
                Some((olat, olon)) => {
                    let o = crate::nav::fix::offset_from(f, olat, olon);
                    (Some(o.0), Some(o.1))
                }
                None => (None, None),
            };
            VorFixResult {
                lat: f.lat,
                lon: f.lon,
                uncertainty_nm: f.uncertainty_nm,
                crossings: f.crossings.iter().map(|&(la, lo)| [la, lo]).collect(),
                lop_count: f.lop_count,
                distance_from_place_nm: dist,
                bearing_from_place_deg: brg,
            }
        });
        VorFixStatus {
            phase: match self.phase {
                FixPhase::Tuning => "tuning",
                FixPhase::Done => "done",
                FixPhase::Failed => "failed",
            }
            .into(),
            station_index: self.current,
            station_count: self.stations.len(),
            current_ident: matches!(self.phase, FixPhase::Tuning)
                .then(|| self.stations.get(self.current).map(|s| s.ident.clone()))
                .flatten(),
            collected: self
                .collected
                .iter()
                .map(|c| CollectedLopInfo {
                    ident: c.ident.clone(),
                    radial_mag_deg: c.radial_mag,
                    ident_ok: c.ident_ok,
                    signal: c.signal,
                })
                .collect(),
            result,
            error: self.error.clone(),
        }
    }
}

pub struct VorListener {
    settings: Arc<Mutex<AppSettings>>,
    rtlsdr_source: Arc<RtlSdrSource>,
    running: Arc<AtomicBool>,
    device_open: Arc<AtomicBool>,
    adsb_paused: Arc<AtomicBool>,
    tuned_khz: Arc<AtomicU32>,
    last_error: Arc<Mutex<Option<String>>>,
    session: Arc<Mutex<Option<Session>>>,
    fix: Arc<Mutex<Option<FixJob>>>,
}

impl VorListener {
    pub fn new(settings: Arc<Mutex<AppSettings>>, rtlsdr_source: Arc<RtlSdrSource>) -> Self {
        Self {
            settings,
            rtlsdr_source,
            running: Arc::new(AtomicBool::new(false)),
            device_open: Arc::new(AtomicBool::new(false)),
            adsb_paused: Arc::new(AtomicBool::new(false)),
            tuned_khz: Arc::new(AtomicU32::new(0)),
            last_error: Arc::new(Mutex::new(None)),
            session: Arc::new(Mutex::new(None)),
            fix: Arc::new(Mutex::new(None)),
        }
    }

    pub fn status(&self) -> VorStatus {
        let mut s = VorStatus {
            running: self.running.load(Ordering::SeqCst),
            device_open: self.device_open.load(Ordering::SeqCst),
            tuned_khz: self.tuned_khz.load(Ordering::SeqCst),
            freq_mhz: None,
            station_ident: None,
            station_name: None,
            station_kind: None,
            station_lat: None,
            station_lon: None,
            station_variation_deg: None,
            has_dme: false,
            expected_ident: None,
            decoded_ident: None,
            ident_ok: None,
            received_radial_deg: None,
            geometric_radial_deg: None,
            radial_delta_deg: None,
            distance_nm: None,
            signal: 0.0,
            adsb_paused: self.adsb_paused.load(Ordering::SeqCst),
            last_error: self.last_error.lock().clone(),
            fix: None,
        };
        if let Some(sess) = self.session.lock().as_ref() {
            sess.status_fields(&mut s);
        }
        if let Some(job) = self.fix.lock().as_ref() {
            s.fix = Some(job.to_status());
        }
        s
    }

    pub async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        for _ in 0..50 {
            if !self.device_open.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        self.tuned_khz.store(0, Ordering::SeqCst);
        *self.session.lock() = None;
        *self.fix.lock() = None;
        *self.last_error.lock() = None;
        if self.adsb_paused.swap(false, Ordering::SeqCst) {
            self.settings.lock().rtlsdr_enabled = true;
        }
    }

    /// Tune to a VOR by its bundled record. Errors for non-VOR navaids or a
    /// frequency outside the nav band.
    pub async fn tune(&self, navaid: &Navaid, device_index: u32) -> Result<()> {
        if !VOR_KINDS.contains(&navaid.kind.as_str()) {
            return Err(anyhow!(
                "{} is a {} — only VOR / VOR-DME / VORTAC carry a decodable bearing signal",
                navaid.ident,
                navaid.kind
            ));
        }
        if !(108_000.0..=118_000.0).contains(&navaid.freq_khz) {
            return Err(anyhow!(
                "{} is on {:.2} MHz, outside the VOR band",
                navaid.ident,
                navaid.freq_khz / 1000.0
            ));
        }
        self.stop().await;

        let obs = {
            let s = self.settings.lock();
            s.places
                .iter()
                .find(|p| p.primary)
                .or_else(|| s.places.first())
                .map(|p| (p.lat, p.lon))
        };

        let sharing_adsb_device = {
            let s = self.settings.lock();
            s.rtlsdr_enabled && s.rtlsdr_device_index == device_index
        };
        if sharing_adsb_device {
            self.settings.lock().rtlsdr_enabled = false;
            self.adsb_paused.store(true, Ordering::SeqCst);
            for _ in 0..60 {
                if !self.rtlsdr_source.status().device_open {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        *self.session.lock() = Some(Session {
            ident: navaid.ident.clone(),
            name: navaid.name.clone(),
            kind: navaid.kind.clone(),
            lat: navaid.lat,
            lon: navaid.lon,
            variation_deg: navaid.station_variation_deg.unwrap_or(0.0),
            has_dme: navaid.has_dme,
            freq_khz: navaid.freq_khz,
            obs,
            received_radial: None,
            decoded_ident: None,
            signal: 0.0,
        });
        *self.last_error.lock() = None;
        self.running.store(true, Ordering::SeqCst);
        self.tuned_khz
            .store(navaid.freq_khz.round() as u32, Ordering::SeqCst);

        let freq_hz = (navaid.freq_khz * 1000.0).round() as u32;
        let running = self.running.clone();
        let device_open = self.device_open.clone();
        let session = self.session.clone();
        let last_error = self.last_error.clone();
        let adsb_paused = self.adsb_paused.clone();
        let settings = self.settings.clone();
        std::thread::spawn(move || {
            run_worker(freq_hz, device_index, &running, &device_open, &session, &last_error);
            running.store(false, Ordering::SeqCst);
            device_open.store(false, Ordering::SeqCst);
            if adsb_paused.swap(false, Ordering::SeqCst) {
                settings.lock().rtlsdr_enabled = true;
            }
        });
        Ok(())
    }

    /// Run a multi-station position fix: tune each station in turn, hold a
    /// radial, then cross them. `stations` is filtered to decodable VOR-family
    /// records in band; needs at least 2.
    pub async fn start_fix(&self, stations: Vec<Navaid>, device_index: u32) -> Result<()> {
        let stations: Vec<Navaid> = stations
            .into_iter()
            .filter(|n| {
                VOR_KINDS.contains(&n.kind.as_str())
                    && (108_000.0..=118_000.0).contains(&n.freq_khz)
            })
            .take(6)
            .collect();
        if stations.len() < 2 {
            return Err(anyhow!(
                "need at least 2 usable VOR / VOR-DME / VORTAC stations for a fix"
            ));
        }
        self.stop().await;

        let obs = {
            let s = self.settings.lock();
            s.places
                .iter()
                .find(|p| p.primary)
                .or_else(|| s.places.first())
                .map(|p| (p.lat, p.lon))
        };

        let sharing_adsb_device = {
            let s = self.settings.lock();
            s.rtlsdr_enabled && s.rtlsdr_device_index == device_index
        };
        if sharing_adsb_device {
            self.settings.lock().rtlsdr_enabled = false;
            self.adsb_paused.store(true, Ordering::SeqCst);
            for _ in 0..60 {
                if !self.rtlsdr_source.status().device_open {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        *self.fix.lock() = Some(FixJob {
            stations: stations.clone(),
            obs,
            current: 0,
            phase: FixPhase::Tuning,
            collected: Vec::new(),
            result: None,
            error: None,
        });
        *self.last_error.lock() = None;
        self.running.store(true, Ordering::SeqCst);

        let running = self.running.clone();
        let device_open = self.device_open.clone();
        let job = self.fix.clone();
        let adsb_paused = self.adsb_paused.clone();
        let settings = self.settings.clone();
        std::thread::spawn(move || {
            run_fix_worker(stations, device_index, &running, &device_open, &job);
            running.store(false, Ordering::SeqCst);
            device_open.store(false, Ordering::SeqCst);
            if adsb_paused.swap(false, Ordering::SeqCst) {
                settings.lock().rtlsdr_enabled = true;
            }
        });
        Ok(())
    }
}

struct RtlSession {
    #[allow(dead_code)]
    sdr: RtlSdr,
    reader: rs_rtl::AsyncReadHandle,
    actual_rate: u32,
}

/// See `atc::open_and_stream` — identical cold-open/retry sequence.
fn open_and_stream(device_index: u32, freq_hz: u32) -> rs_rtl::Result<RtlSession> {
    const ATTEMPTS: u32 = 5;
    let mut last_err = None;
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(200 * attempt as u64));
        }
        let opened = (|| -> rs_rtl::Result<RtlSession> {
            let mut sdr = RtlSdr::open(DeviceId::Index(device_index as usize))?;
            sdr.set_center_freq(freq_hz)?;
            sdr.set_sample_rate(CAPTURE_HZ)?;
            sdr.set_gain_manual(GAIN_TENTHS_DB)?;
            let actual_rate = sdr.sample_rate();
            let reader = sdr.start_streaming()?;
            Ok(RtlSession { sdr, reader, actual_rate })
        })();
        match opened {
            Ok(s) => return Ok(s),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.expect("ATTEMPTS > 0"))
}

fn decimate(mag: &[u16], factor: usize) -> Vec<f32> {
    if factor <= 1 {
        return mag.iter().map(|&v| v as f32).collect();
    }
    mag.chunks(factor)
        .map(|c| c.iter().map(|&v| v as f32).sum::<f32>() / c.len() as f32)
        .collect()
}

/// Widest angular gap among the tracked recent radials — a settle test.
fn radial_spread(v: &VecDeque<f64>) -> f64 {
    let mut m = 0.0_f64;
    for i in 0..v.len() {
        for j in (i + 1)..v.len() {
            m = m.max(geo::angle_diff(v[i], v[j]).abs());
        }
    }
    m
}

/// Hold on one already-open station, decoding until the radial settles or
/// `max_dwell` elapses. Returns the smoothed magnetic radial (if any lock),
/// whether the decoded ident matched `expected`, and a rough signal level.
fn collect_one_radial(
    rtl: &RtlSession,
    running: &AtomicBool,
    expected: &str,
    min_dwell: Duration,
    max_dwell: Duration,
) -> (Option<f64>, Option<bool>, f64) {
    let decim = ((rtl.actual_rate as f64 / TARGET_HZ).round() as usize).max(1);
    let eff_rate = rtl.actual_rate as f64 / decim as f64;
    let window = (eff_rate * WINDOW_SECS) as usize;
    let mut ident = IdentDecoder::new(eff_rate);
    let mut env_buf: Vec<f32> = Vec::with_capacity(window * 2);
    let mut radial: Option<f64> = None;
    let mut signal = 0.0_f64;
    let mut recent: VecDeque<f64> = VecDeque::new();
    let start = Instant::now();

    while running.load(Ordering::SeqCst) && start.elapsed() < max_dwell {
        let Some(iq) = rtl.reader.recv() else { break };
        let dec = decimate(&demod::magnitude(&iq), decim);
        ident.push(&dec);
        env_buf.extend_from_slice(&dec);
        if env_buf.len() >= window {
            if let Some(e) = estimate_radial(&env_buf, eff_rate) {
                radial = Some(match radial {
                    Some(p) => geo::wrap360(p + geo::angle_diff(e.radial_deg, p) * 0.4),
                    None => e.radial_deg,
                });
                signal += ((e.var_level / 0.30).clamp(0.0, 1.0) - signal) * 0.4;
                recent.push_back(radial.unwrap());
                if recent.len() > 6 {
                    recent.pop_front();
                }
                if recent.len() >= 6
                    && start.elapsed() >= min_dwell
                    && radial_spread(&recent) < 1.5
                {
                    break;
                }
            }
            env_buf.clear();
        }
    }
    let ident_ok = ident.current().map(|d| d.eq_ignore_ascii_case(expected));
    (radial, ident_ok, signal)
}

fn run_fix_worker(
    stations: Vec<Navaid>,
    device_index: u32,
    running: &Arc<AtomicBool>,
    device_open: &Arc<AtomicBool>,
    job: &Arc<Mutex<Option<FixJob>>>,
) {
    const MIN_DWELL: Duration = Duration::from_secs(6);
    const MAX_DWELL: Duration = Duration::from_secs(18);
    let mut lops: Vec<Lop> = Vec::new();

    for (i, sta) in stations.iter().enumerate() {
        if !running.load(Ordering::SeqCst) {
            break;
        }
        if let Some(j) = job.lock().as_mut() {
            j.current = i;
        }
        let freq_hz = (sta.freq_khz * 1000.0).round() as u32;
        let (radial, ident_ok, signal) = match open_and_stream(device_index, freq_hz) {
            Ok(rtl) => {
                device_open.store(true, Ordering::SeqCst);
                let r = collect_one_radial(&rtl, running, &sta.ident, MIN_DWELL, MAX_DWELL);
                drop(rtl);
                device_open.store(false, Ordering::SeqCst);
                r
            }
            Err(e) => {
                tracing::warn!("fix: opening {} failed: {e}", sta.ident);
                (None, None, 0.0)
            }
        };

        if let Some(r) = radial {
            lops.push(Lop {
                lat: sta.lat,
                lon: sta.lon,
                true_bearing_deg: geo::wrap360(r + sta.station_variation_deg.unwrap_or(0.0)),
            });
        }
        if let Some(j) = job.lock().as_mut() {
            j.collected.push(Collected {
                ident: sta.ident.clone(),
                radial_mag: radial,
                ident_ok,
                signal,
            });
        }
    }

    let result = position_fix(&lops);
    if let Some(j) = job.lock().as_mut() {
        j.current = j.stations.len();
        match result {
            Some(f) => {
                j.result = Some(f);
                j.phase = FixPhase::Done;
            }
            None => {
                j.phase = FixPhase::Failed;
                j.error = Some(
                    if lops.len() < 2 {
                        "couldn't hold a stable radial from at least two stations"
                    } else {
                        "the radials didn't cross cleanly — try again, or pick stations with a wider angle between them"
                    }
                    .into(),
                );
            }
        }
    }
}

fn run_worker(
    freq_hz: u32,
    device_index: u32,
    running: &Arc<AtomicBool>,
    device_open: &Arc<AtomicBool>,
    session: &Arc<Mutex<Option<Session>>>,
    last_error: &Arc<Mutex<Option<String>>>,
) {
    let rtl = match open_and_stream(device_index, freq_hz) {
        Ok(r) => r,
        Err(e) => {
            *last_error.lock() = Some(format!("couldn't open RTL-SDR #{device_index}: {e}"));
            return;
        }
    };
    let decim = ((rtl.actual_rate as f64 / TARGET_HZ).round() as usize).max(1);
    let eff_rate = rtl.actual_rate as f64 / decim as f64;
    let window = (eff_rate * WINDOW_SECS) as usize;

    let mut ident = IdentDecoder::new(eff_rate);
    let mut env_buf: Vec<f32> = Vec::with_capacity(window * 2);

    *last_error.lock() = None;
    device_open.store(true, Ordering::SeqCst);

    while running.load(Ordering::SeqCst) {
        let Some(iq) = rtl.reader.recv() else {
            *last_error.lock() = Some("RTL-SDR stream ended unexpectedly".into());
            break;
        };
        let mag = demod::magnitude(&iq);
        let dec = decimate(&mag, decim);
        ident.push(&dec);
        env_buf.extend_from_slice(&dec);

        if env_buf.len() >= window {
            let est = estimate_radial(&env_buf, eff_rate);
            env_buf.clear();
            let decoded = ident.current().map(|s| s.to_string());
            if let Some(sess) = session.lock().as_mut() {
                if let Some(e) = est {
                    sess.received_radial = Some(match sess.received_radial {
                        Some(prev) => {
                            geo::wrap360(prev + geo::angle_diff(e.radial_deg, prev) * 0.35)
                        }
                        None => e.radial_deg,
                    });
                    // Rough bar: variable tone near its ~0.30 nominal depth AND
                    // a present reference tone (no reference = no valid bearing).
                    let var_ok = (e.var_level / 0.30).clamp(0.0, 1.0);
                    let ref_ok = (e.ref_level * 400.0).clamp(0.0, 1.0);
                    let s = var_ok.min(ref_ok.max(0.2));
                    sess.signal += (s - sess.signal) * 0.4;
                } else {
                    sess.signal += (0.0 - sess.signal) * 0.2;
                }
                if decoded.is_some() {
                    sess.decoded_ident = decoded;
                }
            }
        }
    }
    // Retune failures aside, dropping `rtl` releases the device (see atc.rs).
    drop(rtl);
}
