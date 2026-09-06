//! RTL-SDR VOR navigation: decode bearing + ident from a tuned VOR and compare
//! against the geometric radial from a saved place.

pub mod fix;
pub mod geo;
pub mod morse;
pub mod vor;
pub mod vor_dsp;
