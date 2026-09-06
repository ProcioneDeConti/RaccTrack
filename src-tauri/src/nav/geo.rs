//! Great-circle helpers for the VOR feature — the geometric radial a receiver
//! *should* read, and DME slant range. Kept local (small, and the codebase
//! already scatters tiny geo helpers rather than sharing one module).

const R_NM: f64 = 3440.065;

pub fn wrap360(deg: f64) -> f64 {
    let d = deg % 360.0;
    if d < 0.0 {
        d + 360.0
    } else {
        d
    }
}

/// Smallest signed difference `a - b`, in (-180, 180].
pub fn angle_diff(a: f64, b: f64) -> f64 {
    let d = wrap360(a - b);
    if d > 180.0 {
        d - 360.0
    } else {
        d
    }
}

pub fn haversine_nm(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * R_NM * a.sqrt().asin()
}

/// Initial true bearing from point 1 to point 2, degrees 0–360.
pub fn true_bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dl = (lon2 - lon1).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    wrap360(y.atan2(x).to_degrees())
}

/// The magnetic radial FROM the station TO the observer — what a VOR receiver
/// would display. `station_variation_deg` is east-positive (OurAirports
/// `slaved_variation_deg`): magnetic = true − east variation.
pub fn geometric_radial(
    sta_lat: f64,
    sta_lon: f64,
    obs_lat: f64,
    obs_lon: f64,
    station_variation_deg: f64,
) -> f64 {
    wrap360(true_bearing(sta_lat, sta_lon, obs_lat, obs_lon) - station_variation_deg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radial_is_true_bearing_minus_east_variation() {
        // Observer due true-north of the station; +10° east variation → the
        // magnetic radial reads 350.
        let r = geometric_radial(40.0, -100.0, 41.0, -100.0, 10.0);
        assert!((r - 350.0).abs() < 0.5, "got {r}");
    }

    #[test]
    fn angle_diff_wraps() {
        assert!((angle_diff(350.0, 10.0) - -20.0).abs() < 1e-9);
        assert!((angle_diff(10.0, 350.0) - 20.0).abs() < 1e-9);
    }

}
