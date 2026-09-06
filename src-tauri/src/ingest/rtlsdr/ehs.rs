//! Mode S Enhanced Surveillance (EHS) — BDS 4,0 / 5,0 / 6,0 decoding from the
//! 56-bit Comm-B message field of a DF20/21 reply. From the public ICAO Annex
//! 10 Vol IV register tables (and cross-checked against the widely published
//! pyModeS test vectors); not derived from any GPL decoder.
//!
//! Comm-B replies don't say which register they carry, so `infer` parses the
//! MB field as each candidate register and only returns one when exactly a
//! single interpretation is self-consistent (status bits match value presence,
//! values in range). The caller additionally throws away any result whose
//! recovered ICAO isn't an aircraft we're already tracking via ADS-B.

/// MB is 56 bits — the 7 bytes at offset 4 of a DF20/21 message.
pub type Mb = [u8; 7];

/// bit `i` (1-indexed, MSB first).
fn bit(mb: &Mb, i: usize) -> u32 {
    ((mb[(i - 1) / 8] >> (7 - ((i - 1) % 8))) & 1) as u32
}

/// bits `first..=last` (1-indexed, inclusive) as an unsigned integer.
fn bits(mb: &Mb, first: usize, last: usize) -> u32 {
    (first..=last).fold(0u32, |acc, i| (acc << 1) | bit(mb, i))
}

