//! ACARS message framing on top of the bitstream `msk` produces — turns
//! recovered bits into ARINC 618 message fields.
//!
//! Same disclaimer as `msk.rs`: this is built from the documented ACARS
//! message structure, not yet confirmed byte-for-byte against a real
//! over-the-air capture. Every message that passes sync is surfaced with
//! `bcc_ok`/`parity_errors` set rather than being dropped when those don't
//! check out, so a real capture can be used to correct a field-boundary
//! assumption here without losing the message that would have revealed the
//! mistake.

use super::msk;
use serde::Serialize;

const SYN: u8 = 0x16;
const SOH: u8 = 0x01;
const STX: u8 = 0x02;
const ETX: u8 = 0x03;
const ETB: u8 = 0x17;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcarsMessage {
    pub mode: String,
    /// Aircraft registration with any leading `.` padding stripped.
    pub tail: String,
    pub tech_ack: String,
    pub label: String,
    pub block_id: String,
    pub text: Option<String>,
    /// Block-check (XOR over mode..terminator, inclusive) matched the
    /// transmitted check character. `false` doesn't necessarily mean the
    /// message is garbage — it may mean the BCC scope or a field width
    /// assumed here is wrong; see the module doc.
    pub bcc_ok: bool,
    pub parity_errors: u32,
    /// Left as `0.0`/`0` by `decode_burst` — a decoded frame has no notion
    /// of which frequency or when it was heard, so the caller
    /// (`acars::run_worker`) fills these in before the message is stored.
    pub freq_mhz: f64,
    pub timestamp_ms: i64,
}

/// One bit-synced ARINC/ASCII character: LSB-first 7 data bits + 1
/// odd-parity bit. Returns `(byte, parity_ok)`.
fn char_at(bits: &[bool], bit_pos: usize) -> Option<(u8, bool)> {
    if bit_pos + 8 > bits.len() {
        return None;
    }
    let mut byte = 0u8;
    for k in 0..7 {
        if bits[bit_pos + k] {
            byte |= 1 << k;
        }
    }
    let parity_bit = bits[bit_pos + 7];
    let ones = byte.count_ones() + parity_bit as u32;
    Some((byte, ones % 2 == 1))
}

fn take_chars(
    bits: &[bool],
    pos: &mut usize,
    n: usize,
    bcc: &mut u8,
    parity_errors: &mut u32,
) -> Option<String> {
    let mut s = String::with_capacity(n);
    for _ in 0..n {
        let (byte, ok) = char_at(bits, *pos)?;
        if !ok {
            *parity_errors += 1;
        }
        *bcc ^= byte;
        s.push(byte as char);
        *pos += 8;
    }
    Some(s)
}

/// Try every candidate bit-clock phase against a captured burst, decoding
/// the first one whose bitstream contains the `SYN SYN SOH` sync word.
/// Bursts are short (a fraction of a second of audio), so trying all
/// `SAMPLES_PER_BIT` offsets per burst is cheap.
pub fn decode_burst(samples: &[f64]) -> Option<AcarsMessage> {
    for offset in 0..msk::SAMPLES_PER_BIT {
        let bits = msk::bits_at_offset(samples, offset);
        if let Some(msg) = decode_bits(&bits) {
            return Some(msg);
        }
    }
    None
}

fn decode_bits(bits: &[bool]) -> Option<AcarsMessage> {
    let mut pos = 0usize;
    while pos + 24 <= bits.len() {
        let sync = (char_at(bits, pos), char_at(bits, pos + 8), char_at(bits, pos + 16));
        if let (Some((SYN, _)), Some((SYN, _)), Some((SOH, _))) = sync {
            if let Some(msg) = decode_message(bits, pos + 24) {
                return Some(msg);
            }
        }
        pos += 1;
    }
    None
}

