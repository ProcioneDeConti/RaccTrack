//! ACARS decoding via RTL-SDR. Reuses the same AM envelope detector as
//! `atc.rs` (`ingest::rtlsdr::demod::magnitude`) and copies its single-dongle
//! tune/retune/device-sharing skeleton (open with retries, pause ADS-B if
//! sharing a device, full close+reopen on every channel hop rather than a
//! live retune — see the long comment on `atc::open_and_stream` for why).
//! That skeleton is duplicated here rather than factored out into something
//! shared: it's already working, hardware-verified code, and after the tune
//! point the two diverge completely (audio playback + WAV recording there,
//! bit demodulation here) — extracting a shared abstraction now would mean
//! touching the fragile, already-tuned ATC code for no benefit to either
//! side.
//!
//! `msk` turns the demodulated audio into bits; `frame` turns bits into
//! ARINC 618 message fields. This module's own job is just the RF/burst
//! layer: watch the same kind of squelch `atc.rs` uses for voice, but here a
//! burst is captured into a buffer instead of played, and handed to
//! `frame::decode_burst` once it goes quiet.
//!
//! Decoded messages accumulate in a capped ring buffer read by the frontend
//! on demand (`messages()`) — the same "poll, don't push" shape `AtcStatus`
//! already uses for worker-thread state.

mod frame;
mod msk;

pub use frame::AcarsMessage;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use rs_rtl::{DeviceId, RtlSdr};
use serde::Serialize;

use crate::config::AppSettings;
use crate::ingest::rtlsdr::demod;
use crate::ingest::RtlSdrSource;
use crate::util::now_ms;

const CAPTURE_HZ: u32 = 240_000;
const DC_BLOCK_R: f64 = 0.995;
/// Same fixed-manual-gain lesson as `atc.rs`: the RTL2832U's hardware AGC
/// is too weak/inconsistent to rely on.
const GAIN_TENTHS_DB: i32 = 400;
/// A data burst is still just "louder than the noise floor" — same squelch
/// shape as `atc.rs`'s voice squelch, just a plain fixed ratio since there's
/// no ear listening in to tune by; unverified against real hardware.
const SQUELCH_RATIO: f64 = 3.0;
/// How long a burst has to stay quiet before it's considered finished and
/// handed to the decoder. ACARS bursts run a few hundred ms; this only
/// needs to be long enough that a within-burst envelope dip (this receiver's
/// AM path doesn't give a perfectly constant envelope even for a clean
/// signal) doesn't split one transmission into two truncated ones.
const BURST_HANG: Duration = Duration::from_millis(120);
/// A burst longer than this is noise/garbage, not a real ACARS message —
/// dropped rather than handed to the decoder.
const MAX_BURST_SAMPLES: usize = msk::SAMPLE_HZ as usize * 3;
const MAX_MESSAGES: usize = 500;
/// Scan mode (multiple frequencies): same dwell/hang timing as `atc.rs`'s
/// voice scan.
const SCAN_MIN_DWELL: Duration = Duration::from_millis(1000);
const SCAN_HANG_TIME: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcarsStatus {
    pub running: bool,
    pub device_open: bool,
    pub tuned_mhz: Option<f64>,
    pub scanning: bool,
    pub retuning: bool,
    pub squelch_open: bool,
    pub adsb_paused: bool,
    pub message_count: usize,
    pub last_error: Option<String>,
}

pub struct AcarsListener {
    settings: Arc<Mutex<AppSettings>>,
    rtlsdr_source: Arc<RtlSdrSource>,
    running: Arc<AtomicBool>,
    device_open: Arc<AtomicBool>,
    scanning: Arc<AtomicBool>,
    retuning: Arc<AtomicBool>,
    squelch_open: Arc<AtomicBool>,
    adsb_paused: Arc<AtomicBool>,
    tuned_khz: Arc<AtomicU32>,
    last_error: Arc<Mutex<Option<String>>>,
    messages: Arc<Mutex<VecDeque<AcarsMessage>>>,
}

