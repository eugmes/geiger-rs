use crate::{clock::BoardClock, hal};

/// Delay implementation for the board.
pub type Delay = hal::delay::Delay<BoardClock>;
