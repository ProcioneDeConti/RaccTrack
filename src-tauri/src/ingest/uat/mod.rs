//! Direct UAT (978 MHz) reception — a second RTL-SDR-decoded ADS-B band,
//! used in the US by GA aircraft below 18,000ft as an alternative to
//! 1090ES. Runs as its own background `AircraftSource`, merged into the
//! existing multi-source aircraft list exactly like `super::rtlsdr`
//! (same "own OS thread for as long as a setting stays on" shape) — not a
//! manual Start/Stop session like `atc`/`acars`, since it's meant to just
//! quietly contribute to the live picture.
//!
//! `demod` does the physical layer (true FM/phase demod, unlike the
//! AM-envelope path everything else in this app uses), `rs` is the
//! Reed-Solomon FEC, `frame` extracts message fields. See those modules'
//! docs for where the parameters come from (`dump978`, not rederived).
//!
//! Scope cut: only aircraft-transmitted ("downlink") messages are decoded
//! — no ground-station FIS-B weather / TIS-B traffic ("uplink"), which
//! would be a separate weather-ingest-shaped feature. A single RTL-SDR can
//! only tune to one frequency at a time, so this competes for a dongle
//! with `rtlsdr`/`atc`/`acars` the same way they compete with each other —
//! but unlike those, this doesn't attempt the same-device pause/resume
//! handoff dance; if `uat_device_index` collides with whichever of the
//! others is actively using that dongle, opening the device just fails
//! and shows up as a plain error in `UatStatus`. Building N-way device
//! arbitration for a fourth contender wasn't worth it for this feature —
//! two dongles (or not running 1090ES direct at the same time) sidesteps
//! it entirely.

mod demod;
mod frame;
mod rs;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use parking_lot::Mutex;
use rs_rtl::{DeviceId, RtlSdr};
use serde::Serialize;

use super::{AircraftSource, PointQuery};
use crate::config::AppSettings;
use crate::ingest::model::{AltBaro, RawAircraft};
use crate::util::now_ms;
use rs::RsCode;

pub const NAME: &str = "uat";

const CENTER_FREQ_HZ: u32 = 978_000_000;
const SAMPLE_RATE_HZ: u32 = demod::SAMPLE_HZ;
/// Fixed manual gain — same lesson learned elsewhere in this app (RTL2832U
/// hardware AGC is too weak/inconsistent to rely on); unverified against
/// real UAT hardware/antenna, a starting point to retune once tested.
const GAIN_TENTHS_DB: i32 = 400;
const STALE_MS: i64 = 60_000;
/// Short message: 18 data + 12 parity bytes.
const SHORT_DATA_LEN: usize = 18;
const SHORT_TOTAL_LEN: usize = 30;
/// Long message: 34 data + 14 parity bytes.
const LONG_DATA_LEN: usize = 34;

struct Track {
    hex: String,
    callsign: Option<String>,
    emitter_category: Option<u8>,
    alt_baro: Option<f64>,
    alt_geom: Option<f64>,
    lat: Option<f64>,
    lon: Option<f64>,
    ground_speed: Option<f64>,
    track_deg: Option<f64>,
    baro_rate: Option<f64>,
    geom_rate: Option<f64>,
    emergency: Option<String>,
    on_ground: bool,
    last_seen_ms: i64,
    last_pos_ms: Option<i64>,
}

impl Track {
    fn new(hex: String, now: i64) -> Self {
        Self {
            hex,
            callsign: None,
            emitter_category: None,
            alt_baro: None,
            alt_geom: None,
            lat: None,
            lon: None,
            ground_speed: None,
            track_deg: None,
            baro_rate: None,
            geom_rate: None,
            emergency: None,
            on_ground: false,
            last_seen_ms: now,
            last_pos_ms: None,
        }
    }

    fn apply(&mut self, m: frame::UatAdsb, now: i64) {
        self.last_seen_ms = now;
        if let Some(lat) = m.lat {
            self.lat = Some(lat);
            self.lon = m.lon;
            self.last_pos_ms = Some(now);
        }
        if let Some(alt) = m.altitude_ft {
            if m.altitude_is_geometric {
                self.alt_geom = Some(alt);
            } else {
                self.alt_baro = Some(alt);
            }
        }
        self.on_ground = m.on_ground;
        self.emergency = m.emergency_status;
        if m.ground_speed_kt.is_some() {
            self.ground_speed = m.ground_speed_kt;
            self.track_deg = m.track_deg;
        }
        if m.vert_rate_fpm.is_some() {
            self.baro_rate = m.vert_rate_fpm;
        }
        if m.callsign.is_some() {
            self.callsign = m.callsign;
        }
        if m.emitter_category.is_some() {
            self.emitter_category = m.emitter_category;
        }
    }

