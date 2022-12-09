use nano_fmt::{NanoDisplay, NanoWrite};

/// Fixed point value with 2 decimal digits.
#[derive(Clone, Copy)]
pub struct Fixed2(u32);

impl Fixed2 {
    /// Construct new value from scaled integer.
    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }
}

impl NanoDisplay for Fixed2 {
    fn fmt<F: NanoWrite>(self, f: &mut F) {
        let fract = (self.0 % 100) as u8;
        let integer = self.0 / 100;

        integer.fmt(f);

        f.write_byte(b'.');

        if fract < 10 {
            f.write_byte(b'0');
        }
        u32::from(fract).fmt(f);
    }
}
