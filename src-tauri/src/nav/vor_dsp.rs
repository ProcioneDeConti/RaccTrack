//! VOR baseband decoding — a from-spec implementation.
//!
//! After AM envelope detection of a VOR carrier (108–118 MHz) the composite
//! baseband carries:
//!   * a 30 Hz AM tone — the *variable* signal (its phase encodes the bearing),
//!   * a 9960 Hz subcarrier FM-modulated at 30 Hz (±480 Hz) — the *reference*,
//!   * a 1020 Hz keyed Morse ident,
//!   * optionally 300–3000 Hz voice.
//!
//! The radial is the phase of the 30 Hz variable tone relative to the 30 Hz
//! tone recovered from the FM subcarrier. (Doppler VORs swap which tone is
//! reference/variable *and* reverse the rotation sense, so a plain receiver —
//! and this code — sees the same result for both types.)
//!
//! Everything here is validated only against the synthetic generator in the
//! tests; real-world SNR, multipath and the exact phase convention are open
//! until it runs against a dongle. The NavPanel shows the decoded radial next
//! to the geometric one so the offset (if any) is visible.

use std::f64::consts::PI;

use crate::nav::geo::wrap360;
use crate::nav::morse;

/// Nominal VOR subcarrier and tone frequencies.
const SUBCARRIER_HZ: f64 = 9960.0;
const TONE_HZ: f64 = 30.0;
/// A fixed correction applied to the raw phase difference. 0 until hardware
/// testing says otherwise (it may also turn out the sign needs flipping —
/// that's `RADIAL_SIGN`).
pub const RADIAL_CALIBRATION_DEG: f64 = 0.0;
pub const RADIAL_SIGN: f64 = 1.0;

#[derive(Debug, Clone, Copy)]
pub struct RadialEstimate {
    /// Radial 0–360, before any user calibration.
    pub radial_deg: f64,
    /// Recovered 30 Hz AM depth on the variable branch (~0.30 on a real VOR).
    pub var_level: f64,
    /// Recovered 30 Hz amplitude from the FM subcarrier (arbitrary units).
    pub ref_level: f64,
}

/// Estimate the radial from a block of AM-envelope samples at `rate` Hz.
/// Needs roughly ≥ 0.3 s; longer blocks average down noise. `None` when the
/// block is too short or no tone is present.
pub fn estimate_radial(env: &[f32], rate: f64) -> Option<RadialEstimate> {
    let n = env.len();
    if (n as f64) < rate * 0.25 {
        return None;
    }

    let mean = env.iter().map(|&v| v as f64).sum::<f64>() / n as f64;
    if mean <= 1e-6 {
        return None;
    }
    // Modulation waveform: carrier level removed, scaled to depth.
    let x: Vec<f64> = env.iter().map(|&v| v as f64 / mean - 1.0).collect();

    // --- variable branch: the 30 Hz AM tone ---
    // A 30 Hz single-bin DFT over many cycles already rejects the far-off
    // 1020/9960 Hz components and voice, so no pre-filter here — that keeps
    // `var_level` a true reading of the ~0.30 AM depth.
    let (vr, vi) = goertzel(&x, rate, TONE_HZ);
    let var_phase = vi.atan2(vr);
    let var_level = (vr * vr + vi * vi).sqrt() / n as f64 * 2.0;

    // --- reference branch: FM-demod the 9960 Hz subcarrier, then its 30 Hz ---
    let w = 2.0 * PI * SUBCARRIER_HZ / rate;
    let mut zr = vec![0.0f64; n];
    let mut zi = vec![0.0f64; n];
    for (k, &xk) in x.iter().enumerate() {
        let (s, c) = (w * k as f64).sin_cos();
        zr[k] = xk * c; // downconvert 9960 -> DC
        zi[k] = -xk * s;
    }
    complex_lowpass_zerophase(&mut zr, &mut zi, rate, 1200.0, 2);

    // Quadrature FM discriminator: phase increment per sample.
    let mut disc = vec![0.0f64; n];
    for k in 1..n {
        let re = zr[k] * zr[k - 1] + zi[k] * zi[k - 1];
        let im = zi[k] * zr[k - 1] - zr[k] * zi[k - 1];
        disc[k] = im.atan2(re);
    }
    if n > 1 {
        disc[0] = disc[1];
    }
    // Zero-phase, so no bearing error: knocks down discriminator hiss before
    // the 30 Hz pick without shifting its phase.
    let refb = lowpass_zerophase(&disc, rate, 200.0, 2);
    let (rr, ri) = goertzel(&refb, rate, TONE_HZ);
    let ref_phase = ri.atan2(rr);
    let ref_level = (rr * rr + ri * ri).sqrt() / n as f64 * 2.0;

    if var_level < 1e-4 || ref_level < 1e-7 {
        return None;
    }

    let radial = wrap360(RADIAL_SIGN * (var_phase - ref_phase).to_degrees() + RADIAL_CALIBRATION_DEG);
    Some(RadialEstimate {
        radial_deg: radial,
        var_level,
        ref_level,
    })
}

