use attiny_hal as hal;
use hal::{
    pac::TC0,
    port::{mode::Output, Pin, PB2},
};

pub struct Beeper {
    timer: TC0,
}

impl Beeper {
    #[must_use]
    pub fn new(_pin: Pin<Output, PB2>, timer: TC0) -> Self {
        // Set up TIMER0 for tone generation.
        // Toggle OC0A (pin PB2) on compare match and set timer to CTC mode.
        timer
            .tccr0a
            .write(|w| w.com0a().match_toggle().wgm0().ctc());
        // Stop TIMER0 (no sound).
        timer.tccr0b.reset();

        Self { timer }
    }

    /// Turns on the beeper.
    pub fn turn_on(&mut self) {
        // enable OCR0A output on pin PB2
        self.timer.tccr0a.modify(|_, w| w.com0a().match_toggle());
        // Set prescaler to clk/8 (1Mhz) or 1us/count.
        self.timer.tccr0b.modify(|_, w| w.cs0().prescale_8());
        // 160 = toggle OCR0A every 160ms, period = 320us, freq= 3.125kHz
        self.timer.ocr0a.write(|w| unsafe { w.bits(160) });
    }

    /// Turns off the beeper.
    pub fn turn_off(&mut self) {
        // Disable TIMER0 since we're no longer using it.
        self.timer.tccr0b.reset();
        // Disconnect OCR0A from TIMER0, this avoids occasional HVPS whine after beep.
        self.timer.tccr0a.modify(|_, w| w.com0a().disconnected());
    }
}
