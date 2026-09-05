//! UAT ADS-B message field extraction (RTCA DO-282B "Basic"/"Long" ADS-B
//! messages). Bit offsets and scale factors are taken from `dump978`'s
//! `uat_decode.c`, not rederived from the spec text.
//!
//! Scope cut: only the state-vector fields common to every message type
//! (position/altitude/velocity) plus callsign/emitter-category/emergency
//! (present only in a Long frame's Mode-Status region, bytes 17-23) are
//! parsed. Full per-mdb-type field semantics (auxiliary state vector, NIC
//! supplement bits, etc.) are not — this is enough to plot an aircraft and
//! merge it into the existing list, not a complete UAT decoder.

const BASE40: &[u8; 40] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ  ..";

#[derive(Debug, Clone, Default)]
pub struct UatAdsb {
    pub icao: String, // lowercase hex, matching the rest of the app's hex convention
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub altitude_ft: Option<f64>,
    pub altitude_is_geometric: bool,
    pub on_ground: bool,
    pub ground_speed_kt: Option<f64>,
    pub track_deg: Option<f64>,
    pub vert_rate_fpm: Option<f64>,
    pub callsign: Option<String>,
    pub emitter_category: Option<u8>,
    /// `None` when the message reports no emergency; otherwise the same
    /// human-readable strings `ingest::rtlsdr` uses for 1090ES's identical
    /// 3-bit emergency/priority status encoding, so both feed the same
    /// `alerts.rs` emergency check the same way.
    pub emergency_status: Option<String>,
}

/// `payload` is the FEC-corrected data portion only (18 or 34 bytes — no
/// parity). Returns `None` if it's too short to contain even the fields
/// common to every message type (shouldn't happen for a valid short/long
/// frame, but keeps this defensive against a future caller mistake).
pub fn parse(payload: &[u8]) -> Option<UatAdsb> {
    if payload.len() < 17 {
        return None;
    }

    let mdb_type = (payload[0] >> 3) & 0x1f;
    let address_qualifier = payload[0] & 0x07;
    let icao = format!("{:06x}", u32::from_be_bytes([0, payload[1], payload[2], payload[3]]));

    let raw_lat =
        ((payload[4] as u32) << 15) | ((payload[5] as u32) << 7) | ((payload[6] as u32) >> 1);
    let raw_lon = (((payload[6] as u32) & 0x01) << 23)
        | ((payload[7] as u32) << 15)
        | ((payload[8] as u32) << 7)
        | ((payload[9] as u32) >> 1);
    let (lat, lon) = if raw_lat == 0 && raw_lon == 0 {
        (None, None)
    } else {
        let mut lat = raw_lat as f64 * 360.0 / 16_777_216.0;
        if lat > 90.0 {
            lat -= 180.0;
        }
        let mut lon = raw_lon as f64 * 360.0 / 16_777_216.0;
        if lon > 180.0 {
            lon -= 360.0;
        }
        (Some(lat), Some(lon))
    };
    let altitude_is_geometric = payload[9] & 1 == 1;

    let raw_alt = ((payload[10] as u32) << 4) | (((payload[11] as u32) & 0xf0) >> 4);
    let altitude_ft = (raw_alt != 0).then(|| (raw_alt as f64 - 1.0) * 25.0 - 1000.0);

    // 0=subsonic airborne, 1=supersonic airborne, 2=on ground, 3=reserved.
    let airground_state = (payload[12] >> 6) & 0x03;
    let on_ground = airground_state == 2;
    let supersonic = airground_state == 1;

    let raw_ns = (((payload[12] as u32) & 0x1f) << 6) | (((payload[13] as u32) & 0xfc) >> 2);
    let ns_vel = decode_velocity(raw_ns, supersonic);
    let raw_ew = (((payload[13] as u32) & 0x03) << 9)
        | ((payload[14] as u32) << 1)
        | (((payload[15] as u32) & 0x80) >> 7);
    let ew_vel = decode_velocity(raw_ew, supersonic);
    let (ground_speed_kt, track_deg) = match (ns_vel, ew_vel) {
        (Some(ns), Some(ew)) => {
            let gs = (ns * ns + ew * ew).sqrt();
            let mut track = ew.atan2(ns).to_degrees();
            if track < 0.0 {
                track += 360.0;
            }
            (Some(gs), Some(track))
        }
        _ => (None, None),
    };

    let raw_vvel = (((payload[15] as u32) & 0x7f) << 4) | (((payload[16] as u32) & 0xf0) >> 4);
    let vert_rate_fpm = (raw_vvel & 0x1ff != 0).then(|| {
        let magnitude = ((raw_vvel & 0x1ff) as f64 - 1.0) * 64.0;
        if raw_vvel & 0x200 != 0 {
            -magnitude
        } else {
            magnitude
        }
    });

    // Callsign/emitter-category/emergency live in the Mode Status region
    // (bytes 17-23, so only present at all in a Long frame), and only for
    // the mdb_types that actually carry Mode Status rather than an
    // Auxiliary State Vector in that same byte range (see the module doc's
    // mdb_type table) — types 1 and 3.
    let has_mode_status = matches!(mdb_type, 1 | 3);
    let (callsign, emitter_category, emergency_status) = if has_mode_status && payload.len() >= 24 {
        let mut digits = Vec::with_capacity(9);
        for word_start in [17usize, 19, 21] {
            let v = ((payload[word_start] as u32) << 8) | payload[word_start + 1] as u32;
            digits.push((v / 1600) % 40);
            digits.push((v / 40) % 40);
            digits.push(v % 40);
        }
        let emitter_category = digits[0] as u8;
        let callsign: String = digits[1..9]
            .iter()
            .map(|&d| BASE40[d as usize] as char)
            .collect::<String>()
            .trim_end()
            .to_string();
        let callsign = (!callsign.is_empty()).then_some(callsign);
        let emergency_status = payload.get(23).map(|&b| (b >> 5) & 0x07).and_then(emergency_text);
        (callsign, Some(emitter_category), emergency_status)
    } else {
        (None, None, None)
    };

    let _ = address_qualifier; // not surfaced yet — see module doc scope cut
    Some(UatAdsb {
        icao,
        lat,
        lon,
        altitude_ft,
        altitude_is_geometric,
        on_ground,
        ground_speed_kt,
        track_deg,
        vert_rate_fpm,
        callsign,
        emitter_category,
        emergency_status,
    })
}