fn decode_message(bits: &[bool], start: usize) -> Option<AcarsMessage> {
    let mut pos = start;
    let mut parity_errors = 0u32;
    let mut bcc = 0u8;

    let mode = take_chars(bits, &mut pos, 1, &mut bcc, &mut parity_errors)?;
    let tail_raw = take_chars(bits, &mut pos, 7, &mut bcc, &mut parity_errors)?;
    let tech_ack = take_chars(bits, &mut pos, 1, &mut bcc, &mut parity_errors)?;
    let label = take_chars(bits, &mut pos, 2, &mut bcc, &mut parity_errors)?;
    let block_id = take_chars(bits, &mut pos, 1, &mut bcc, &mut parity_errors)?;

    // Either STX + free text + ETX/ETB, or straight to ETX/ETB with no body.
    let (first, ok) = char_at(bits, pos)?;
    if !ok {
        parity_errors += 1;
    }
    let text = if first == STX {
        bcc ^= first;
        pos += 8;
        let mut body = String::new();
        loop {
            let (b, ok) = char_at(bits, pos)?;
            if !ok {
                parity_errors += 1;
            }
            bcc ^= b;
            pos += 8;
            if b == ETX || b == ETB {
                break;
            }
            body.push(b as char);
            // A real ACARS text block is well under this — treat a runaway
            // (bit-sync locked onto noise, no terminator ever appears) as
            // "not a frame" rather than growing forever.
            if body.len() > 4096 {
                return None;
            }
        }
        Some(body)
    } else if first == ETX || first == ETB {
        bcc ^= first;
        pos += 8;
        None
    } else {
        return None;
    };

    let (bcc_byte, _) = char_at(bits, pos)?;
    let bcc_ok = bcc_byte == bcc;

    Some(AcarsMessage {
        mode,
        tail: tail_raw.trim_start_matches('.').to_string(),
        tech_ack,
        label,
        block_id,
        text,
        bcc_ok,
        parity_errors,
        freq_mhz: 0.0,
        timestamp_ms: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acars::msk::encode_bit_samples;

    fn encode_char(byte: u8) -> [bool; 8] {
        let mut bits = [false; 8];
        for k in 0..7 {
            bits[k] = (byte >> k) & 1 == 1;
        }
        let ones = byte.count_ones();
        bits[7] = ones % 2 == 0; // make total ones odd (odd parity)
        bits
    }

    fn samples_for(chars: &[u8]) -> Vec<f64> {
        let mut bits = Vec::new();
        for &c in chars {
            bits.extend(encode_char(c));
        }
        let mut samples = Vec::new();
        for b in bits {
            samples.extend(encode_bit_samples(b, samples.len(), 1000.0));
        }
        samples
    }

    #[test]
    fn decodes_a_synthetic_message_with_text() {
        let mut chars: Vec<u8> = vec![SYN, SYN, SOH];
        chars.push(b'2'); // mode
        chars.extend(b".N12345"); // 7-char tail, dot-padded
        chars.push(b'_'); // tech ack
        chars.extend(b"H1"); // label
        chars.push(b'A'); // block id
        chars.push(STX);
        chars.extend(b"TEST MSG");
        chars.push(ETX);

        let mut bcc = 0u8;
        for &c in &chars[3..] {
            bcc ^= c;
        }
        chars.push(bcc);

        let samples = samples_for(&chars);
        let msg = decode_burst(&samples).expect("should sync and decode");
        assert_eq!(msg.mode, "2");
        assert_eq!(msg.tail, "N12345");
        assert_eq!(msg.label, "H1");
        assert_eq!(msg.block_id, "A");
        assert_eq!(msg.text.as_deref(), Some("TEST MSG"));
        assert!(msg.bcc_ok);
        assert_eq!(msg.parity_errors, 0);
    }

    #[test]
    fn decodes_a_synthetic_message_with_no_text_body() {
        let mut chars: Vec<u8> = vec![SYN, SYN, SOH];
        chars.push(b'2');
        chars.extend(b".N54321");
        chars.push(b'_');
        chars.extend(b"Q0");
        chars.push(b'1');
        chars.push(ETB);
        let mut bcc = 0u8;
        for &c in &chars[3..] {
            bcc ^= c;
        }
        chars.push(bcc);

        let samples = samples_for(&chars);
        let msg = decode_burst(&samples).expect("should sync and decode");
        assert_eq!(msg.tail, "N54321");
        assert_eq!(msg.text, None);
        assert!(msg.bcc_ok);
    }

    #[test]
    fn skips_leading_noise_before_the_sync_word() {
        let noise = [true, false, true, true, false, false, true, false, true, false];
        let mut chars: Vec<u8> = vec![SYN, SYN, SOH];
        chars.push(b'2');
        chars.extend(b".N11111");
        chars.push(b'_');
        chars.extend(b"SA");
        chars.push(b'1');
        chars.push(ETX);
        let mut bcc = 0u8;
        for &c in &chars[3..] {
            bcc ^= c;
        }
        chars.push(bcc);

        let mut bits = noise.to_vec();
        for &c in &chars {
            bits.extend(encode_char(c));
        }
        let mut samples = Vec::new();
        for b in bits {
            samples.extend(encode_bit_samples(b, samples.len(), 1000.0));
        }
        let msg = decode_burst(&samples).expect("should find sync past the noise");
        assert_eq!(msg.tail, "N11111");
    }

    #[test]
    fn flags_a_corrupted_block_check() {
        let mut chars: Vec<u8> = vec![SYN, SYN, SOH];
        chars.push(b'2');
        chars.extend(b".N99999");
        chars.push(b'_');
        chars.extend(b"H1");
        chars.push(b'A');
        chars.push(ETX);
        chars.push(0x00); // deliberately wrong BCC

        let samples = samples_for(&chars);
        let msg = decode_burst(&samples).expect("frame structure still parses");
        assert!(!msg.bcc_ok);
    }
}