impl AcarsListener {
    pub fn new(settings: Arc<Mutex<AppSettings>>, rtlsdr_source: Arc<RtlSdrSource>) -> Self {
        Self {
            settings,
            rtlsdr_source,
            running: Arc::new(AtomicBool::new(false)),
            device_open: Arc::new(AtomicBool::new(false)),
            scanning: Arc::new(AtomicBool::new(false)),
            retuning: Arc::new(AtomicBool::new(false)),
            squelch_open: Arc::new(AtomicBool::new(false)),
            adsb_paused: Arc::new(AtomicBool::new(false)),
            tuned_khz: Arc::new(AtomicU32::new(0)),
            last_error: Arc::new(Mutex::new(None)),
            messages: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn status(&self) -> AcarsStatus {
        let khz = self.tuned_khz.load(Ordering::SeqCst);
        AcarsStatus {
            running: self.running.load(Ordering::SeqCst),
            device_open: self.device_open.load(Ordering::SeqCst),
            tuned_mhz: (khz > 0).then(|| khz as f64 / 1000.0),
            scanning: self.scanning.load(Ordering::SeqCst),
            retuning: self.retuning.load(Ordering::SeqCst),
            squelch_open: self.squelch_open.load(Ordering::SeqCst),
            adsb_paused: self.adsb_paused.load(Ordering::SeqCst),
            message_count: self.messages.lock().len(),
            last_error: self.last_error.lock().clone(),
        }
    }

    /// Most-recent-first.
    pub fn messages(&self) -> Vec<AcarsMessage> {
        self.messages.lock().iter().cloned().collect()
    }

    pub fn clear_messages(&self) {
        self.messages.lock().clear();
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
        self.scanning.store(false, Ordering::SeqCst);
        self.retuning.store(false, Ordering::SeqCst);
        self.squelch_open.store(false, Ordering::SeqCst);
        *self.last_error.lock() = None;
        if self.adsb_paused.swap(false, Ordering::SeqCst) {
            self.settings.lock().rtlsdr_enabled = true;
        }
    }

    /// Listen on one or more VHF frequencies (kHz-rounded like `atc.rs`);
    /// more than one means scan/hop mode. Errors if any frequency is outside
    /// the airband, or the list is empty.
    pub async fn start(&self, freqs: Vec<f64>, device_index: u32) -> Result<()> {
        if freqs.is_empty() {
            return Err(anyhow!("no frequencies to listen on"));
        }
        for &mhz in &freqs {
            if !(108.0..=140.0).contains(&mhz) {
                return Err(anyhow!("{mhz} MHz is outside the VHF airband range"));
            }
        }
        self.stop().await;

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

        *self.last_error.lock() = None;
        self.running.store(true, Ordering::SeqCst);
        self.scanning.store(freqs.len() > 1, Ordering::SeqCst);
        self.tuned_khz
            .store((freqs[0] * 1000.0).round() as u32, Ordering::SeqCst);

        let running = self.running.clone();
        let device_open = self.device_open.clone();
        let retuning = self.retuning.clone();
        let squelch_open = self.squelch_open.clone();
        let tuned_khz = self.tuned_khz.clone();
        let messages = self.messages.clone();
        let last_error = self.last_error.clone();
        std::thread::spawn(move || {
            run_worker(
                freqs,
                device_index,
                &running,
                &device_open,
                &retuning,
                &squelch_open,
                &tuned_khz,
                &messages,
                &last_error,
            );
            running.store(false, Ordering::SeqCst);
            device_open.store(false, Ordering::SeqCst);
        });
        Ok(())
    }
}

/// Runs on its own OS thread until `running` goes false or the device
/// fails. Blocking device I/O doesn't fit an async task (same reasoning as
/// `atc::run_worker`).
#[allow(clippy::too_many_arguments)]
fn run_worker(
    freqs: Vec<f64>,
    device_index: u32,
    running: &Arc<AtomicBool>,
    device_open: &Arc<AtomicBool>,
    retuning: &Arc<AtomicBool>,
    squelch_open: &Arc<AtomicBool>,
    tuned_khz: &Arc<AtomicU32>,
    messages: &Arc<Mutex<VecDeque<AcarsMessage>>>,
    last_error: &Arc<Mutex<Option<String>>>,
) {
    let mut rtl = match open_and_stream(device_index, (freqs[0] * 1_000_000.0).round() as u32) {
        Ok(r) => r,
        Err(e) => {
            *last_error.lock() = Some(format!("couldn't open RTL-SDR #{device_index}: {e}"));
            return;
        }
    };
    let mut decim = ((rtl.actual_rate as f64 / msk::SAMPLE_HZ as f64).round() as usize).max(1);

    *last_error.lock() = None;
    device_open.store(true, Ordering::SeqCst);

    let mut dc_prev_in = 0.0_f64;
    let mut dc_prev_out = 0.0_f64;
    let mut noise_floor = 40.0_f64;
    let mut level = 40.0_f64;
    let mut burst: Vec<f64> = Vec::new();
    let mut quiet_since: Option<Instant> = None;

    let mut freq_idx = 0usize;
    let mut dwell_start = Instant::now();
    let mut scan_quiet_since: Option<Instant> = Some(Instant::now());

    let finish_burst = |burst: &mut Vec<f64>, messages: &Arc<Mutex<VecDeque<AcarsMessage>>>, mhz: f64| {
        if let Some(mut msg) = frame::decode_burst(burst) {
            msg.freq_mhz = mhz;
            msg.timestamp_ms = now_ms();
            let mut m = messages.lock();
            m.push_front(msg);
            m.truncate(MAX_MESSAGES);
        }
        burst.clear();
    };

    'read: while running.load(Ordering::SeqCst) {
        let Some(iq) = rtl.reader.recv() else {
            *last_error.lock() = Some("RTL-SDR stream ended unexpectedly".into());
            break;
        };
        let mag = demod::magnitude(&iq);
        for chunk in mag.chunks(decim) {
            if chunk.is_empty() {
                continue;
            }
            let avg = chunk.iter().map(|&v| v as f64).sum::<f64>() / chunk.len() as f64;
            level += (avg - level) * 0.3;
            if level < noise_floor {
                noise_floor += (level - noise_floor) * 0.02;
            } else {
                noise_floor += (level - noise_floor) * 0.005;
            }
            let open = level > noise_floor * SQUELCH_RATIO + 8.0;
            squelch_open.store(open, Ordering::Relaxed);

            if freqs.len() > 1 {
                if open {
                    scan_quiet_since = None;
                } else if scan_quiet_since.is_none() {
                    scan_quiet_since = Some(Instant::now());
                }
                let ready_to_move = dwell_start.elapsed() >= SCAN_MIN_DWELL
                    && scan_quiet_since.is_some_and(|t| t.elapsed() >= SCAN_HANG_TIME);
                if ready_to_move {
                    finish_burst(&mut burst, messages, freqs[freq_idx]);
                    quiet_since = None;
                    freq_idx = (freq_idx + 1) % freqs.len();
                    retuning.store(true, Ordering::SeqCst);
                    drop(rtl);
                    let opened = open_and_stream(
                        device_index,
                        (freqs[freq_idx] * 1_000_000.0).round() as u32,
                    );
                    retuning.store(false, Ordering::SeqCst);
                    match opened {
                        Ok(r) => rtl = r,
                        Err(e) => {
                            *last_error.lock() = Some(format!("retune failed: {e}"));
                            break 'read;
                        }
                    }
                    decim = ((rtl.actual_rate as f64 / msk::SAMPLE_HZ as f64).round() as usize).max(1);
                    tuned_khz.store((freqs[freq_idx] * 1000.0).round() as u32, Ordering::SeqCst);
                    dwell_start = Instant::now();
                    scan_quiet_since = Some(Instant::now());
                    noise_floor = 40.0;
                    level = 40.0;
                    dc_prev_in = 0.0;
                    dc_prev_out = 0.0;
                }
            }

            let x = avg;
            let y = x - dc_prev_in + DC_BLOCK_R * dc_prev_out;
            dc_prev_in = x;
            dc_prev_out = y;

            if open {
                quiet_since = None;
                burst.push(y);
                if burst.len() > MAX_BURST_SAMPLES {
                    burst.clear(); // runaway squelch-open — not a real burst
                }
            } else if !burst.is_empty() {
                if quiet_since.is_none() {
                    quiet_since = Some(Instant::now());
                } else if quiet_since.unwrap().elapsed() >= BURST_HANG {
                    finish_burst(&mut burst, messages, freqs[freq_idx]);
                    quiet_since = None;
                }
            }
        }
    }
    if !burst.is_empty() {
        finish_burst(&mut burst, messages, freqs[freq_idx]);
    }
}

/// An open, streaming RTL-SDR session — see `atc::RtlSession`, this is the
/// same shape, duplicated for the reasons in the module doc.
struct RtlSession {
    #[allow(dead_code)]
    sdr: RtlSdr,
    reader: rs_rtl::AsyncReadHandle,
    actual_rate: u32,
}

/// See `atc::open_and_stream` — identical device-open/retry sequence.
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
