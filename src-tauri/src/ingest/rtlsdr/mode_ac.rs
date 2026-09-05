//! Legacy ATCRBS Mode A/C detection — the pre-Mode-S transponder waveform
//! (1950s radar beacon system), still used by some older GA aircraft,
//! gliders, and military-adjacent equipment. Runs on the exact same 1090MHz
//! magnitude stream `demod.rs` already captures for Mode S, as a second,
//! independent pattern search (different preamble, different pulse timing
//! entirely — nothing here is Mode-S-specific).
//!
//! Timing (F1/F2 framing pulse spacing, inter-pulse spacing, the
//! C1/A1/C2/A2/C4/A4/X/B1/D1/B2/D2/B4 chronological pulse order, and the
//! Gillham-code altitude decode algorithm) is a decades-old, universally
//! documented ICAO/RTCA physical standard — confirmed against public
//! references, and the altitude decode specifically ported from
//! `dump1090`'s well-established `internalModeAToModeC` (same bit-mask/XOR
//! correction sequence, not rederived).
//!
//! **Fundamental limitation, not a bug to fix later:** a Mode A/C reply
//! carries no ICAO address and no position. A single passive receiver can't
//! tell *where* one of these is, only that something nearby replied with a
//! given code — and can't even tell whether that code means a squawk or an
//! altitude (that depends on which interrogation triggered the reply, which
//! only the interrogating radar/TCAS unit — not us — actually knows).
//! `squawk_string`/`altitude_ft` below are both offered as *possible*
//! readings of the same bits, not a confident single answer.

/// Matches the existing Mode S magnitude stream's rate — see `demod.rs`.
const SAMPLE_HZ: f64 = 2_000_000.0;
const US: f64 = SAMPLE_HZ / 1_000_000.0; // samples per microsecond (2.0)

/// F1-to-F2 framing pulse spacing.
const FRAME_US: f64 = 20.3;
/// Spacing between the 13 possible pulse positions (F1, 12 data slots, F2
/// are 14 equally-spaced instants; this is that interval).
const SLOT_US: f64 = 1.45;
const DATA_SLOTS: usize = 13;
/// Chronological transmission order of the 13 slots between (exclusive)
/// the framing pulses — confirmed against a published F1-relative timing
/// table (1.45, 2.90, 4.35, 5.80, 7.25, 8.70, [10.15=X], 11.60, 13.05,
/// 14.50, 15.95, 17.40, [18.85=D4] µs). The 7th, `X`, isn't part of any
/// squawk/altitude digit and is decoded but otherwise unused. The last
/// slot, D4, mirrors C4/A4's position in the first half of the pattern —
/// easy to miss since `dump1090`'s validity checks treat D-group bits as
/// essentially never legitimately set for altitude, but the physical pulse
/// position is real and must be decoded (and skipped) like any other, not
/// omitted, or every slot after it would decode one position off.
const SLOT_LABELS: [Label; DATA_SLOTS] = [
    Label::C1,
    Label::A1,
    Label::C2,
    Label::A2,
    Label::C4,
    Label::A4,
    Label::X,
    Label::B1,
    Label::D1,
    Label::B2,
    Label::D2,
    Label::B4,
    Label::D4,
];

#[derive(Clone, Copy)]
enum Label {
    A1,
    A2,
    A4,
    B1,
    B2,
    B4,
    C1,
    C2,
    C4,
    D1,
    D2,
    D4,
    X,
}

/// Packs into the same 16-bit layout `dump1090` uses (and its altitude
/// decode below expects): `0:A4:A2:A1 0:B4:B2:B1 0:C4:C2:C1 0:D4:D2:D1`.
/// Bit 15/11/7/3 are always 0.
fn pack_bit(code: &mut u16, label: Label, present: bool) {
    if !present {
        return;
    }
    let bit = match label {
        Label::A1 => 12,
        Label::A2 => 13,
        Label::A4 => 14,
        Label::B1 => 8,
        Label::B2 => 9,
        Label::B4 => 10,
        Label::C1 => 4,
        Label::C2 => 5,
        Label::C4 => 6,
        Label::D1 => 0,
        Label::D2 => 1,
        Label::D4 => 2,
        Label::X => return, // not part of the packed value at all
    };
    *code |= 1 << bit;
}

