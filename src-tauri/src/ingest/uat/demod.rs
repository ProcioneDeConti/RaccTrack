//! UAT physical layer: true frequency/phase demodulation (binary CPFSK),
//! unlike everything else this app decodes off an RTL-SDR (1090ES and the
//! ATC/ACARS AM paths are all amplitude-envelope based). Parameters are
//! taken from `dump978` (the reference open-source UAT decoder), not
//! rederived: 2 samples/bit at `SAMPLE_HZ`, a 36-bit sync word, always
//! demodulating a full `LONG_FRAME_BYTES` after it (FEC in `super::rs`
//! decides short vs. long — see `super::mod` for why).
//!
//! Only the downlink (aircraft-transmitted) sync word is searched for —
//! ground-station uplink (FIS-B weather/TIS-B) decoding is out of scope.

pub const SAMPLE_HZ: u32 = 2_083_334;
const SYNC_BITS: u32 = 36;
const SYNC_MASK: u64 = (1u64 << SYNC_BITS) - 1;
/// Downlink sync word (dump978 `ADSB_SYNC_WORD`). The uplink word is this
/// bit-complemented, which we never need since only downlink is decoded.
const DOWNLINK_SYNC: u64 = 0xE_ACDD_A4E2;

pub const LONG_FRAME_BYTES: usize = 48;
const LONG_FRAME_BITS: usize = LONG_FRAME_BYTES * 8;

/// A demodulated candidate: a full long-frame's worth of bytes (the FEC
/// layer determines whether it's actually a short or long message).
pub struct Candidate {
    pub bytes: [u8; LONG_FRAME_BYTES],
}

/// Phase advance from `a` to `b` (proportional to `sin(arg(b) - arg(a))`,
/// i.e. `Im(b * conj(a))`) — positive for an upward frequency deviation
/// (bit 1), negative for downward (bit 0). Cheaper than an actual `atan2`
/// discriminator and sufficient since only the sign relative to a
/// calibrated threshold matters here, not the magnitude.
fn cross(a: (f32, f32), b: (f32, f32)) -> f32 {
    a.0 * b.1 - a.1 * b.0
}

fn to_complex(iq: &[u8]) -> Vec<(f32, f32)> {
    iq.chunks_exact(2)
        .map(|c| (c[0] as f32 - 127.5, c[1] as f32 - 127.5))
        .collect()
}

