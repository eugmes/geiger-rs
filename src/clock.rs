use crate::hal;

pub use hal::clock::Clock;

/// Board clock rate.
pub type BoardClock = hal::clock::MHz8;
