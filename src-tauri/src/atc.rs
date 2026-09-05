//! ATC voice audio: tunes the RTL-SDR to a VHF airband frequency (118-137
//! MHz AM, e.g. a tower/ground/approach frequency from an airport's info
//! panel) instead of 1090ES ADS-B, demodulates AM by envelope detection —
//! reusing the same magnitude computation `ingest::rtlsdr::demod` uses for
//! ADS-B, since AM envelope detection and Mode S pulse-amplitude detection
//! are the same math — and plays the result out the default audio device.
//! Can also scan a list of frequencies (parking on whichever one currently
//! has a transmission) and record the session to a WAV file.
//!
//! A single RTL-SDR can only tune to one frequency at a time. If the
//! device index chosen for ATC is the *same* one the ADS-B decoder is
//! configured to use, starting a session pauses ADS-B decoding first (flips
//! `rtlsdr_enabled` off in the in-memory settings only — deliberately not
//! persisted, so a crash mid-session can't leave the user's real
//! preference stuck off) and restores it when listening stops. If it's a
//! *different* device index (two dongles), no coordination happens at all;
//! both simply run.

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use rs_rtl::{DeviceId, RtlSdr};
use serde::Serialize;

use crate::config::AppSettings;
use crate::ingest::rtlsdr::demod;
use crate::ingest::RtlSdrSource;

/// Capture rate — comfortably wide for one ~25kHz airband channel plus
/// margin, and inside RTL-SDR's low-rate valid range (225,001-300,000 Hz;
/// the 1090ES decoder uses the *other* valid range, 900k-3.2M, since it
/// needs far more bandwidth for pulse timing).
const CAPTURE_HZ: u32 = 240_000;
/// Single-pole DC-blocker time constant on the AM envelope — removes the
/// carrier's average level (a large DC-ish offset) so what's left is just
/// the voice waveform. Standard `y[n] = x[n] - x[n-1] + R*y[n-1]` blocker.
const DC_BLOCK_R: f64 = 0.995;
/// Roughly converts the demodulated envelope's AC swing into a usable i16
/// PCM range — unverified against real hardware/headphones, since that
/// needs a human listening; treat as a starting point to retune by ear.
const AUDIO_GAIN: f64 = 120.0;
/// Squelch opens when the short-term signal level exceeds the adaptive
/// noise-floor estimate by this ratio, plus a flat margin (see `run_worker`).
const SQUELCH_RATIO: f64 = 3.2;
/// Fixed manual gain for ATC voice, in tenths of dB — the same lesson
/// learned the hard way for ADS-B: RTL2832U's hardware AGC
/// (`set_gain_auto`) proved too weak/inconsistent to reliably pull a real
/// signal above the noise floor, so this fixes a reasonably high gain
/// instead of relying on it. Unverified against real hardware.
const ATC_GAIN_TENTHS_DB: i32 = 400; // 40.0 dB
/// Scan mode: minimum time to stay on a frequency before considering moving
/// on, even if it's silent — avoids retuning so fast between channels that
/// a transmission starting right after we leave gets missed indefinitely.
const SCAN_MIN_DWELL: Duration = Duration::from_millis(1000);
/// Scan mode: how long a channel has to stay *quiet* after a transmission
/// (or from the start) before we move to the next one — long enough that a
/// brief gap mid-transmission (a pause between sentences) doesn't bounce us
/// off the channel.
const SCAN_HANG_TIME: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AtcStatus {
    pub running: bool,
    pub device_open: bool,
    pub tuned_mhz: Option<f64>,
    /// True while listening to more than one frequency — `tuned_mhz` is
    /// whichever one the scan is currently parked on.
    pub scanning: bool,
    /// True for the brief (roughly 1-3s) gap while a scan hop is closing
    /// and reopening the device — see the comment on `open_and_stream` for
    /// why a scan can't just retune the still-open device.
    pub retuning: bool,
    /// True while a transmission (not just noise) is being heard — same
    /// idea as a hardware scanner's squelch light.
    pub squelch_open: bool,
    /// Set while this session has ADS-B decoding paused on a shared dongle.
    pub adsb_paused: bool,
    pub recording: bool,
    pub last_error: Option<String>,
}

type WavWriter = hound::WavWriter<BufWriter<File>>;

