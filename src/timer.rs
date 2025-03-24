use crate::{clock::BoardClock, clock::Clock, hal};

/// A timer using `TC1` peripheral.
///
/// Triggers `TIMER1_COMPA` interrupt.
pub struct Timer;

impl Timer {
    /// Create a new timer instance.
    ///
    /// The created timer is running with 1 second period.
    pub fn new(p: hal::pac::TC1) -> Self {
        // Calculate the maximum counter value for 1 second period.
        let counter_max = (BoardClock::FREQ / 256) as u16;

        // Set up TIMER1 for 1 second interrupts.
        // CTC mode, prescaler = 256 (32us ticks).
        p.tccr1b.write(|w| {
            let w = w.wgm1().bits(0b01);
            w.wgm1().bits(0b01).cs1().prescale_256()
        });

        p.ocr1a.write(|w| w.bits(counter_max));
        // TIMER1 overflow interrupt enable.
        p.timsk.write(|w| w.ocie1a().set_bit());
        Self {}
    }
}
