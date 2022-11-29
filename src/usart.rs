pub use avr_hal_generic::usart::Baudrate;

use core::marker::PhantomData;

use attiny_hal::{
    clock::Clock,
    port::{PinOps, PD0, PD1},
};
use avr_hal_generic::port;

use crate::nano_fmt;

pub trait CharIO {
    type RX: PinOps;
    type TX: PinOps;

    fn raw_init<CLOCK: Clock>(&mut self, baudrate: Baudrate<CLOCK>);
    fn raw_write(&mut self, c: u8);
    fn raw_read(&mut self) -> Option<u8>;
}

impl CharIO for attiny_hal::pac::USART {
    type RX = PD0;
    type TX = PD1;

    fn raw_init<CLOCK: Clock>(&mut self, baudrate: Baudrate<CLOCK>) {
        self.ubrrh
            .write(|w| unsafe { w.bits((baudrate.ubrr >> 8) as u8) });
        self.ubrrl
            .write(|w| unsafe { w.bits((baudrate.ubrr & 0xFF) as u8) });
        self.ucsra.write(|w| w.u2x().bit(baudrate.u2x));

        // Enable receiver and transmitter.
        self.ucsrb.write(|w| w.txen().set_bit().rxen().set_bit());
    }

    fn raw_write(&mut self, c: u8) {
        while self.ucsra.read().udre().bit_is_clear() {}

        unsafe {
            self.udr.write(|w| w.bits(c));
        }
    }

    fn raw_read(&mut self) -> Option<u8> {
        if self.ucsra.read().rxc().bit_is_clear() {
            return None;
        }
        let c = self.udr.read().bits();
        Some(c)
    }
}

pub struct Usart0<USART, RX, TX> {
    p: USART,
    _rx: PhantomData<RX>,
    _tx: PhantomData<TX>,
}

impl<USART, RXPIN, TXPIN>
    Usart0<USART, port::Pin<port::mode::Input, RXPIN>, port::Pin<port::mode::Output, TXPIN>>
where
    USART: CharIO<RX = RXPIN, TX = TXPIN>,
    RXPIN: port::PinOps,
    TXPIN: port::PinOps,
{
    pub fn new<IMODE: port::mode::InputMode, CLOCK: Clock>(
        p: USART,
        _rx: port::Pin<port::mode::Input<IMODE>, RXPIN>,
        _tx: port::Pin<port::mode::Output, TXPIN>,
        baudrate: Baudrate<CLOCK>,
    ) -> Self {
        let mut usart = Self {
            p,
            _rx: PhantomData,
            _tx: PhantomData,
        };

        usart.p.raw_init(baudrate);
        usart
    }
}

impl<USART: CharIO, RX, TX> nano_fmt::NanoWrite for Usart0<USART, RX, TX> {
    fn write_byte(&mut self, b: u8) {
        self.p.raw_write(b);
    }
}
