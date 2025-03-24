use nano_fmt::NanoWrite;

use crate::clock::BoardClock;
use crate::hal::port::{self, PD0, PD1};

/// Wrapper around USART that can be used for output formatting.
pub struct Usart0 {
    p: attiny_hal::pac::USART,
}

type Baudrate = avr_hal_generic::usart::Baudrate<BoardClock>;

impl Usart0 {
    /// Create new instance from raw hardware.
    #[must_use]
    pub fn new<IMODE: port::mode::InputMode>(
        p: attiny_hal::pac::USART,
        _rx: port::Pin<port::mode::Input<IMODE>, PD0>,
        _tx: port::Pin<port::mode::Output, PD1>,
        baudrate: u32,
    ) -> Self {
        let baudrate = Baudrate::new(baudrate);
        p.ubrrh.write(|w| w.bits((baudrate.ubrr >> 8) as u8));
        p.ubrrl.write(|w| w.bits((baudrate.ubrr & 0xFF) as u8));
        p.ucsra.write(|w| w.u2x().bit(baudrate.u2x));

        // Enable receiver and transmitter.
        p.ucsrb.write(|w| w.txen().set_bit().rxen().set_bit());

        Self { p }
    }
}

impl NanoWrite for Usart0 {
    fn write_byte(&mut self, b: u8) {
        while self.p.ucsra.read().udre().bit_is_clear() {}

        self.p.udr.write(|w| w.bits(b));
    }
}