/// `width`-bit two's-complement value starting at bit `first`.
fn signed(mb: &Mb, first: usize, width: usize) -> i32 {
    let raw = bits(mb, first, first + width - 1) as i32;
    if raw & (1 << (width - 1)) != 0 {
        raw - (1 << width)
    } else {
        raw
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bds50 {
    /// deg, + = right wing down
    pub roll: Option<f64>,
    pub true_track: Option<f64>,
    pub ground_speed: Option<f64>,
    /// deg/s, + = turning right
    pub track_rate: Option<f64>,
    pub true_airspeed: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bds60 {
    pub mag_heading: Option<f64>,
    pub ias: Option<f64>,
    pub mach: Option<f64>,
    /// ft/min, barometric
    pub baro_vrate: Option<f64>,
    /// ft/min, inertial
    pub inertial_vrate: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bds40 {
    pub mcp_alt_ft: Option<f64>,
    pub fms_alt_ft: Option<f64>,
    pub qnh_mb: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ehs {
    Bds40(Bds40),
    Bds50(Bds50),
    Bds60(Bds60),
}

fn parse_bds50(mb: &Mb) -> Option<Bds50> {
    let mut r = Bds50::default();
    // Roll: status bit 1, bits 2-10 (9-bit magnitude + sign at 2) — 10-bit
    // signed field bits 2..=11, LSB 45/256 deg.
    if bit(mb, 1) == 1 {
        let v = signed(mb, 2, 10) as f64 * (45.0 / 256.0);
        if v.abs() > 90.0 {
            return None;
        }
        r.roll = Some(v);
    } else if bits(mb, 2, 11) != 0 {
        return None;
    }
    // True track angle: status 12, bits 13-23 signed, LSB 90/512 deg.
    if bit(mb, 12) == 1 {
        let mut v = signed(mb, 13, 11) as f64 * (90.0 / 512.0);
        v = v.rem_euclid(360.0);
        r.true_track = Some(v);
    } else if bits(mb, 13, 23) != 0 {
        return None;
    }
    // Ground speed: status 24, bits 25-34, LSB 2 kt.
    if bit(mb, 24) == 1 {
        let v = bits(mb, 25, 34) as f64 * 2.0;
        if v > 2046.0 {
            return None;
        }
        r.ground_speed = Some(v);
    } else if bits(mb, 25, 34) != 0 {
        return None;
    }
    // Track angle rate: status 35, sign 36, 9-bit magnitude 37-45,
    // LSB 8/256 deg/s. All-ones magnitude = "not available".
    if bit(mb, 35) == 1 {
        if bits(mb, 37, 45) == 0x1FF {
            return None;
        }
        let v = signed(mb, 36, 10) as f64 * (8.0 / 256.0);
        if v.abs() > 16.0 {
            return None;
        }
        r.track_rate = Some(v);
    } else if bits(mb, 36, 45) != 0 {
        return None;
    }
    // True airspeed: status 46, bits 47-56, LSB 2 kt.
    if bit(mb, 46) == 1 {
        let v = bits(mb, 47, 56) as f64 * 2.0;
        if v > 2046.0 {
            return None;
        }
        r.true_airspeed = Some(v);
    } else if bits(mb, 47, 56) != 0 {
        return None;
    }
    // A reply with every field empty tells us nothing — treat as no-match.
    if r == Bds50::default() {
        return None;
    }
    // GS and TAS should be in the same ballpark when both are present.
    if let (Some(gs), Some(tas)) = (r.ground_speed, r.true_airspeed) {
        if (gs - tas).abs() > 200.0 {
            return None;
        }
    }
    Some(r)
}

fn parse_bds60(mb: &Mb) -> Option<Bds60> {
    let mut r = Bds60::default();
    // Magnetic heading: status 1, bits 2-12 signed, LSB 90/512 deg.
    if bit(mb, 1) == 1 {
        let v = (signed(mb, 2, 11) as f64 * (90.0 / 512.0)).rem_euclid(360.0);
        r.mag_heading = Some(v);
    } else if bits(mb, 2, 12) != 0 {
        return None;
    }
    // IAS: status 13, bits 14-23, LSB 1 kt.
    if bit(mb, 13) == 1 {
        let v = bits(mb, 14, 23) as f64;
        if v == 0.0 || v > 500.0 {
            return None;
        }
        r.ias = Some(v);
    } else if bits(mb, 14, 23) != 0 {
        return None;
    }
    // Mach: status 24, bits 25-34, LSB 2.048/512 = 0.004.
    if bit(mb, 24) == 1 {
        let v = bits(mb, 25, 34) as f64 * (2.048 / 512.0);
        if v == 0.0 || v > 1.0 {
            return None;
        }
        r.mach = Some(v);
    } else if bits(mb, 25, 34) != 0 {
        return None;
    }
    // Barometric altitude rate: status 35, sign 36, 9-bit magnitude 37-45,
    // LSB 32 ft/min. All-ones magnitude = "no information".
    if bit(mb, 35) == 1 {
        if bits(mb, 37, 45) == 0x1FF {
            return None;
        }
        let v = signed(mb, 36, 10) as f64 * 32.0;
        if v.abs() > 6000.0 {
            return None;
        }
        r.baro_vrate = Some(v);
    } else if bits(mb, 36, 45) != 0 {
        return None;
    }
    // Inertial vertical velocity: status 46, sign 47, 9-bit magnitude 48-56.
    if bit(mb, 46) == 1 {
        if bits(mb, 48, 56) == 0x1FF {
            return None;
        }
        let v = signed(mb, 47, 10) as f64 * 32.0;
        if v.abs() > 6000.0 {
            return None;
        }
        r.inertial_vrate = Some(v);
    } else if bits(mb, 47, 56) != 0 {
        return None;
    }
    if r == Bds60::default() {
        return None;
    }
    // The two vertical-rate sources should roughly agree.
    if let (Some(a), Some(b)) = (r.baro_vrate, r.inertial_vrate) {
        if (a - b).abs() > 2000.0 {
            return None;
        }
    }
    // IAS and Mach can't disagree wildly (rough tropopause-ish check).
    if let (Some(ias), Some(mach)) = (r.ias, r.mach) {
        if ias > 100.0 && mach < 0.1 {
            return None;
        }
    }
    Some(r)
}

fn parse_bds40(mb: &Mb) -> Option<Bds40> {
    let mut r = Bds40::default();
    // MCP/FCU selected altitude: status 1, bits 2-13, LSB 16 ft.
    if bit(mb, 1) == 1 {
        r.mcp_alt_ft = Some(bits(mb, 2, 13) as f64 * 16.0);
    } else if bits(mb, 2, 13) != 0 {
        return None;
    }
    // FMS selected altitude: status 14, bits 15-26, LSB 16 ft.
    if bit(mb, 14) == 1 {
        r.fms_alt_ft = Some(bits(mb, 15, 26) as f64 * 16.0);
    } else if bits(mb, 15, 26) != 0 {
        return None;
    }
    // Barometric pressure setting: status 27, bits 28-39, LSB 0.1 mb, +800.
    if bit(mb, 27) == 1 {
        let v = bits(mb, 28, 39) as f64 * 0.1 + 800.0;
        if !(745.0..=1150.0).contains(&v) {
            return None;
        }
        r.qnh_mb = Some(v);
    } else if bits(mb, 28, 39) != 0 {
        return None;
    }
    // Reserved bits 40-47 are always zero; 51-53 (target source) and 54-56
    // (target alt) can be non-zero, so only check the reserved run.
    if bits(mb, 40, 47) != 0 {
        return None;
    }
    if r == Bds40::default() {
        return None;
    }
    // A selected altitude should be a plausible flight altitude.
    for a in [r.mcp_alt_ft, r.fms_alt_ft].into_iter().flatten() {
        if a > 60000.0 {
            return None;
        }
    }
    Some(r)
}

/// Parse the MB field, returning a register only when exactly one of
/// {4,0; 5,0; 6,0} is self-consistent. `adsb_track_deg` (the aircraft's known
/// ADS-B ground track, if any) breaks a 5,0-vs-6,0 tie.
pub fn infer(mb: &Mb, adsb_track_deg: Option<f64>) -> Option<Ehs> {
    let b50 = parse_bds50(mb);
    let b60 = parse_bds60(mb);
    let b40 = parse_bds40(mb);

    match (b40, b50, b60) {
        (Some(v), None, None) => Some(Ehs::Bds40(v)),
        (None, Some(v), None) => Some(Ehs::Bds50(v)),
        (None, None, Some(v)) => Some(Ehs::Bds60(v)),
        (None, Some(v50), Some(v60)) => {
            // Prefer whichever heading-ish value sits closer to the ADS-B track.
            match adsb_track_deg {
                Some(trk) => {
                    let d50 = v50.true_track.map(|t| ang_diff(t, trk));
                    let d60 = v60.mag_heading.map(|h| ang_diff(h, trk));
                    match (d50, d60) {
                        (Some(a), Some(b)) if a <= b => Some(Ehs::Bds50(v50)),
                        (Some(_), Some(_)) => Some(Ehs::Bds60(v60)),
                        (Some(_), None) => Some(Ehs::Bds50(v50)),
                        (None, Some(_)) => Some(Ehs::Bds60(v60)),
                        (None, None) => None,
                    }
                }
                None => None, // genuinely ambiguous
            }
        }
        _ => None,
    }
}

fn ang_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 7 MB bytes are hex chars 8..22 of the full 28-hex-char message.
    fn mb(full_hex: &str) -> Mb {
        let bytes: Vec<u8> = (0..full_hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&full_hex[i..i + 2], 16).unwrap())
            .collect();
        bytes[4..11].try_into().unwrap()
    }

    /// Hand-built BDS 5,0 field values: roll +30, track +400, GS raw 200,
    /// track-rate +10, TAS raw 195 (all fields status = 1). See the comment
    /// block in the test for the bit packing.
    const SYNTH_BDS50: Mb = [0x83, 0xD3, 0x21, 0x32, 0x20, 0x54, 0xC3];

    #[test]
    fn bds50_round_trips() {
        let r = parse_bds50(&SYNTH_BDS50).unwrap();
        assert!((r.roll.unwrap() - 30.0 * 45.0 / 256.0).abs() < 1e-6);
        assert!((r.true_track.unwrap() - 400.0 * 90.0 / 512.0).abs() < 1e-6);
        assert_eq!(r.ground_speed, Some(400.0));
        assert!((r.track_rate.unwrap() - 10.0 * 8.0 / 256.0).abs() < 1e-6);
        assert_eq!(r.true_airspeed, Some(390.0));
    }

    // Real DF20/21 messages (pyModeS test corpus) — anchors the bit alignment.
    #[test]
    fn bds60_heading_and_speed() {
        let r = parse_bds60(&mb("A00004128F39F91A7E27C46ADC21")).unwrap();
        assert!((r.mag_heading.unwrap() - 42.71).abs() < 0.1, "{:?}", r.mag_heading);
        assert_eq!(r.ias, Some(252.0));
        assert!((r.mach.unwrap() - 0.42).abs() < 0.01, "{:?}", r.mach);
        assert_eq!(r.baro_vrate, Some(-1920.0));
        assert_eq!(r.inertial_vrate, Some(-1920.0));
    }

    #[test]
    fn bds40_selected_altitude() {
        let r = parse_bds40(&mb("A000029C85E42F313000007047D3")).unwrap();
        assert_eq!(r.mcp_alt_ft, Some(3008.0));
        assert_eq!(r.fms_alt_ft, Some(3008.0));
        assert!((r.qnh_mb.unwrap() - 1020.0).abs() < 0.2, "{:?}", r.qnh_mb);
    }

    #[test]
    fn infer_picks_the_unambiguous_register() {
        assert!(matches!(infer(&SYNTH_BDS50, None), Some(Ehs::Bds50(_))));
        assert!(matches!(
            infer(&mb("A00004128F39F91A7E27C46ADC21"), None),
            Some(Ehs::Bds60(_))
        ));
    }

    #[test]
    fn all_zero_mb_is_no_register() {
        assert_eq!(infer(&[0; 7], Some(180.0)), None);
    }
}