/// Single-bin DFT at `freq`. Returns `(re, im)` of `Σ x[k]·e^{-jωk}`; for
/// `x[k] = A·cos(ωk + φ)` this is `≈ (N/2)·A·(cos φ + j sin φ)`.
fn goertzel(x: &[f64], rate: f64, freq: f64) -> (f64, f64) {
    let w = 2.0 * PI * freq / rate;
    let (mut re, mut im) = (0.0, 0.0);
    for (k, &v) in x.iter().enumerate() {
        let (s, c) = (w * k as f64).sin_cos();
        re += v * c;
        im -= v * s;
    }
    (re, im)
}

/// Forward–backward one-pole low-pass cascade: real filter, zero net phase, so
/// the tone-phase measurements in the two branches stay comparable.
fn lowpass_zerophase(x: &[f64], rate: f64, cutoff_hz: f64, stages: usize) -> Vec<f64> {
    let a = (-2.0 * PI * cutoff_hz / rate).exp();
    let g = 1.0 - a;
    let mut buf = x.to_vec();
    for _ in 0..stages {
        let mut y = buf.first().copied().unwrap_or(0.0);
        for v in buf.iter_mut() {
            y = a * y + g * *v;
            *v = y;
        }
        let mut y = buf.last().copied().unwrap_or(0.0);
        for v in buf.iter_mut().rev() {
            y = a * y + g * *v;
            *v = y;
        }
    }
    buf
}

fn complex_lowpass_zerophase(
    re: &mut [f64],
    im: &mut [f64],
    rate: f64,
    cutoff_hz: f64,
    stages: usize,
) {
    let a = (-2.0 * PI * cutoff_hz / rate).exp();
    let g = 1.0 - a;
    let run = |ch: &mut [f64]| {
        for _ in 0..stages {
            let mut y = ch.first().copied().unwrap_or(0.0);
            for v in ch.iter_mut() {
                y = a * y + g * *v;
                *v = y;
            }
            let mut y = ch.last().copied().unwrap_or(0.0);
            for v in ch.iter_mut().rev() {
                y = a * y + g * *v;
                *v = y;
            }
        }
    };
    run(re);
    run(im);
}

// --- RBJ biquad (bandpass) for the ident tone ---

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn bandpass(rate: f64, f0: f64, q: f64) -> Self {
        let w0 = 2.0 * PI * f0 / rate;
        let (sn, cs) = w0.sin_cos();
        let alpha = sn / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            b1: 0.0,
            b2: -alpha / a0,
            a1: -2.0 * cs / a0,
            a2: (1.0 - alpha) / a0,
            ..Default::default()
        }
    }
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Streaming decoder for the keyed 1020 Hz Morse ident. Fed the same envelope
/// samples as the radial estimator; keeps state across calls because a VOR
/// only sends its ident every ~10 s.
pub struct IdentDecoder {
    rate: f64,
    bp: Biquad,
    env: f64,
    env_a: f64,
    /// Slowly-relaxing high/low envelope bounds → hysteresis threshold.
    hi: f64,
    lo: f64,
    keyed: bool,
    run: usize,
    unit_samples: f64,
    symbol: String,
    group: String,
    last: Option<String>,
}

