#![no_std]

/// Writer trait for resource constrained systems.
pub trait NanoWrite {
    /// Write a byte to the writer.
    fn write_byte(&mut self, b: u8);
}

/// Display trait for resource constrained systems.
pub trait NanoDisplay {
    /// Write formatted representation of `self` to `f`.
    fn fmt<F: NanoWrite>(self, f: &mut F);
}

/// Implement NanoDisplay for an unsigned type.
macro_rules! display_unsigned {
    ($ty:ident) => {
        impl $crate::NanoDisplay for $ty {
            fn fmt<F: $crate::NanoWrite>(mut self, f: &mut F) {
                const MAX_POW10: $ty = <$ty>::pow(10, $ty::MAX.ilog10() as u32);

                let mut div = MAX_POW10;
                let mut print = false;

                while div > 0 {
                    let dig = (self / div) as u8;
                    self %= div;
                    div /= 10;

                    if !print && dig > 0 {
                        print = true;
                    }

                    if print || (div == 0) {
                        let b = 0x30 + dig;
                        f.write_byte(b);
                    }
                }
            }
        }
    };
}

display_unsigned!(u8);
display_unsigned!(u16);
display_unsigned!(u32);
display_unsigned!(u64);
display_unsigned!(u128);
display_unsigned!(usize);