/// Linear interpolation at a fractional sample index — needed since 1.45µs
/// (2.9 samples at 2 Msps) doesn't land on a whole sample.
fn interp(mag: &[u16], t: f64) -> f64 {
    let i0 = t.floor() as usize;
    let frac = t - i0 as f64;
    let a = *mag.get(i0).unwrap_or(&0) as f64;
    let b = *mag.get(i0 + 1).unwrap_or(&(a as u16)) as f64;
    a + (b - a) * frac
}

/// A detected reply's raw packed code, before deciding whether to read it
/// as a squawk, an altitude, or both.
pub struct Candidate {
    pub code: u16,
}

/// Scan `mag` for ATCRBS Mode A/C replies. Same return shape as
/// `demod::demod`: candidates plus how many samples were consumed, so the
/// caller can keep whatever tail wasn't consumed for the next chunk.
pub fn scan(mag: &[u16]) -> (Vec<Candidate>, usize) {
    let mut out = Vec::new();
    let mut i = 0usize;
    let span = (FRAME_US * US).ceil() as usize + 2;
    while i + span <= mag.len() {
        if let Some(code) = try_reply_at(mag, i) {
            out.push(Candidate { code });
            // A real reply is at least this long; skip past it rather than
            // re-matching inside its own pulses.
            i += span;
            continue;
        }
        i += 1;
    }
    (out, i)
}

/// Ratio + flat-margin threshold, same shape as `atc.rs`'s squelch (a pure
/// ratio collapses to a near-zero threshold whenever the sampled floor
/// happens to be very quiet, letting almost any blip through). Needed
/// because ATCRBS's 2-peak signature is a much weaker constraint than Mode
/// S's 4-peak preamble — a busy 1090MHz environment has near-continuous
/// Mode S activity, and a loose threshold here matches a lot of it by
/// coincidence rather than genuine Mode A/C replies. Unverified against
/// real hardware (no ATCRBS traffic confirmed yet) — a starting point to
/// retune if real reception still shows implausibly many contacts.
const THRESHOLD_RATIO: f64 = 3.5;
const THRESHOLD_MARGIN: f64 = 10.0;

fn try_reply_at(mag: &[u16], start: usize) -> Option<u16> {
    let f1 = interp(mag, start as f64);
    let f2 = interp(mag, start as f64 + FRAME_US * US);

    // Noise floor from every half-slot gap in the window (before slot 1,
    // between each pair of adjacent slots, after slot 13) — always
    // strictly between two real pulse positions, so none can coincide
    // with an actual pulse regardless of which slots this particular
    // reply has set. Using all 14 (rather than a handful) makes the
    // estimate far less likely to be thrown off by one unlucky quiet
    // sample landing right where a real signal's floor should be judged.
    let floor = (0..=DATA_SLOTS)
        .map(|k| interp(mag, start as f64 + (k as f64 + 0.5) * SLOT_US * US))
        .fold(0.0, f64::max)
        .max(1.0);
    let threshold = floor * THRESHOLD_RATIO + THRESHOLD_MARGIN;

    if f1 < threshold || f2 < threshold {
        return None;
    }

    let mut code = 0u16;
    for (n, &label) in SLOT_LABELS.iter().enumerate() {
        let t = start as f64 + (n as f64 + 1.0) * SLOT_US * US;
        let present = interp(mag, t) > threshold;
        pack_bit(&mut code, label, present);
    }
    // A real Gillham code essentially never sets every single data bit —
    // this is a cheap sanity check against a noisy/busy window that
    // happened to clear the F1/F2 threshold by chance.
    if code.count_ones() >= 11 {
        return None;
    }
    Some(code)
}