pub struct AtcListener {
    settings: Arc<Mutex<AppSettings>>,
    rtlsdr_source: Arc<RtlSdrSource>,
    running: Arc<AtomicBool>,
    device_open: Arc<AtomicBool>,
    scanning: Arc<AtomicBool>,
    retuning: Arc<AtomicBool>,
    squelch_open: Arc<AtomicBool>,
    adsb_paused: Arc<AtomicBool>,
    /// kHz, so it fits in an atomic without needing a float; 0 = not tuned.
    tuned_khz: Arc<AtomicU32>,
    /// Set by the worker once it knows the audio device's real sample rate
    /// — needed to give a recording's WAV header the right rate. 0 until a
    /// session has actually opened the audio device.
    out_rate: Arc<AtomicU32>,
    recording: Arc<Mutex<Option<WavWriter>>>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl AtcListener {
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
            out_rate: Arc::new(AtomicU32::new(0)),
            recording: Arc::new(Mutex::new(None)),
            last_error: Arc::new(Mutex::new(None)),
        }
    }

    pub fn status(&self) -> AtcStatus {
        let khz = self.tuned_khz.load(Ordering::SeqCst);
        AtcStatus {
            running: self.running.load(Ordering::SeqCst),
            device_open: self.device_open.load(Ordering::SeqCst),
            tuned_mhz: (khz > 0).then(|| khz as f64 / 1000.0),
            scanning: self.scanning.load(Ordering::SeqCst),
            retuning: self.retuning.load(Ordering::SeqCst),
            squelch_open: self.squelch_open.load(Ordering::SeqCst),
            adsb_paused: self.adsb_paused.load(Ordering::SeqCst),
            recording: self.recording.lock().is_some(),
            last_error: self.last_error.lock().clone(),
        }
    }

    /// Stop listening (if running) and wait for the device to actually be
    /// released before returning. `tune()`/`scan()` always call this first
    /// too, so there's never a moment where two worker threads could
    /// contend for the same RTL-SDR device index.
    pub async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        for _ in 0..50 {
            if !self.device_open.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        self.finish_recording();
        self.tuned_khz.store(0, Ordering::SeqCst);
        self.out_rate.store(0, Ordering::SeqCst);
        self.scanning.store(false, Ordering::SeqCst);
        self.retuning.store(false, Ordering::SeqCst);
        self.squelch_open.store(false, Ordering::SeqCst);
        // Also doubles as "Dismiss" on a failed-tune error banner (see
        // StatusBar.svelte) — both Stop and Dismiss call this same command.
        *self.last_error.lock() = None;
        if self.adsb_paused.swap(false, Ordering::SeqCst) {
            self.settings.lock().rtlsdr_enabled = true;
        }
    }

    pub async fn tune(&self, mhz: f64, device_index: u32) -> Result<()> {
        self.start(vec![mhz], device_index).await
    }

    /// Cycle through `freqs`, parking on whichever one currently has a
    /// transmission — see `SCAN_MIN_DWELL`/`SCAN_HANG_TIME`.
    pub async fn scan(&self, freqs: Vec<f64>, device_index: u32) -> Result<()> {
        if freqs.is_empty() {
            return Err(anyhow!("no frequencies to scan"));
        }
        self.start(freqs, device_index).await
    }

    async fn start(&self, freqs: Vec<f64>, device_index: u32) -> Result<()> {
        for &mhz in &freqs {
            if !(108.0..=140.0).contains(&mhz) {
                return Err(anyhow!("{mhz} MHz is outside the VHF airband range"));
            }
        }
        self.stop().await;

        // If ADS-B is configured to use the same physical device, pause it
        // and wait for it to actually let go before we try to open it.
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
        let out_rate = self.out_rate.clone();
        let recording = self.recording.clone();
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
                &out_rate,
                &recording,
                &last_error,
            );
            running.store(false, Ordering::SeqCst);
            device_open.store(false, Ordering::SeqCst);
        });
        Ok(())
    }

    /// Start recording the current session to `path` — errors if nothing is
    /// currently playing (recording only makes sense alongside a live
    /// session, and the WAV header needs the audio device's real sample
    /// rate, only known once one is open).
    pub fn start_recording(&self, path: PathBuf) -> Result<PathBuf> {
        if !self.device_open.load(Ordering::SeqCst) {
            return Err(anyhow!("not currently listening"));
        }
        let rate = self.out_rate.load(Ordering::SeqCst);
        if rate == 0 {
            return Err(anyhow!("audio device not ready yet — try again in a moment"));
        }
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer = hound::WavWriter::create(&path, spec)
            .map_err(|e| anyhow!("couldn't create recording file: {e}"))?;
        *self.recording.lock() = Some(writer);
        Ok(path)
    }

    pub fn stop_recording(&self) {
        self.finish_recording();
    }

    fn finish_recording(&self) {
        if let Some(w) = self.recording.lock().take() {
            let _ = w.finalize();
        }
    }
}

