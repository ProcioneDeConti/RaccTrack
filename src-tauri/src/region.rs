//! North America coverage region. Mirror of `src/lib/map/region.ts` — keep the
//! numbers identical in both files.

use serde::{Deserialize, Serialize};

pub const WEST: f64 = -172.0;
pub const SOUTH: f64 = 7.0;
pub const EAST: f64 = -52.0;
pub const NORTH: f64 = 72.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Area {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl Area {
    pub const NORTH_AMERICA: Area = Area {
        west: WEST,
        south: SOUTH,
        east: EAST,
        north: NORTH,
    };

    /// Intersect with the North America box. Never returns an inverted area.
    pub fn clamped(&self) -> Area {
        let west = self.west.max(WEST);
        let south = self.south.max(SOUTH);
        let east = self.east.min(EAST);
        let north = self.north.min(NORTH);
        Area {
            west,
            south,
            east: east.max(west + 0.01),
            north: north.max(south + 0.01),
        }
    }

    pub fn intersects_region(&self) -> bool {
        self.west < EAST && self.east > WEST && self.south < NORTH && self.north > SOUTH
    }

    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        lon >= self.west && lon <= self.east && lat >= self.south && lat <= self.north
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_inside_region() {
        let a = Area {
            west: -200.0,
            south: -10.0,
            east: 10.0,
            north: 90.0,
        }
        .clamped();
        assert!(a.west >= WEST && a.east <= EAST);
        assert!(a.south >= SOUTH && a.north <= NORTH);
    }

    #[test]
    fn pacific_does_not_intersect() {
        let a = Area {
            west: 150.0,
            south: 0.0,
            east: 170.0,
            north: 20.0,
        };
        assert!(!a.intersects_region());
    }
}
