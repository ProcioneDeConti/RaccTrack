//! Position fix by crossing two or more VOR radials (lines of position).
//!
//! Flat-earth intersection on a local tangent plane: at the ranges a ground
//! receiver works over (< ~150 nm) the projection error is well under the
//! bearing error the DSP already carries, and it sidesteps the antipodal /
//! ambiguous cases of spherical great-circle intersection.

use crate::nav::geo::{haversine_nm, true_bearing};

/// A line of position: the observer lies somewhere along `true_bearing_deg`
/// from the station (magnetic radial + station variation).
pub struct Lop {
    pub lat: f64,
    pub lon: f64,
    pub true_bearing_deg: f64,
}

#[derive(Debug, Clone)]
pub struct PositionFix {
    pub lat: f64,
    pub lon: f64,
    /// Spread of the accepted pairwise crossings (nm) — the "cocked hat".
    pub uncertainty_nm: f64,
    /// The pairwise crossing points that fed the fix, for drawing.
    pub crossings: Vec<(f64, f64)>,
    pub lop_count: usize,
}

/// Below this crossing angle a pair is too near-parallel to trust.
const MIN_CROSS_DEG: f64 = 18.0;
/// A crossing farther than this from either station is a bad-radial artefact.
const MAX_RANGE_NM: f64 = 320.0;
/// Assumed 1-sigma radial error, for the 2-station uncertainty estimate.
const RADIAL_SIGMA_DEG: f64 = 3.0;

struct Local {
    p: (f64, f64),
    d: (f64, f64),
}

pub fn position_fix(lops: &[Lop]) -> Option<PositionFix> {
    if lops.len() < 2 {
        return None;
    }
    let ref_lat = lops.iter().map(|l| l.lat).sum::<f64>() / lops.len() as f64;
    let ref_lon = lops.iter().map(|l| l.lon).sum::<f64>() / lops.len() as f64;
    let coslat = ref_lat.to_radians().cos().abs().max(1e-6);
    let to_local = |lat: f64, lon: f64| ((lon - ref_lon) * 60.0 * coslat, (lat - ref_lat) * 60.0);
    let to_geo = |x: f64, y: f64| (ref_lat + y / 60.0, ref_lon + x / (60.0 * coslat));

    let locals: Vec<Local> = lops
        .iter()
        .map(|l| {
            let b = l.true_bearing_deg.to_radians();
            Local {
                p: to_local(l.lat, l.lon),
                d: (b.sin(), b.cos()), // (east, north), unit
            }
        })
        .collect();

    // (point, crossing angle deg, mean range from the two stations)
    let mut hits: Vec<((f64, f64), f64, f64)> = Vec::new();
    for i in 0..locals.len() {
        for j in (i + 1)..locals.len() {
            let (a, b) = (&locals[i], &locals[j]);
            let det = b.d.0 * a.d.1 - a.d.0 * b.d.1;
            if det.abs() < 1e-6 {
                continue;
            }
            let dot = (a.d.0 * b.d.0 + a.d.1 * b.d.1).clamp(-1.0, 1.0);
            let ang = dot.acos().to_degrees();
            let cross = ang.min(180.0 - ang);
            if cross < MIN_CROSS_DEG {
                continue;
            }
            let rx = b.p.0 - a.p.0;
            let ry = b.p.1 - a.p.1;
            let t = (-rx * b.d.1 + b.d.0 * ry) / det;
            let s = (a.d.0 * ry - a.d.1 * rx) / det;
            // Observer is *along* each radial (positive t/s); allow a little
            // slack for noise, reject a reciprocal-radial cross.
            if t < -15.0 || s < -15.0 || t.abs() > MAX_RANGE_NM || s.abs() > MAX_RANGE_NM {
                continue;
            }
            let px = a.p.0 + t * a.d.0;
            let py = a.p.1 + t * a.d.1;
            hits.push(((px, py), cross, (t.abs() + s.abs()) / 2.0));
        }
    }

    if hits.is_empty() {
        return None;
    }

    let cx = hits.iter().map(|h| h.0 .0).sum::<f64>() / hits.len() as f64;
    let cy = hits.iter().map(|h| h.0 .1).sum::<f64>() / hits.len() as f64;

    let uncertainty_nm = if hits.len() == 1 {
        let (_, cross, range) = hits[0];
        (RADIAL_SIGMA_DEG.to_radians() * range / cross.to_radians().sin()).max(0.1)
    } else {
        hits.iter()
            .map(|h| ((h.0 .0 - cx).powi(2) + (h.0 .1 - cy).powi(2)).sqrt())
            .fold(0.0_f64, f64::max)
            .max(0.1)
    };

    let (lat, lon) = to_geo(cx, cy);
    let crossings = hits.iter().map(|h| to_geo(h.0 .0, h.0 .1)).collect();

    Some(PositionFix {
        lat,
        lon,
        uncertainty_nm,
        crossings,
        lop_count: lops.len(),
    })
}