/// The 4 squawk digits (each independently 0-7, since each comes from
/// exactly 3 bits) as a display string — always well-formed, since there's
/// no invalid bit pattern for a squawk reading the way there is for
/// altitude (see `altitude_ft`).
pub fn squawk_string(code: u16) -> String {
    let digit = |nibble: u16| -> char {
        let v = (code >> (nibble * 4)) & 0x7;
        (b'0' + v as u8) as char
    };
    // nibble 3 = A (bits 14-12), 2 = B (10-8), 1 = C (6-4), 0 = D (2-0).
    [digit(3), digit(2), digit(1), digit(0)].iter().collect()
}

/// Gillham-decoded altitude in feet, or `None` if this code isn't a
/// structurally valid Mode C reading (ported from `dump1090`'s
/// `internalModeAToModeC`, same validity checks and correction sequence).
pub fn altitude_ft(code: u16) -> Option<f64> {
    // Reserved bits (15,11,7,3) must be zero, and D1 (bit 0) set is
    // illegal for altitude; C1..C4 (bits 4-6) can't all be zero. (dump1090
    // checks `ModeA & 0xFFFF8889`, a 32-bit mask — the high `FFFF` is
    // irrelevant to a genuinely-16-bit value, leaving `0x8889`.)
    if code & 0x8889 != 0 || code & 0x0070 == 0 {
        return None;
    }

    let mut hundreds: i32 = 0;
    if code & 0x0010 != 0 {
        hundreds ^= 0x007;
    } // C1
    if code & 0x0020 != 0 {
        hundreds ^= 0x003;
    } // C2
    if code & 0x0040 != 0 {
        hundreds ^= 0x001;
    } // C4
    if hundreds & 5 == 5 {
        hundreds ^= 2; // 7s <-> 5s correction
    }
    if hundreds > 5 {
        return None;
    }

    let mut five_hundreds: i32 = 0;
    if code & 0x0002 != 0 {
        five_hundreds ^= 0x0FF;
    } // D2
    if code & 0x0004 != 0 {
        five_hundreds ^= 0x07F;
    } // D4
    if code & 0x1000 != 0 {
        five_hundreds ^= 0x03F;
    } // A1
    if code & 0x2000 != 0 {
        five_hundreds ^= 0x01F;
    } // A2
    if code & 0x4000 != 0 {
        five_hundreds ^= 0x00F;
    } // A4
    if code & 0x0100 != 0 {
        five_hundreds ^= 0x007;
    } // B1
    if code & 0x0200 != 0 {
        five_hundreds ^= 0x003;
    } // B2
    if code & 0x0400 != 0 {
        five_hundreds ^= 0x001;
    } // B4

    if five_hundreds & 1 != 0 {
        hundreds = 6 - hundreds;
    }

    Some(((five_hundreds * 5 + hundreds - 13) * 100) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a raw 13-slot bit pattern (in `SLOT_LABELS` chronological
    /// order, X included as a don't-care) into a synthetic magnitude
    /// stream: clean F1/F2 pulses plus whichever data slots are set.
    fn synth(slot_bits: [bool; DATA_SLOTS], peak: u16, floor: u16) -> Vec<u16> {
        let span = (FRAME_US * US).ceil() as usize + 4;
        let mut w = vec![floor; span];
        let put = |w: &mut Vec<u16>, t: f64, v: u16| {
            // Land squarely on a sample (offset .5 into the interpolation
            // window rounds cleanly) so the synthetic pulse isn't split
            // across two samples and softened by `interp`.
            let i = t.round() as usize;
            w[i] = v;
        };
        put(&mut w, 0.0, peak); // F1
        put(&mut w, FRAME_US * US, peak); // F2
        for (n, &bit) in slot_bits.iter().enumerate() {
            if bit {
                put(&mut w, (n as f64 + 1.0) * SLOT_US * US, peak);
            }
        }
        w
    }

    fn code_from_labels(set: &[Label]) -> u16 {
        let mut code = 0u16;
        for &l in set {
            pack_bit(&mut code, l, true);
        }
        code
    }

    #[test]
    fn decodes_the_last_slot_d4_through_the_full_synthetic_pipeline() {
        // D4 is the 13th and last data slot, chronologically farthest from
        // F1 — the exact position a slot-count/timing-table mistake would
        // most likely miss (as one already did during development: the
        // slot table was originally missing D4 entirely).
        let mut slot_bits = [false; DATA_SLOTS];
        for (n, &label) in SLOT_LABELS.iter().enumerate() {
            slot_bits[n] = matches!(label, Label::D4);
        }
        let mag = synth(slot_bits, 200, 10);
        let (found, _) = scan(&mag);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, code_from_labels(&[Label::D4]));
    }

    #[test]
    fn decodes_a_squawk_like_reply() {
        // Squawk 1200: A=1 (A1), B=2 (B2), C=0, D=0.
        let mut slot_bits = [false; DATA_SLOTS];
        for (n, &label) in SLOT_LABELS.iter().enumerate() {
            slot_bits[n] = matches!(label, Label::A1 | Label::B2);
        }
        let mag = synth(slot_bits, 200, 10);
        let (found, _) = scan(&mag);
        assert_eq!(found.len(), 1);
        assert_eq!(squawk_string(found[0].code), "1200");
    }

    #[test]
    fn decodes_a_plausible_altitude() {
        // Build a code from labels directly (bypassing the synthetic pulse
        // stream) to check the Gillham math against a known-good case:
        // all of A1,B1,C1 set, nothing else -> a specific, verifiable
        // altitude via the same arithmetic `altitude_ft` uses.
        let code = code_from_labels(&[Label::A1, Label::B1, Label::C1]);
        // hundreds: C1 -> hundreds^=7 => 7, then (7&5==5)? 7&5=5 -> ^=2 -> 5
        // five_hundreds: A1(^0x3F)=0x3F, B1(^0x7)=0x38 -> 0011_1000=56
        // five_hundreds&1 == 0, so hundreds stays 5
        // altitude = (56*5 + 5 - 13)*100 = (280+5-13)*100 = 272*100=27200
        assert_eq!(altitude_ft(code), Some(27_200.0));
    }

    #[test]
    fn rejects_structurally_invalid_altitude_code() {
        // C1..C4 all zero is explicitly invalid per the module doc.
        let code = code_from_labels(&[Label::A1]);
        assert_eq!(altitude_ft(code), None);
    }

    #[test]
    fn d1_set_is_illegal_for_altitude() {
        let code = code_from_labels(&[Label::C1, Label::D1]);
        assert_eq!(altitude_ft(code), None);
    }

    #[test]
    fn rejects_a_window_with_almost_every_slot_set() {
        // Not a plausible Gillham code — the kind of thing a busy/noisy
        // window could produce if it happened to clear the F1/F2
        // threshold by chance. All 12 real data slots on (X left off,
        // since it's never part of `code` regardless).
        let mut slot_bits = [true; DATA_SLOTS];
        for (n, &label) in SLOT_LABELS.iter().enumerate() {
            if matches!(label, Label::X) {
                slot_bits[n] = false;
            }
        }
        let mag = synth(slot_bits, 200, 10);
        let (found, _) = scan(&mag);
        assert!(found.is_empty());
    }

    #[test]
    fn ignores_pure_noise() {
        let mag = vec![50u16; 400];
        let (found, _) = scan(&mag);
        assert!(found.is_empty());
    }

    #[test]
    fn squawk_string_is_always_four_digits_0_to_7() {
        for code in [0x0000u16, 0x7777, 0x1234 & 0x7777] {
            let s = squawk_string(code);
            assert_eq!(s.len(), 4);
            assert!(s.chars().all(|c| ('0'..='7').contains(&c)));
        }
    }
}
