//! Direct RTL-SDR dongle support — decodes 1090ES ADS-B ourselves instead of
//! depending on an external dump1090/readsb process (that's `super::local`
//! for anyone who already has one running). Device I/O is `rs_rtl` (MIT,
//! pure-Rust, no libusb/C dependency); `demod` (this crate, from-spec) turns
//! raw IQ into candidate Mode S byte messages; `adsb_deku` (MIT) parses and
//! validates them.
//!
//! Runs a dedicated OS thread (the device's blocking streaming API doesn't
//! fit the async poll-on-demand shape the other sources use) that owns the
//! dongle for as long as `AppSettings.rtlsdr_enabled` stays on, publishing
//! decoded aircraft into a shared map that `snapshot()` just reads.

pub mod demod;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use adsb_deku::adsb::{EmergencyState, TypeCoding, ME};
use adsb_deku::{cpr, Altitude, CPRFormat, Frame, DF};
use anyhow::Result;
use async_trait::async_trait;
use parking_lot::Mutex;
use rs_rtl::{DeviceId, RtlSdr};

use super::{AircraftSource, PointQuery};
use crate::config::AppSettings;
use crate::ingest::model::{AltBaro, RawAircraft};
use crate::util::now_ms;

pub const NAME: &str = "rtl-sdr";

/// 1090ES is a fixed frequency; only the sample rate is a choice, and 2 Msps
/// (vs. the fractional 2.4 Msps some tools use) keeps PPM bit timing on
/// exact sample boundaries — see `demod`.
const CENTER_FREQ_HZ: u32 = 1_090_000_000;
const SAMPLE_RATE_HZ: u32 = 2_000_000;
/// Global CPR position needs one even + one odd frame within this window
/// (ICAO Annex 10 Vol IV airborne global decode).
const CPR_PAIR_WINDOW_MS: i64 = 10_000;
/// Drop a track we haven't heard from in this long.
const STALE_MS: i64 = 60_000;

struct Track {
    hex: String,
    callsign: Option<String>,
    category: Option<String>,
    alt_baro: Option<f64>,
    lat: Option<f64>,
    lon: Option<f64>,
    ground_speed: Option<f64>,
    track_deg: Option<f64>,
    baro_rate: Option<f64>,
    squawk: Option<String>,
    emergency: Option<String>,
    even: Option<(Altitude, i64)>,
    odd: Option<(Altitude, i64)>,
    last_seen_ms: i64,
    last_pos_ms: Option<i64>,
}

impl Track {
    fn new(hex: String, now: i64) -> Self {
        Self {
            hex,
            callsign: None,
            category: None,
            alt_baro: None,
            lat: None,
            lon: None,
            ground_speed: None,
            track_deg: None,
            baro_rate: None,
            squawk: None,
            emergency: None,
            even: None,
            odd: None,
            last_seen_ms: now,
            last_pos_ms: None,
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
            category: self.category.clone(),
            alt_baro: self.alt_baro.map(AltBaro::Num),
            alt_geom: None,
            gs: self.ground_speed,
            ias: None,
            tas: None,
            mach: None,
            track: self.track_deg,
            mag_heading: None,
            true_heading: None,
            baro_rate: self.baro_rate,
            geom_rate: None,
            squawk: self.squawk.clone(),
            emergency: self.emergency.clone(),
            nav_altitude_mcp: None,
            nav_altitude_fms: None,
            nav_heading: None,
            nav_qnh: None,
            lat: self.lat,
            lon: self.lon,
            rssi: None,
            messages: None,
            seen: Some((now - self.last_seen_ms).max(0) as f64 / 1000.0),
            seen_pos: self
                .last_pos_ms
                .map(|t| (now - t).max(0) as f64 / 1000.0),
            mlat: Vec::new(),
            tisb: Vec::new(),
            db_flags: None,
        }
    }
}