/// Runs on its own OS thread until `running` goes false or the device/audio
/// stream fails. Blocking device + audio I/O doesn't fit an async task.
#[allow(clippy::too_many_arguments)]
fn run_worker(
    freqs: Vec<f64>,
    device_index: u32,
    running: &Arc<AtomicBool>,
    device_open: &Arc<AtomicBool>,
    retuning: &Arc<AtomicBool>,
    squelch_open: &Arc<AtomicBool>,
    tuned_khz: &Arc<AtomicU32>,
    out_rate_shared: &Arc<AtomicU32>,
    recording: &Arc<Mutex<Option<WavWriter>>>,
    last_error: &Arc<Mutex<Option<String>>>,
) {
    let mut rtl = match open_and_stream(device_index, (freqs[0] * 1_000_000.0).round() as u32) {
        Ok(r) => r,
        Err(e) => {
            *last_error.lock() = Some(format!("couldn't open RTL-SDR #{device_index}: {e}"));
            return;
        }
    };
    let actual_rate = rtl.actual_rate;

    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        *last_error.lock() = Some("no audio output device found".into());
        return;
    };
    let out_config = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            *last_error.lock() = Some(format!("no usable audio output config: {e}"));
            return;
        }
    };
    let out_channels = out_config.channels() as usize;
    let out_rate = out_config.sample_rate().0;
    let sample_format = out_config.sample_format();
    let config: cpal::StreamConfig = out_config.into();
    out_rate_shared.store(out_rate, Ordering::SeqCst);

    // How many envelope samples (at CAPTURE_HZ) to average into one output
    // audio sample — a plain boxcar low-pass + decimate. Coarse compared to
    // a real filter, but airband voice is ~3kHz wide and this decimates by
    // several hundred, leaving plenty of margin against aliasing artifacts
    // being audible as more than mild background roughness.
    let decim = ((actual_rate as f64 / out_rate as f64).round() as usize).max(1);

    let (tx, rx) = std::sync::mpsc::sync_channel::<i16>(out_rate as usize * 2);
    let err_fn = |e: cpal::StreamError| tracing::warn!("atc: audio stream error: {e}");
    let stream_result = match sample_format {
        cpal::SampleFormat::I16 => device.build_output_stream(
            &config,
            move |data: &mut [i16], _| fill_audio(data, out_channels, &rx, |s| s),
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_output_stream(
            &config,
            move |data: &mut [u16], _| {
                fill_audio(data, out_channels, &rx, |s| (s as i32 + 32768) as u16)
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config,
            move |data: &mut [f32], _| {
                fill_audio(data, out_channels, &rx, |s| s as f32 / 32768.0)
            },
            err_fn,
            None,
        ),
        other => {
            *last_error.lock() = Some(format!("unsupported audio sample format: {other:?}"));
            return;
        }
    };
    let stream = match stream_result {
        Ok(s) => s,
        Err(e) => {
            *last_error.lock() = Some(format!("couldn't open audio output: {e}"));
            return;
        }
    };
    if let Err(e) = stream.play() {
        *last_error.lock() = Some(format!("couldn't start audio playback: {e}"));
        return;
    }

    *last_error.lock() = None;
    device_open.store(true, Ordering::SeqCst);

    // AM envelope's DC-blocker state and adaptive noise-floor estimate —
    // local to this thread; only the derived `squelch_open` bool (and, in
    // scan mode, which channel we're on) is shared.
    let mut dc_prev_in = 0.0_f64;
    let mut dc_prev_out = 0.0_f64;
    let mut noise_floor = 40.0_f64;
    // Smoothed signal level used only for the squelch decision (kept
    // separate from the raw envelope so a single noisy chunk can't flip
    // squelch open on its own the way comparing the floor to a jumpy
    // sample-by-sample value could).
    let mut level = 40.0_f64;

    // Scan-mode state — irrelevant (never advances) when `freqs.len() == 1`.
    let mut freq_idx = 0usize;
    let mut dwell_start = Instant::now();
    let mut quiet_since: Option<Instant> = Some(Instant::now());

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

            // Track the floor slowly and roughly symmetrically, toward the
            // *typical* quiet level rather than hugging the noise's own
            // instantaneous minimum (see the module-level constant docs).
            if level < noise_floor {
                noise_floor += (level - noise_floor) * 0.02;
            } else {
                noise_floor += (level - noise_floor) * 0.005;
            }
            let open = level > noise_floor * SQUELCH_RATIO + 8.0;
            squelch_open.store(open, Ordering::Relaxed);

            if freqs.len() > 1 {
                if open {
                    quiet_since = None;
                } else if quiet_since.is_none() {
                    quiet_since = Some(Instant::now());
                }
                let ready_to_move = dwell_start.elapsed() >= SCAN_MIN_DWELL
                    && quiet_since.is_some_and(|t| t.elapsed() >= SCAN_HANG_TIME);
                if ready_to_move {
                    freq_idx = (freq_idx + 1) % freqs.len();
                    // Retuning the still-open device — whether through
                    // rs-rtl's "retune during streaming" control channel
                    // (used the same way by its own author's reference app)
                    // or by dropping just the streaming handle and calling
                    // set_center_freq again — reliably stalls the tuner's
                    // vendor control endpoint on this hardware/driver
                    // combination. The only sequence that's worked so far
                    // is the exact one a cold start uses, so a retune here
                    // fully closes the device (dropping `rtl`, which puts
                    // the tuner in standby via RtlSdr's Drop impl) and
                    // reopens it from scratch — slower per hop, but this is
                    // the one path that's actually held up.
                    retuning.store(true, Ordering::SeqCst);
                    drop(rtl);
                    let opened =
                        open_and_stream(device_index, (freqs[freq_idx] * 1_000_000.0).round() as u32);
                    retuning.store(false, Ordering::SeqCst);
                    match opened {
                        Ok(r) => rtl = r,
                        Err(e) => {
                            *last_error.lock() = Some(format!("retune failed: {e}"));
                            break 'read;
                        }
                    }
                    tuned_khz.store((freqs[freq_idx] * 1000.0).round() as u32, Ordering::SeqCst);
                    dwell_start = Instant::now();
                    quiet_since = Some(Instant::now());
                    // Per-channel state — a floor/DC-block estimate carried
                    // over from a completely different frequency isn't
                    // meaningful on the new one.
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

            let sample = if open {
                (y * AUDIO_GAIN).clamp(-32760.0, 32760.0) as i16
            } else {
                0
            };
            if let Some(w) = recording.lock().as_mut() {
                let _ = w.write_sample(sample);
            }
            // A full buffer means the audio thread has stalled or the
            // device is gone; either way there's nothing useful to do but
            // drop samples rather than block the RTL-SDR read loop.
            let _ = tx.try_send(sample);
        }
    }
    drop(stream);
}

/// An open, streaming RTL-SDR session — bundled together because both
/// halves get torn down and recreated together on every retune (see
/// `open_and_stream`).
struct RtlSession {
    /// Kept alive alongside `reader`: dropping it puts the tuner in standby
    /// (see `RtlSdr`'s `Drop` impl) and lets the device be reopened cleanly.
    #[allow(dead_code)]
    sdr: RtlSdr,
    reader: rs_rtl::AsyncReadHandle,
    actual_rate: u32,
}

/// Open the device fresh, configure it, and start streaming — the exact
/// sequence a cold start uses. Retuning *without* a full close/reopen
/// (either through rs-rtl's runtime retune-during-streaming control channel,
/// or dropping just the streaming handle and calling `set_center_freq`
/// again on the still-open device) reliably stalls the tuner's vendor
/// control endpoint on at least some hardware/driver combinations, so scan
/// mode calls this on every channel hop instead — slower per hop, but it's
/// the one sequence that's actually held up. Retries with a short settle
/// delay in case the device needs a moment after a previous session's
/// bulk transfers are torn down before it's ready to be reopened.
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
            sdr.set_gain_manual(ATC_GAIN_TENTHS_DB)?;
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

/// Shared cpal callback body for whichever sample format the output device
/// wants — `conv` maps our internal i16 PCM to that format.
fn fill_audio<T: Copy>(
    data: &mut [T],
    channels: usize,
    rx: &std::sync::mpsc::Receiver<i16>,
    conv: impl Fn(i16) -> T,
) {
    for frame in data.chunks_mut(channels.max(1)) {
        let s = conv(rx.try_recv().unwrap_or(0));
        for out in frame {
            *out = s;
        }
    }
}