impl IdentDecoder {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            bp: Biquad::bandpass(rate, 1020.0, 12.0),
            env: 0.0,
            env_a: (-2.0 * PI * 60.0 / rate).exp(),
            hi: 0.0,
            lo: 0.0,
            keyed: false,
            run: 0,
            unit_samples: rate * 0.12, // ~10 wpm starting guess
            symbol: String::new(),
            group: String::new(),
            last: None,
        }
    }

    /// The most recently completed ident group, if any.
    pub fn current(&self) -> Option<&str> {
        self.last.as_deref()
    }

    pub fn push(&mut self, env: &[f32]) {
        let g = 1.0 - self.env_a;
        // hi/lo relax toward each other at this fraction of their spread per
        // sample — slow enough to survive a symbol and the 3–7 unit gaps, fast
        // enough to track a fading signal over a multi-second ident.
        let relax = 5e-5;
        for &s in env {
            let tone = self.bp.process(s as f64).abs();
            self.env = self.env_a * self.env + g * tone;

            if self.env > self.hi {
                self.hi = self.env;
            } else {
                self.hi -= (self.hi - self.lo) * relax;
            }
            if self.env < self.lo {
                self.lo = self.env;
            } else {
                self.lo += (self.hi - self.lo) * relax;
            }
            let span = self.hi - self.lo;
            let valid = span > 1e-4 && self.hi > self.lo * 3.0;
            let on = if !valid {
                false
            } else if self.keyed {
                self.env > self.lo + span * 0.4
            } else {
                self.env > self.lo + span * 0.6
            };

            if on == self.keyed {
                self.run += 1;
                // A long gap ends the group.
                if !self.keyed && self.run as f64 > self.unit_samples * 6.0 && !self.group.is_empty() {
                    self.flush_symbol();
                    if !self.group.is_empty() {
                        self.last = Some(std::mem::take(&mut self.group));
                    }
                    self.group.clear();
                }
            } else {
                self.classify_run();
                self.keyed = on;
                self.run = 1;
            }
        }
    }

    fn classify_run(&mut self) {
        let len = self.run as f64;
        if self.keyed {
            // just-ended ON run: dit or dah
            if len < self.unit_samples * 2.0 {
                self.symbol.push('.');
                // adapt the unit estimate toward observed dit length
                self.unit_samples += (len - self.unit_samples) * 0.2;
            } else {
                self.symbol.push('-');
                self.unit_samples += (len / 3.0 - self.unit_samples) * 0.1;
            }
            self.unit_samples = self.unit_samples.clamp(self.rate * 0.03, self.rate * 0.3);
        } else {
            // just-ended OFF run: intra-char / char gap / (group gap handled above)
            if len > self.unit_samples * 2.0 {
                self.flush_symbol();
            }
        }
    }

    fn flush_symbol(&mut self) {
        if self.symbol.is_empty() {
            return;
        }
        if let Some(ch) = morse::letter_from(&self.symbol) {
            self.group.push(ch);
        }
        self.symbol.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI as P;

    const RATE: f64 = 24_000.0;

    /// Synthesise a clean VOR AM-envelope for a given radial (degrees), per the
    /// convention: variable leads reference by the radial angle.
    fn synth_vor(radial_deg: f64, secs: f64, noise: f64) -> Vec<f32> {
        let n = (RATE * secs) as usize;
        let r = radial_deg.to_radians();
        let beta = 480.0 / 30.0; // FM modulation index
        let mut seed = 0x1234_5678u32;
        let mut rng = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f64 / (1u32 << 24) as f64 - 0.5
        };
        (0..n)
            .map(|k| {
                let t = k as f64 / RATE;
                let variable = 0.30 * (2.0 * P * 30.0 * t + r).cos();
                let subcarrier =
                    0.30 * (2.0 * P * SUBCARRIER_HZ * t + beta * (2.0 * P * 30.0 * t).sin()).cos();
                (1.0 + variable + subcarrier + noise * rng()) as f32
            })
            .collect()
    }

    #[test]
    fn recovers_several_radials() {
        for target in [0.0, 45.0, 137.0, 213.0, 330.0] {
            let env = synth_vor(target, 0.8, 0.0);
            let est = estimate_radial(&env, RATE).expect("estimate");
            let err = crate::nav::geo::angle_diff(est.radial_deg, target).abs();
            assert!(err < 2.0, "radial {target}: got {:.1} (err {err:.2})", est.radial_deg);
            assert!(est.var_level > 0.2 && est.var_level < 0.4, "var {}", est.var_level);
        }
    }

    #[test]
    fn tolerates_moderate_noise() {
        let env = synth_vor(88.0, 1.0, 0.15);
        let est = estimate_radial(&env, RATE).expect("estimate");
        let err = crate::nav::geo::angle_diff(est.radial_deg, 88.0).abs();
        assert!(err < 6.0, "got {:.1}", est.radial_deg);
    }

    #[test]
    fn too_short_returns_none() {
        assert!(estimate_radial(&synth_vor(0.0, 0.1, 0.0), RATE).is_none());
    }

    /// Key a 1020 Hz tone as Morse for `text` and check the decoder recovers it.
    fn synth_ident(text: &str, unit_s: f64) -> Vec<f32> {
        let unit = (RATE * unit_s) as usize;
        let mut out: Vec<f32> = Vec::new();
        let tone = |on: bool, samples: usize, out: &mut Vec<f32>| {
            for k in 0..samples {
                let t = (out.len() as f64 + k as f64) / RATE;
                out.push(if on {
                    (0.5 * (2.0 * P * 1020.0 * t).sin()) as f32
                } else {
                    0.0
                });
            }
        };
        tone(false, unit * 8, &mut out); // lead-in silence
        for (ci, ch) in text.chars().enumerate() {
            if ci > 0 {
                tone(false, unit * 3, &mut out); // inter-char gap
            }
            let code = morse::code_for(ch).unwrap();
            for (ei, sym) in code.chars().enumerate() {
                if ei > 0 {
                    tone(false, unit, &mut out); // intra-char gap
                }
                tone(true, if sym == '-' { unit * 3 } else { unit }, &mut out);
            }
        }
        tone(false, unit * 10, &mut out); // trailing group gap
        out
    }

    #[test]
    fn decodes_a_keyed_ident() {
        let mut dec = IdentDecoder::new(RATE);
        let sig = synth_ident("PDZ", 0.09);
        for chunk in sig.chunks(2048) {
            dec.push(chunk);
        }
        assert_eq!(dec.current(), Some("PDZ"));
    }
}