/// ICAO Annex 4 wake-vortex category letter+number, e.g. "A3" — matches the
/// convention the rest of the app (and the community feeds) already use.
fn category_string(tc: TypeCoding, ca: u8) -> String {
    format!("{tc}{ca}")
}

/// Snapshot of the worker thread's real-world progress, for the Settings
/// panel — distinguishes "enabled, no error yet" (which can be true the
/// instant you flip the toggle) from "the device is actually open and
/// decoding real messages."
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RtlSdrStatus {
    pub enabled: bool,
    pub device_open: bool,
    pub messages_decoded: u64,
    /// Preamble-shaped candidates found before CRC validation — a temporary
    /// diagnostic. If this stays near zero, the antenna/gain/RF chain isn't
    /// getting signal at all; if it's high but `messages_decoded` isn't,
    /// see `frames_parsed`/`adsb_frames` to narrow down where they're lost.
    pub raw_candidates: u64,
    /// Of `raw_candidates`, how many were even structurally valid Mode S
    /// (right length, `adsb_deku` could parse a DF out of them) — temporary
    /// diagnostic. Low relative to `raw_candidates` means most "candidates"
    /// are noise the preamble detector is too loose about, not real bursts.
    pub frames_parsed: u64,
    /// Of `frames_parsed`, how many were DF17/18 (ADS-B) rather than some
    /// other Mode S downlink format (radar interrogation replies etc., which
    /// are real and common but never become `messages_decoded`) — temporary
    /// diagnostic. Non-zero here but `messages_decoded` still zero means
    /// real ES squitters are arriving but failing CRC — a demod/CRC bug.
    pub adsb_frames: u64,
    pub aircraft_tracked: usize,
    pub last_error: Option<String>,
}

pub struct RtlSdrSource {
    settings: Arc<Mutex<AppSettings>>,
    tracks: Arc<Mutex<HashMap<String, Track>>>,
    running: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
    device_open: Arc<AtomicBool>,
    messages_decoded: Arc<AtomicU64>,
    raw_candidates: Arc<AtomicU64>,
    frames_parsed: Arc<AtomicU64>,
    adsb_frames: Arc<AtomicU64>,
}

