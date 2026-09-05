//! MSK bit-level demodulation for ACARS. ACARS is data sent as audio-frequency
//! tones over an ordinary AM voice channel — the same AM envelope detector
//! `atc.rs` already uses for voice gives us the same audio waveform a human
//! would hear; this module turns that waveform into bits. `frame.rs` builds
//! the ARINC 618 character/message structure on top.
//!
//! ASSUMPTION carried through this module, not yet confirmed against a real
//! over-the-air capture: bit 0 = one half-cycle of a 1200 Hz tone, bit 1 =
//! one full cycle of a 2400 Hz tone (Sunde/MSK-style continuous-phase FSK at
//! 2400 baud) — the tone/bit mapping documented for VHF ACARS. If real
//! captures never sync (see `frame::decode_burst`) even on a clearly-open
//! squelch, try swapping the `>` in `bits_at_offset` before suspecting
//! anything else — that flips the mapping in one place.

pub const SAMPLE_HZ: u32 = 9600;
const BAUD: u32 = 2400;
/// 9600 / 2400 = 4 samples per bit — deliberately chosen (over decimating
/// straight from the IQ capture rate) so both tone periods land on whole
/// sample counts: 2400 Hz is exactly one cycle per bit, 1200 Hz exactly one
/// cycle per two bits. No fractional-sample resampling needed anywhere in
/// this module.
pub const SAMPLES_PER_BIT: usize = (SAMPLE_HZ / BAUD) as usize;
const TONE0_HZ: u32 = 1200;
const TONE1_HZ: u32 = 2400;

/// Both reference tones repeat with this period (samples) — tone1's period
/// (`SAMPLES_PER_BIT`) evenly divides it, so a single free-running sample
/// counter mod `LO_PERIOD` indexes both tables. Correlating against a
/// continuous local oscillator like this (rather than restarting the phase
/// at zero for every bit window) keeps the correlation phase-continuous
/// with the true incoming tone across bit boundaries, regardless of which
/// of the `SAMPLES_PER_BIT` candidate window alignments `bits_at_offset` is
/// being asked to try.
const LO_PERIOD: usize = (SAMPLE_HZ / TONE0_HZ) as usize;

struct Lo {
    cos0: [f64; LO_PERIOD],
    sin0: [f64; LO_PERIOD],
    cos1: [f64; LO_PERIOD],
    sin1: [f64; LO_PERIOD],
}

fn lo_tables() -> Lo {
    let mut lo = Lo {
        cos0: [0.0; LO_PERIOD],
        sin0: [0.0; LO_PERIOD],
        cos1: [0.0; LO_PERIOD],
        sin1: [0.0; LO_PERIOD],
    };
    for n in 0..LO_PERIOD {
        let t = n as f64 / SAMPLE_HZ as f64;
        let a0 = 2.0 * std::f64::consts::PI * TONE0_HZ as f64 * t;
        let a1 = 2.0 * std::f64::consts::PI * TONE1_HZ as f64 * t;
        lo.cos0[n] = a0.cos();
        lo.sin0[n] = a0.sin();
        lo.cos1[n] = a1.cos();
        lo.sin1[n] = a1.sin();
    }
    lo
}

/// Non-coherent tone-energy bit decision for every full bit window starting
/// at `phase_offset` (0..`SAMPLES_PER_BIT`) into `samples`. One bool per
/// window: `true` = tone1 (`1`), `false` = tone0 (`0`). Called once per
/// candidate offset per burst (a handful of times, on buffers a few hundred
/// milliseconds long) — not a hot per-sample path, so recomputing the LO
/// tables per call is fine.
pub fn bits_at_offset(samples: &[f64], phase_offset: usize) -> Vec<bool> {
    let lo = lo_tables();
    let mut bits = Vec::with_capacity(samples.len() / SAMPLES_PER_BIT);
    let mut i = phase_offset;
    while i + SAMPLES_PER_BIT <= samples.len() {
        let (mut i0, mut q0, mut i1, mut q1) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
        for k in 0..SAMPLES_PER_BIT {
            let s = samples[i + k];
            let n = (i + k) % LO_PERIOD;
            i0 += s * lo.cos0[n];
            q0 += s * lo.sin0[n];
            i1 += s * lo.cos1[n];
            q1 += s * lo.sin1[n];
        }
        let e0 = i0 * i0 + q0 * q0;
        let e1 = i1 * i1 + q1 * q1;
        bits.push(e1 > e0);
        i += SAMPLES_PER_BIT;
    }
    bits
}

#[cfg(test)]
pub(crate) fn encode_bit_samples(bit: bool, sample_offset: usize, amplitude: f64) -> Vec<f64> {
    let lo = lo_tables();
    (0..SAMPLES_PER_BIT)
        .map(|k| {
            let n = (sample_offset + k) % LO_PERIOD;
            amplitude * if bit { lo.cos1[n] } else { lo.cos0[n] }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_bit_pattern() {
        let bits_in = [true, false, false, true, true, true, false, false];
        let mut samples = Vec::new();
        for &b in &bits_in {
            samples.extend(encode_bit_samples(b, samples.len(), 1000.0));
        }
        let bits_out = bits_at_offset(&samples, 0);
        assert_eq!(&bits_out[..bits_in.len()], &bits_in[..]);
    }
}
