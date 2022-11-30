#![no_std]
#![feature(asm_experimental_arch)]

#[cfg(target_arch = "avr")]
use core::arch::asm;

use nano_fmt::{NanoDisplay, NanoWrite};
pub use nano_fmt_macro::{write, P};

/// C-style string stored in program memory.
/// It is only suitable for formatted output.
#[derive(Clone, Copy)]
pub struct PStr(*const u8);

impl PStr {
    pub unsafe fn new(ptr: *const u8) -> Self {
        Self(ptr)
    }
}

impl NanoDisplay for PStr {
    fn fmt<F: NanoWrite>(self, f: &mut F) {
        let mut p = self.0;

        loop {
            let b: u8;

            unsafe {
                #[cfg(target_arch = "avr")]
                asm! {
                    "lpm {b}, Z+",
                    b = out(reg) b,
                    inout("Z") p,
                    // Technically, this does access program memory, but it should
                    // not in any way influence the program.
                    options(pure, nomem, preserves_flags, nostack),
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