/// Same 3-bit emergency/priority status enumeration as 1090ES's BDS 6,1
/// (ICAO Annex 10) — 0 is "no emergency" (`None`), 1-7 map to the same
/// strings `adsb_deku`'s `EmergencyState` `Display` produces (underscored),
/// which is what `ingest::rtlsdr::apply_frame` already feeds into
/// `RawAircraft.emergency` and `alerts.rs` checks against.
fn emergency_text(code: u8) -> Option<String> {
    let s = match code {
        0 => return None,
        1 => "general",
        2 => "lifeguard",
        3 => "minimum_fuel",
        4 => "no_communication",
        5 => "unlawful_interference",
        6 => "downed_aircraft",
        _ => "reserved",
    };
    Some(s.to_string())
}

fn decode_velocity(raw: u32, supersonic: bool) -> Option<f64> {
    if raw & 0x3ff == 0 {
        return None;
    }
    let mut v = (raw & 0x3ff) as f64 - 1.0;
    if raw & 0x400 != 0 {
        v = -v;
    }
    if supersonic {
        v *= 4.0;
    }
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a 34-byte Long-frame payload with the given field values,
    /// leaving everything else zeroed.
    fn payload_with(
        lat_deg: f64,
        lon_deg: f64,
        alt_ft: f64,
        ns: i32,
        ew: i32,
        vrate: i32,
        callsign: &str,
    ) -> [u8; 34] {
        let mut p = [0u8; 34];
        p[0] = 1 << 3; // mdb_type=1 (HDR+SV+MS+AUXSV) so Mode Status parses
        p[1] = 0xa6;
        p[2] = 0xa6;
        p[3] = 0xfe;

        // raw_lat is 23 bits (b4<<15 | b5<<7 | b6>>1 — only 8+8+7 bits of
        // storage exist for it); raw_lon is the full 24 bits (gets an extra
        // top bit from b6's bit0). Matches `parse()`'s extraction exactly,
        // just inverted.
        let raw_lat = ((lat_deg.rem_euclid(360.0)) * 16_777_216.0 / 360.0).round() as u32 & 0x7f_ffff;
        p[4] = (raw_lat >> 15) as u8;
        p[5] = (raw_lat >> 7) as u8;

        let raw_lon = ((lon_deg.rem_euclid(360.0)) * 16_777_216.0 / 360.0).round() as u32 & 0xff_ffff;
        p[6] = (((raw_lat & 0x7f) as u8) << 1) | ((raw_lon >> 23) as u8 & 0x01);
        p[7] = (raw_lon >> 15) as u8;
        p[8] = (raw_lon >> 7) as u8;
        p[9] = ((raw_lon & 0x7f) as u8) << 1; // + altitude-type bit0 = 0 (baro)

        let raw_alt = (((alt_ft + 1000.0) / 25.0) + 1.0).round() as u32;
        p[10] = (raw_alt >> 4) as u8;
        p[11] = ((raw_alt & 0x0f) << 4) as u8;

        p[12] = 0; // airborne subsonic
        let raw_ns = if ns < 0 { 0x400 | ((-ns as u32 + 1) & 0x3ff) } else { (ns as u32 + 1) & 0x3ff };
        p[12] |= (raw_ns >> 6) as u8 & 0x1f;
        p[13] = ((raw_ns & 0x3f) << 2) as u8;
        let raw_ew = if ew < 0 { 0x400 | ((-ew as u32 + 1) & 0x3ff) } else { (ew as u32 + 1) & 0x3ff };
        p[13] |= (raw_ew >> 9) as u8 & 0x03;
        p[14] = (raw_ew >> 1) as u8;
        p[15] = ((raw_ew & 0x01) << 7) as u8;

        let raw_vvel = if vrate < 0 {
            0x200 | (((-vrate / 64) as u32 + 1) & 0x1ff)
        } else {
            ((vrate / 64) as u32 + 1) & 0x1ff
        };
        p[15] |= (raw_vvel >> 4) as u8 & 0x7f;
        p[16] = ((raw_vvel & 0x0f) << 4) as u8;

        // Callsign: pad to 8 chars with the alphabet's own space code (36),
        // split into 3 base-40 words (word0's first digit is
        // emitter_category, left 0 here).
        let mut chars = vec![36u32; 8];
        for (i, c) in callsign.bytes().take(8).enumerate() {
            chars[i] = BASE40.iter().position(|&b| b == c).unwrap_or(36) as u32;
        }
        let digits = [0u32, chars[0], chars[1], chars[2], chars[3], chars[4], chars[5], chars[6], chars[7]];
        for (w, word_start) in [17usize, 19, 21].into_iter().enumerate() {
            let d0 = digits[w * 3];
            let d1 = digits[w * 3 + 1];
            let d2 = digits[w * 3 + 2];
            let v = d0 * 1600 + d1 * 40 + d2;
            p[word_start] = (v >> 8) as u8;
            p[word_start + 1] = (v & 0xff) as u8;
        }
        p
    }

    #[test]
    fn decodes_position_altitude_and_velocity() {
        let p = payload_with(40.0, -105.0, 35000.0, 300, 0, 640, "N528DN");
        let m = parse(&p).expect("parses");
        assert_eq!(m.icao, "a6a6fe");
        assert!((m.lat.unwrap() - 40.0).abs() < 0.001);
        assert!((m.lon.unwrap() - (-105.0)).abs() < 0.001);
        assert!((m.altitude_ft.unwrap() - 35000.0).abs() < 25.0);
        assert!(!m.altitude_is_geometric);
        assert!(!m.on_ground);
        assert!((m.ground_speed_kt.unwrap() - 300.0).abs() < 1.0);
        assert!((m.track_deg.unwrap() - 0.0).abs() < 1.0);
        assert!((m.vert_rate_fpm.unwrap() - 640.0).abs() < 64.0);
        assert_eq!(m.callsign.as_deref(), Some("N528DN"));
    }

    #[test]
    fn zero_lat_lon_is_treated_as_no_position() {
        let p = payload_with(0.0, 0.0, 10000.0, 100, 100, 0, "");
        let m = parse(&p).expect("parses");
        assert_eq!(m.lat, None);
        assert_eq!(m.lon, None);
    }

    #[test]
    fn short_frame_payload_has_no_callsign_fields() {
        let long = payload_with(10.0, 10.0, 5000.0, 50, 50, 0, "TEST123");
        let short = &long[..18];
        let m = parse(short).expect("parses");
        assert_eq!(m.callsign, None);
        assert_eq!(m.emitter_category, None);
        assert!(m.lat.is_some()); // position still decodes from the short payload
    }
}