/// Scan `iq` (raw RTL-SDR IQ bytes) for downlink sync words, demodulate a
/// long-frame's worth of bytes after each, and return the candidates plus
/// how many *bytes of `iq`* were consumed (the caller keeps the remainder
/// as a tail across read boundaries, same pattern as `ingest::rtlsdr`).
pub fn find_frames(iq: &[u8]) -> (Vec<Candidate>, usize) {
    let samples = to_complex(iq);
    let mut candidates = Vec::new();
    if samples.len() < 2 {
        return (candidates, 0);
    }

    // One cross-product per adjacent raw-sample pair; phase p's bitstream
    // is the subsequence starting at index p, stepping by 2.
    let cross_vals: Vec<f32> = (0..samples.len() - 1)
        .map(|n| cross(samples[n], samples[n + 1]))
        .collect();

    let mut regs = [0u64; 2]; // rolling 36-bit sync shift registers, one per phase
    let mut n = 0usize;
    let mut last_consumed_sample = 0usize;
    while n < cross_vals.len() {
        let phase = n % 2;
        let bit = cross_vals[n] > 0.0;
        regs[phase] = ((regs[phase] << 1) | bit as u64) & SYNC_MASK;

        if regs[phase] == DOWNLINK_SYNC {
            // The sync word's own bits are our threshold calibration: split
            // this phase's last 36 cross-product values by the sync word's
            // known bit pattern, threshold at the midpoint of the two
            // clusters' means (compensates for hardware DC/frequency
            // offset, same idea as the adaptive squelch elsewhere in this
            // app — see `atc.rs`).
            let sync_start = n as i64 - (SYNC_BITS as i64 - 1) * 2;
            if sync_start >= 0 {
                let (mut sum0, mut cnt0, mut sum1, mut cnt1) = (0.0f32, 0u32, 0.0f32, 0u32);
                for i in 0..SYNC_BITS as i64 {
                    let idx = (sync_start + i * 2) as usize;
                    let expect_one = (DOWNLINK_SYNC >> (SYNC_BITS as i64 - 1 - i)) & 1 == 1;
                    let v = cross_vals[idx];
                    if expect_one {
                        sum1 += v;
                        cnt1 += 1;
                    } else {
                        sum0 += v;
                        cnt0 += 1;
                    }
                }
                let threshold = if cnt0 > 0 && cnt1 > 0 {
                    (sum0 / cnt0 as f32 + sum1 / cnt1 as f32) / 2.0
                } else {
                    0.0
                };

                // Demodulate the frame immediately following the sync word.
                // `n` is this phase's *last* sync bit; the next bit at the
                // same phase parity is 2 cross-product indices further on.
                let frame_start = n + 2;
                let mut bytes = [0u8; LONG_FRAME_BYTES];
                let mut ok = true;
                'bytes: for (byte_i, byte) in bytes.iter_mut().enumerate() {
                    let mut b = 0u8;
                    for bit_i in 0..8 {
                        let ci = frame_start + (byte_i * 8 + bit_i) * 2;
                        let Some(&v) = cross_vals.get(ci) else {
                            ok = false;
                            break 'bytes;
                        };
                        if v > threshold {
                            b |= 0x80 >> bit_i;
                        }
                    }
                    *byte = b;
                }
                if ok {
                    candidates.push(Candidate { bytes });
                    last_consumed_sample = frame_start + LONG_FRAME_BITS * 2;
                    regs = [0; 2]; // frame consumed; don't re-match inside it
                    n = last_consumed_sample.max(n + 1);
                    continue;
                }
            }
        }
        last_consumed_sample = n;
        n += 1;
    }

    // Consumed bytes = 2 bytes (one IQ sample) per cross-product index
    // advanced past, plus the 1 sample of lookback `cross_vals` needs.
    let consumed_samples = last_consumed_sample.min(samples.len());
    (candidates, consumed_samples * 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode `bits` (MSB-first bool stream) as raw IQ bytes: each bit is
    /// one continuous-phase step, `+step` for 1 and `-step` for 0, sampled
    /// before and after the step — matching `cross()`'s sign convention
    /// (`cross(sample_before, sample_after) > 0` for an upward/1 step).
    fn encode_bits(bits: &[bool]) -> Vec<u8> {
        let mut phase = 0.0f32;
        let step = 0.6f32; // radians per bit, well inside +-pi/2 for cross() linearity
        let mut out = Vec::new();
        let push_sample = |out: &mut Vec<u8>, ph: f32| {
            let i = (127.5 + 100.0 * ph.cos()).round().clamp(0.0, 255.0) as u8;
            let q = (127.5 + 100.0 * ph.sin()).round().clamp(0.0, 255.0) as u8;
            out.push(i);
            out.push(q);
        };
        for &b in bits {
            push_sample(&mut out, phase);
            phase += if b { step } else { -step };
            push_sample(&mut out, phase);
        }
        out
    }

    fn sync_bits() -> Vec<bool> {
        (0..SYNC_BITS).map(|i| (DOWNLINK_SYNC >> (SYNC_BITS - 1 - i)) & 1 == 1).collect()
    }

    #[test]
    fn finds_a_sync_word_and_demodulates_the_frame_after_it() {
        let mut bits = sync_bits();
        // A distinctive, checkable frame payload.
        let frame_bits: Vec<bool> = (0..LONG_FRAME_BITS)
            .map(|i| (i / 8) % 2 == 0) // byte i: 0xFF if i even, 0x00 if odd
            .collect();
        bits.extend(&frame_bits);
        // Leading junk so the sync search has to skip past noise first.
        let mut iq = encode_bits(&[true, false, true, false, false, true]);
        iq.extend(encode_bits(&bits));

        let (candidates, _consumed) = find_frames(&iq);
        assert_eq!(candidates.len(), 1);
        for (i, &b) in candidates[0].bytes.iter().enumerate() {
            let expected = if i % 2 == 0 { 0xFF } else { 0x00 };
            assert_eq!(b, expected, "byte {i}");
        }
    }

    #[test]
    fn pure_noise_finds_nothing() {
        let mut x = 12345u64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let bits: Vec<bool> = (0..2000).map(|_| next() % 2 == 0).collect();
        let iq = encode_bits(&bits);
        let (candidates, _) = find_frames(&iq);
        assert!(candidates.is_empty());
    }
}