    fn to_raw(&self, now: i64) -> RawAircraft {
        RawAircraft {
            hex: Some(self.hex.clone()),
            r#type: Some("adsb_icao".into()),
            flight: self.callsign.clone(),
            r: None,
            t: None,
            desc: None,
            category: self.emitter_category.map(|c| format!("A{c}")),
            alt_baro: self.alt_baro.map(AltBaro::Num),
            alt_geom: self.alt_geom,
            gs: self.ground_speed,
            ias: None,
            tas: None,
            mach: None,
            track: self.track_deg,
            mag_heading: None,
            true_heading: None,
            roll: None,
            track_rate: None,
            baro_rate: self.baro_rate,
            geom_rate: self.geom_rate,
            squawk: None,
            emergency: None,
            nav_altitude_mcp: None,
            nav_altitude_fms: None,
            nav_heading: None,
            nav_qnh: None,
            lat: self.lat,
            lon: self.lon,
            rssi: None,
            messages: None,
            seen: Some((now - self.last_seen_ms).max(0) as f64 / 1000.0),
            seen_pos: self.last_pos_ms.map(|t| (now - t).max(0) as f64 / 1000.0),
            mlat: Vec::new(),
            tisb: Vec::new(),
            db_flags: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UatStatus {
    pub enabled: bool,
    pub device_open: bool,
    /// Sync words found (before FEC) — near zero means no signal at all,
    /// same diagnostic role as `RtlSdrStatus.raw_candidates`.
    pub frames_found: u64,
    /// Of those, how many passed Reed-Solomon (a real message).
    pub messages_decoded: u64,
    pub aircraft_tracked: usize,
    pub last_error: Option<String>,
}

pub struct UatSource {
    settings: Arc<Mutex<AppSettings>>,
    tracks: Arc<Mutex<HashMap<String, Track>>>,
    running: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
    device_open: Arc<AtomicBool>,
    frames_found: Arc<AtomicU64>,
    messages_decoded: Arc<AtomicU64>,
}

impl UatSource {
    pub fn new(settings: Arc<Mutex<AppSettings>>) -> Self {
        Self {
            settings,
            tracks: Arc::new(Mutex::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            started: Arc::new(AtomicBool::new(false)),
            last_error: Arc::new(Mutex::new(None)),
            device_open: Arc::new(AtomicBool::new(false)),
            frames_found: Arc::new(AtomicU64::new(0)),
            messages_decoded: Arc::new(AtomicU64::new(0)),
        }
    }

    fn ensure_started(&self) {
        let enabled = self.settings.lock().uat_enabled;
        if enabled
            && self
                .started
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            self.running.store(true, Ordering::SeqCst);
            let settings = self.settings.clone();
            let tracks = self.tracks.clone();
            let running = self.running.clone();
            let started = self.started.clone();
            let last_error = self.last_error.clone();
            let device_open = self.device_open.clone();
            let frames_found = self.frames_found.clone();
            let messages_decoded = self.messages_decoded.clone();
            std::thread::spawn(move || {
                run_worker(
                    &settings,
                    &tracks,
                    &running,
                    &last_error,
                    &device_open,
                    &frames_found,
                    &messages_decoded,
                );
                running.store(false, Ordering::SeqCst);
                device_open.store(false, Ordering::SeqCst);
                started.store(false, Ordering::SeqCst);
            });
        }
    }

    pub fn status(&self) -> UatStatus {
        UatStatus {
            enabled: self.settings.lock().uat_enabled,
            device_open: self.device_open.load(Ordering::SeqCst),
            frames_found: self.frames_found.load(Ordering::SeqCst),
            messages_decoded: self.messages_decoded.load(Ordering::SeqCst),
            aircraft_tracked: self.tracks.lock().len(),
            last_error: self.last_error.lock().clone(),
        }
    }
}

#[async_trait]
impl AircraftSource for UatSource {
    fn name(&self) -> &str {
        NAME
    }

    async fn snapshot(&self, _queries: &[PointQuery]) -> Result<Vec<RawAircraft>> {
        self.ensure_started();
        let now = now_ms();
        let mut tracks = self.tracks.lock();
        tracks.retain(|_, t| now - t.last_seen_ms < STALE_MS);
        let out: Vec<RawAircraft> = tracks.values().map(|t| t.to_raw(now)).collect();
        if out.is_empty() {
            if let Some(e) = self.last_error.lock().as_ref() {
                return Err(anyhow::anyhow!("{e}"));
            }
        }
        Ok(out)
    }
}

#[allow(clippy::too_many_arguments)]
fn run_worker(
    settings: &Arc<Mutex<AppSettings>>,
    tracks: &Arc<Mutex<HashMap<String, Track>>>,
    running: &Arc<AtomicBool>,
    last_error: &Arc<Mutex<Option<String>>>,
    device_open: &Arc<AtomicBool>,
    frames_found: &Arc<AtomicU64>,
    messages_decoded: &Arc<AtomicU64>,
) {
    let device_index = settings.lock().uat_device_index;

    let mut sdr = match RtlSdr::open(DeviceId::Index(device_index as usize)) {
        Ok(s) => s,
        Err(e) => {
            *last_error.lock() = Some(format!("couldn't open RTL-SDR #{device_index}: {e}"));
            return;
        }
    };
    if let Err(e) = sdr.set_center_freq(CENTER_FREQ_HZ) {
        *last_error.lock() = Some(format!("set frequency failed: {e}"));
        return;
    }
    if let Err(e) = sdr.set_sample_rate(SAMPLE_RATE_HZ) {
        *last_error.lock() = Some(format!("set sample rate failed: {e}"));
        return;
    }
    if let Err(e) = sdr.set_gain_manual(GAIN_TENTHS_DB) {
        *last_error.lock() = Some(format!("set gain failed: {e}"));
        return;
    }
    let reader = match sdr.start_streaming() {
        Ok(r) => r,
        Err(e) => {
            *last_error.lock() = Some(format!("start streaming failed: {e}"));
            return;
        }
    };
    *last_error.lock() = None;
    device_open.store(true, Ordering::SeqCst);

    let short_code = RsCode::new(0x87, 120, 1, 12);
    let long_code = RsCode::new(0x87, 120, 1, 14);

    // Carries unconsumed IQ bytes across read boundaries, same pattern as
    // `ingest::rtlsdr`.
    let mut tail: Vec<u8> = Vec::new();

    while running.load(Ordering::SeqCst) && settings.lock().uat_enabled {
        let Some(iq) = reader.recv() else {
            *last_error.lock() = Some("RTL-SDR stream ended unexpectedly".into());
            return;
        };
        tail.extend_from_slice(&iq);

        let (candidates, consumed) = demod::find_frames(&tail);
        if !candidates.is_empty() {
            frames_found.fetch_add(candidates.len() as u64, Ordering::Relaxed);
            let now = now_ms();
            for c in candidates {
                if let Some(payload) = decode_candidate(&short_code, &long_code, c.bytes) {
                    if let Some(msg) = frame::parse(&payload) {
                        messages_decoded.fetch_add(1, Ordering::Relaxed);
                        let mut t = tracks.lock();
                        let track =
                            t.entry(msg.icao.clone()).or_insert_with(|| Track::new(msg.icao.clone(), now));
                        track.apply(msg, now);
                    }
                }
            }
        }
        tail.drain(0..consumed.min(tail.len()));
        // A preamble-plus-long-frame's worth of trailing bytes, in case a
        // sync word starts right at the end of this chunk (2 bytes/sample,
        // 2 samples/bit).
        const MAX_TAIL: usize = (36 + demod::LONG_FRAME_BYTES as u32 as usize * 8) * 2 * 2;
        if tail.len() > MAX_TAIL {
            let drop = tail.len() - MAX_TAIL;
            tail.drain(0..drop);
        }
    }
}

/// Try the short-message code first (fewer bits, so a genuinely short
/// message doesn't need to also happen to pass the long code against
/// leftover/irrelevant trailing bytes), falling back to long.
fn decode_candidate(
    short_code: &RsCode,
    long_code: &RsCode,
    mut bytes: [u8; demod::LONG_FRAME_BYTES],
) -> Option<Vec<u8>> {
    let mut short_buf = bytes[..SHORT_TOTAL_LEN].to_vec();
    if short_code.decode(&mut short_buf).is_some() {
        return Some(short_buf[..SHORT_DATA_LEN].to_vec());
    }
    if long_code.decode(&mut bytes).is_some() {
        return Some(bytes[..LONG_DATA_LEN].to_vec());
    }
    None
}
