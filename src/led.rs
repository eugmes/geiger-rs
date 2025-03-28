use crate::hal::port::{Pin, PinOps, mode::Output};

pub struct Led<P> {
    p: P,
}

impl<PIN> Led<Pin<Output, PIN>>
where
    PIN: PinOps,
{
    pub fn new(p: Pin<Output, PIN>) -> Self {
        Self { p }
    }

    pub fn turn_on(&mut self) {
        self.p.set_high();
    }

    pub fn turn_off(&mut self) {
        self.p.set_low();
    }
}
