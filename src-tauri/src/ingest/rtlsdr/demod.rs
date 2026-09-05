//! Raw-IQ Mode S / ADS-B demodulation — a from-spec implementation. Preamble
//! shape and pulse-position-modulation (PPM) bit timing are from the public
//! ICAO Annex 10 Volume IV Mode S specification (also described in numerous
//! independent public technical references, e.g. Junzi Sun's "The 1090MHz
//! Riddle"), not derived from dump1090 or any other existing demodulator's
//! source. CRC/message validation is left to `adsb_deku` (see `mod.rs`)
//! rather than reimplemented here.
//!
//! Expects magnitude samples at exactly 2,000,000 Hz (2 samples/µs) so
//! preamble and bit timing land on exact sample boundaries — no fractional-
//! sample interpolation needed. `rs_rtl` supports this rate directly, so the
//! device is configured for it rather than the fractional 2.4 Msps some
//! other tools use.

/// Preamble: four 0.5µs pulses at t = 0.0, 1.0, 3.5, 4.5µs into an 8µs
/// window — i.e. samples 0, 2, 7, 9 of a 16-sample (2 Msps) window.
const PREAMBLE_LEN: usize = 16;
const PREAMBLE_PEAKS: [usize; 4] = [0, 2, 7, 9];

const SHORT_MSG_BITS: usize = 56;
const LONG_MSG_BITS: usize = 112;

/// A demodulated Mode S message: full byte content (7 or 14 bytes,
/// depending on DF), not yet CRC-validated.
pub struct Candidate {
    pub bytes: Vec<u8>,
}

/// Convert interleaved RTL-SDR IQ bytes (`[I0, Q0, I1, Q1, ...]`, unsigned,
/// centred on ~127.5) into per-sample magnitude. u16 headroom keeps the
/// preamble comparisons below simple and overflow-free.
pub fn magnitude(iq: &[u8]) -> Vec<u16> {
    iq.chunks_exact(2)
        .map(|p| {
            let i = f32::from(p[0]) - 127.5;
            let q = f32::from(p[1]) - 127.5;
            (i * i + q * q).sqrt() as u16
        })
        .collect()
}

/// Scan `mag` for Mode S messages. Returns each candidate found together
/// with the sample index *after* it, so callers can keep whatever tail of
/// `mag` wasn't consumed for the next chunk.
pub fn demod(mag: &[u16]) -> (Vec<Candidate>, usize) {
    let mut out = Vec::new();
    let mut i = 0;
    // Only require enough for the *shortest* possible message here —
    // `decode_message` itself declines (returns `None`, retried once more
    // data arrives) if a long message's tail hasn't been received yet.
    let min_span = PREAMBLE_LEN + SHORT_MSG_BITS * 2;
    while i + min_span <= mag.len() {
        if looks_like_preamble(&mag[i..i + PREAMBLE_LEN]) {
            if let Some(bytes) = decode_message(&mag[i + PREAMBLE_LEN..]) {
                let consumed = PREAMBLE_LEN + bytes.len() * 8 * 2;
                out.push(Candidate { bytes });
                i += consumed;
                continue;
            }
        }
        i += 1;
    }
    (out, i)
}

/// The four expected pulses must all clear the noise floor set by the
/// non-pulse positions with a healthy margin — a relative (not absolute)
/// test, so it works across whatever signal strength/gain the dongle sees.
fn looks_like_preamble(w: &[u16]) -> bool {
    debug_assert_eq!(w.len(), PREAMBLE_LEN);
    let peak = PREAMBLE_PEAKS.iter().map(|&i| w[i]).min().unwrap_or(0);
    let valley = (0..PREAMBLE_LEN)
        .filter(|i| !PREAMBLE_PEAKS.contains(i))
        .map(|i| w[i])
        .max()
        .unwrap_or(u16::MAX);
    peak > 0 && peak as u32 > valley as u32 * 2
}

/// PPM-demodulate the message following the preamble: read the 5-bit
/// downlink format to learn the message length (bit 0 of DF's MSBs set ⇒
/// 112-bit "long" message, else 56-bit "short"), then the rest.
fn decode_message(data: &[u16]) -> Option<Vec<u8>> {
    if data.len() < SHORT_MSG_BITS * 2 {
        return None;
    }
    let df = (0..5).fold(0u8, |acc, b| (acc << 1) | read_bit(data, b));
    let bits = if df & 0x10 != 0 { LONG_MSG_BITS } else { SHORT_MSG_BITS };
    if data.len() < bits * 2 {
        return None;
    }
    let mut bytes = vec![0u8; bits / 8];
    for b in 0..bits {
        if read_bit(data, b) == 1 {
            bytes[b / 8] |= 1 << (7 - (b % 8));
        }
    }
    Some(bytes)
}

/// PPM: a pulse in the first half of the bit period is a 1, second half a 0
/// (ICAO Annex 10 Vol IV §3.1.2.3.3).
fn read_bit(data: &[u16], bit: usize) -> u8 {
    u8::from(data[bit * 2] > data[bit * 2 + 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic 2 Msps magnitude stream for a given message: a
    /// clean preamble followed by PPM-encoded bits, `peak`/`floor` standing
    /// in for signal/noise level (no actual RF involved — this validates
    /// the demod *logic*, not real-world reception).
    fn synth(bytes: &[u8], peak: u16, floor: u16) -> Vec<u16> {
        let mut w = vec![floor; PREAMBLE_LEN];
        for &p in &PREAMBLE_PEAKS {
            w[p] = peak;
        }
        for byte in bytes {
            for bit_i in 0..8 {
                let bit = (byte >> (7 - bit_i)) & 1;
                if bit == 1 {
                    w.push(peak);
                    w.push(floor);
                } else {
                    w.push(floor);
                    w.push(peak);
                }
            }
        }
        w
    }

    #[test]
    fn recovers_a_short_message() {
        // DF0 (short, 56 bits) — top bit of the DF nibble clear.
        let msg = [0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mag = synth(&msg, 200, 10);
        let (found, _) = demod(&mag);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bytes, msg);
    }

    #[test]
    fn recovers_a_long_df17_message() {
        // DF17 (10001 = 0x11 in the top 5 bits) — long, 112 bits.
        let mut msg = vec![0x8Du8, 0x48, 0x41, 0x0d, 0x99];
        msg.extend_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99]);
        assert_eq!(msg.len(), 14);
        let mag = synth(&msg, 220, 8);
        let (found, consumed) = demod(&mag);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bytes, msg);
        assert_eq!(consumed, mag.len());
    }

    #[test]
    fn ignores_pure_noise() {
        let mag = vec![50u16; 400];
        let (found, _) = demod(&mag);
        assert!(found.is_empty());
    }

    #[test]
    fn weak_preamble_margin_is_rejected() {
        // Peaks only marginally above the floor — not a confident detection.
        let msg = [0u8; 7];
        let mag = synth(&msg, 20, 15);
        let (found, _) = demod(&mag);
        assert!(found.is_empty());
    }

    #[test]
    fn magnitude_is_zero_at_dc_center() {
        // 127/128 round-trips to ~127.5 center — near-zero magnitude.
        let mag = magnitude(&[127, 128, 128, 127]);
        assert!(mag.iter().all(|&m| m <= 1));
    }

    #[test]
    fn magnitude_scales_with_deviation_from_center() {
        let mag = magnitude(&[255, 128, 0, 128]);
        assert!(mag[0] > 100);
        assert!(mag[1] > 100);
    }
}
