#[cfg(target_arch = "avr")]
use core::arch::asm;

use crate::nano_fmt::{NanoDisplay, NanoWrite};

/// C-style string stored in program memory.
/// It is only suitable for formatted output.
#[derive(Clone, Copy)]
pub struct PStr(pub *const u8);

#[macro_export]
macro_rules! P {
    ($s:literal) => {
        {
            const SIZE: usize = $s.len() + 1;
            #[cfg_attr(target_arch = "avr", link_section = ".progmem.data")]
            static S: [u8; SIZE] = *concat_bytes!($s, b"\0");
            $crate::progmem::PStr(S.as_ptr() as *const u8)
        }
    };
}

impl NanoDisplay for PStr {
    fn fmt<F: NanoWrite>(self, f: &mut F) {
        let mut p = self.0;

        loop {
            let b: u8;

            unsafe {
                #[cfg(target_arch = "avr")]
                asm!{
                    "lpm {b}, Z+",
                    b = out(reg) b,
                    inout("Z") p,
                };
                #[cfg(not(target_arch = "avr"))]
                {
                    b = *p;
                    p = p.add(1);
                }
            }

            if b == 0 {
                break;
            }

            f.write_byte(b);
        }
    }
}