/// Distance + true bearing from a known point to a computed fix — for the
/// "≈ X nm from your saved location" readout.
pub fn offset_from(fix: &PositionFix, from_lat: f64, from_lon: f64) -> (f64, f64) {
    (
        haversine_nm(from_lat, from_lon, fix.lat, fix.lon),
        true_bearing(from_lat, from_lon, fix.lat, fix.lon),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::geo::{haversine_nm, true_bearing};

    fn lop_to(sta: (f64, f64), obs: (f64, f64)) -> Lop {
        Lop {
            lat: sta.0,
            lon: sta.1,
            true_bearing_deg: true_bearing(sta.0, sta.1, obs.0, obs.1),
        }
    }

    #[test]
    fn two_radials_cross_at_the_observer() {
        let obs = (40.0, -100.0);
        let lops = vec![
            lop_to((40.0, -101.0), obs), // due west, bearing ~090
            lop_to((41.0, -100.0), obs), // due north, bearing ~180
        ];
        let f = position_fix(&lops).expect("fix");
        assert!(haversine_nm(f.lat, f.lon, obs.0, obs.1) < 1.0, "{f:?}");
    }

    #[test]
    fn three_radials_give_a_small_cocked_hat() {
        let obs = (39.5, -98.0);
        let stations = [(40.5, -99.0), (39.0, -97.0), (38.8, -99.3)];
        let lops: Vec<Lop> = stations.iter().map(|&s| lop_to(s, obs)).collect();
        let f = position_fix(&lops).expect("fix");
        assert!(haversine_nm(f.lat, f.lon, obs.0, obs.1) < 2.0);
        assert!(f.uncertainty_nm < 2.0);
        assert_eq!(f.crossings.len(), 3);
    }

    #[test]
    fn near_parallel_radials_fail() {
        let obs = (40.0, -100.0);
        let lops = vec![
            lop_to((41.0, -100.05), obs),
            lop_to((41.2, -100.02), obs),
        ];
        assert!(position_fix(&lops).is_none());
    }

    #[test]
    fn one_lop_is_not_a_fix() {
        assert!(position_fix(&[lop_to((41.0, -100.0), (40.0, -100.0))]).is_none());
    }

    #[test]
    fn noisy_radials_still_land_close() {
        let obs = (40.0, -100.0);
        let stations = [(40.8, -101.0), (39.2, -99.2), (40.1, -98.7)];
        let errs = [3.0, -3.0, 2.0];
        let lops: Vec<Lop> = stations
            .iter()
            .zip(errs)
            .map(|(&s, e)| {
                let mut l = lop_to(s, obs);
                l.true_bearing_deg += e;
                l
            })
            .collect();
        let f = position_fix(&lops).expect("fix");
        assert!(haversine_nm(f.lat, f.lon, obs.0, obs.1) < 10.0, "{f:?}");
        assert!(f.uncertainty_nm > 0.0 && f.uncertainty_nm < 20.0, "{f:?}");
    }
}