impl RtlSdrSource {
    pub fn new(settings: Arc<Mutex<AppSettings>>) -> Self {
        Self {
            settings,
            tracks: Arc::new(Mutex::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            started: Arc::new(AtomicBool::new(false)),
            last_error: Arc::new(Mutex::new(None)),
            device_open: Arc::new(AtomicBool::new(false)),
            messages_decoded: Arc::new(AtomicU64::new(0)),
            raw_candidates: Arc::new(AtomicU64::new(0)),
            frames_parsed: Arc::new(AtomicU64::new(0)),
            adsb_frames: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Spawn the worker if it should be running but isn't. `compare_exchange`
    /// makes the check-and-flip atomic so two near-simultaneous callers can't
    /// both spawn a thread for the same device.
    fn ensure_started(&self) {
        let enabled = self.settings.lock().rtlsdr_enabled;
        if enabled
            && self
                .started
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            self.running.store(true, Ordering::SeqCst);
            spawn_worker(
                self.settings.clone(),
                self.tracks.clone(),
                self.running.clone(),
                self.started.clone(),
                self.last_error.clone(),
                self.device_open.clone(),
                self.messages_decoded.clone(),
                self.raw_candidates.clone(),
                self.frames_parsed.clone(),
                self.adsb_frames.clone(),
            );
        }
    }

    pub fn status(&self) -> RtlSdrStatus {
        RtlSdrStatus {
            enabled: self.settings.lock().rtlsdr_enabled,
            device_open: self.device_open.load(Ordering::SeqCst),
            messages_decoded: self.messages_decoded.load(Ordering::SeqCst),
            raw_candidates: self.raw_candidates.load(Ordering::SeqCst),
            frames_parsed: self.frames_parsed.load(Ordering::SeqCst),
            adsb_frames: self.adsb_frames.load(Ordering::SeqCst),
            aircraft_tracked: self.tracks.lock().len(),
            last_error: self.last_error.lock().clone(),
        }
    }
}

#[async_trait]
impl AircraftSource for RtlSdrSource {
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

/// Runs on its own OS thread for as long as `rtlsdr_enabled` stays on;
/// blocking device I/O doesn't fit the async poll model the other sources
/// use. All exit paths (device error, stream end, or noticing the setting
/// flipped off) fall through to one cleanup point, so `running`/`started`/
/// `device_open` always end up consistent — re-enabling later spawns a
/// fresh thread via `ensure_started` on the next poll.
fn spawn_worker(
    settings: Arc<Mutex<AppSettings>>,
    tracks: Arc<Mutex<HashMap<String, Track>>>,
    running: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
    device_open: Arc<AtomicBool>,
    messages_decoded: Arc<AtomicU64>,
    raw_candidates: Arc<AtomicU64>,
    frames_parsed: Arc<AtomicU64>,
    adsb_frames: Arc<AtomicU64>,
) {
    std::thread::spawn(move || {
        run_worker(
            &settings,
            &tracks,
            &running,
            &last_error,
            &device_open,
            &messages_decoded,
            &raw_candidates,
            &frames_parsed,
            &adsb_frames,
        );
        running.store(false, Ordering::SeqCst);
        device_open.store(false, Ordering::SeqCst);
        started.store(false, Ordering::SeqCst);
    });
}

/// Returns once the device fails to open/configure, the stream ends, or
/// `running`/`rtlsdr_enabled` says to stop — see `spawn_worker` for cleanup.
fn run_worker(
    settings: &Arc<Mutex<AppSettings>>,
    tracks: &Arc<Mutex<HashMap<String, Track>>>,
    running: &Arc<AtomicBool>,
    last_error: &Arc<Mutex<Option<String>>>,
    device_open: &Arc<AtomicBool>,
    messages_decoded: &Arc<AtomicU64>,
    raw_candidates: &Arc<AtomicU64>,
    frames_parsed: &Arc<AtomicU64>,
    adsb_frames: &Arc<AtomicU64>,
) {
    let (device_index, gain_tenths_db) = {
        let s = settings.lock();
        (s.rtlsdr_device_index, s.rtlsdr_gain_tenths_db)
    };

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
    let gain_result = match gain_tenths_db {
        Some(g) => sdr.set_gain_manual(g),
        None => sdr.set_gain_auto(),
    };
    if let Err(e) = gain_result {
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

    // Carries unconsumed magnitude samples across read boundaries so a
    // message split across two USB transfers isn't lost.
    let mut tail: Vec<u16> = Vec::new();

    while running.load(Ordering::SeqCst) && settings.lock().rtlsdr_enabled {
        let Some(iq) = reader.recv() else {
            *last_error.lock() = Some("RTL-SDR stream ended unexpectedly".into());
            return;
        };
        tail.extend(demod::magnitude(&iq));

        let (candidates, consumed) = demod::demod(&tail);
        if !candidates.is_empty() {
            raw_candidates.fetch_add(candidates.len() as u64, Ordering::Relaxed);
            let now = now_ms();
            let mut t = tracks.lock();
            for c in candidates {
                if let Ok(frame) = Frame::from_bytes(&c.bytes) {
                    frames_parsed.fetch_add(1, Ordering::Relaxed);
                    if matches!(&frame.df, DF::ADSB(_)) {
                        adsb_frames.fetch_add(1, Ordering::Relaxed);
                    }
                    if apply_frame(&mut t, frame, now) {
                        messages_decoded.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
        // Keep a preamble+long-message-worth of trailing samples in case a
        // message starts right at the end of this chunk.
        let keep_from = consumed.min(tail.len());
        tail.drain(0..keep_from);
        const MAX_TAIL: usize = 16 + 112 * 2 * 2;
        if tail.len() > MAX_TAIL {
            let drop = tail.len() - MAX_TAIL;
            tail.drain(0..drop);
        }
    }
}

/// Applies a parsed frame to its aircraft's track. Returns whether it was a
/// genuine CRC-valid ADS-B message (vs. our demod locking onto noise, or a
/// non-ADS-B DF we don't decode) — used to drive the "messages decoded"
/// counter so it means something.
fn apply_frame(tracks: &mut HashMap<String, Track>, frame: Frame, now: i64) -> bool {
    let DF::ADSB(adsb) = frame.df else { return false };
    // Unlike overlay-format DFs (DF0/4/5/11/16/20/21/24), where the ICAO
    // address is recovered by XOR-ing the parity field with a CRC computed
    // over the data — and where adsb_deku's `frame.crc` *is* that recovered
    // address — DF17/18 carry the ICAO explicitly in the AA field, and the
    // trailing parity is a plain CRC-24 remainder over the whole message.
    // adsb_deku computes that remainder as `frame.crc` too, but for this DF
    // it's valid iff the remainder is zero, not iff it equals the ICAO.
    // (Comparing it to the ICAO here — the previous version of this check —
    // meant every genuine DF17/18 message was rejected as noise, since a
    // real message's `crc` is 0, never the aircraft's address.)
    if frame.crc != 0 {
        return false;
    }
    let icao = adsb.icao.0;
    let icao_val = u32::from_be_bytes([0, icao[0], icao[1], icao[2]]);
    let hex = format!("{icao_val:06x}");
    let t = tracks
        .entry(hex.clone())
        .or_insert_with(|| Track::new(hex, now));
    t.last_seen_ms = now;

    match adsb.me {
        ME::AirbornePositionBaroAltitude { altitude, .. }
        | ME::AirbornePositionGNSSAltitude { altitude, .. } => {
            if let Some(alt) = altitude.alt {
                t.alt_baro = Some(f64::from(alt));
            }
            match altitude.odd_flag {
                CPRFormat::Even => t.even = Some((altitude, now)),
                CPRFormat::Odd => t.odd = Some((altitude, now)),
            }
            if let (Some((e, et)), Some((o, ot))) = (&t.even, &t.odd) {
                if (et - ot).abs() <= CPR_PAIR_WINDOW_MS {
                    if let Some(pos) = cpr::get_position((e, o)) {
                        t.lat = Some(pos.latitude);
                        t.lon = Some(pos.longitude);
                        t.last_pos_ms = Some(now);
                    }
                }
            }
        }
        ME::AirborneVelocity(v) => {
            if let Some((heading, gs, vrate)) = v.calculate() {
                t.track_deg = Some(f64::from(heading));
                t.ground_speed = Some(gs);
                t.baro_rate = Some(f64::from(vrate));
            }
        }
        ME::AircraftIdentification { identification, .. } => {
            let cn = identification.cn.trim().to_string();
            if !cn.is_empty() {
                t.callsign = Some(cn);
            }
            t.category = Some(category_string(identification.tc, identification.ca));
        }
        ME::AircraftStatus(status) => {
            t.emergency = match status.emergency_state {
                EmergencyState::None => None,
                other => Some(format!("{other}").replace(' ', "_")),
            };
            t.squawk = Some(format!("{:04o}", status.squawk));
        }
        _ => {}
    }
    true
}

/// Probe for connected dongles (Settings "detect device" button).
pub fn list_devices() -> Result<Vec<String>> {
    let descriptors = rs_rtl::DeviceDescriptors::new()?;
    Ok(descriptors
        .iter()
        .map(|d| {
            format!(
                "#{}: {} {}",
                d.index,
                d.manufacturer.as_deref().unwrap_or("?"),
                d.product.as_deref().unwrap_or("?"),
            )
        })
        .collect())
}
