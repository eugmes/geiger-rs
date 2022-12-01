#![no_std]
#![feature(asm_experimental_arch)]

#[cfg(target_arch = "avr")]
use core::arch::asm;
use core::num::NonZeroU8;

use cfg_if::cfg_if;
use nano_fmt::{NanoDisplay, NanoWrite};
pub use nano_fmt_macro::{write, P};

/// C-style string stored in program memory.
/// It is only suitable for formatted output.
#[derive(Clone, Copy)]
pub struct PStr(*const u8);

impl PStr {
    /// Construct a new instance of a string.
    ///
    /// # Safety
    /// `ptr` should point to the beginning of a nul-terminated string.
    /// The string should stay constant during program execution.
    /// On AVR, the string should reside in program memory.
    #[must_use]
    pub unsafe fn new(ptr: *const u8) -> Self {
        Self(ptr)
    }
}

impl IntoIterator for PStr {
    type Item = NonZeroU8;

    type IntoIter = Iter;

    fn into_iter(self) -> Self::IntoIter {
        Iter(self.0)
    }
}

pub struct Iter(*const u8);

impl Iterator for Iter {
    type Item = NonZeroU8;

    fn next(&mut self) -> Option<Self::Item> {
        let b: u8;

        unsafe {
            cfg_if! {
                if #[cfg(target_arch = "avr")] {
                    asm! {
                        "lpm {b}, Z+",
                        b = out(reg) b,
                        inout("Z") self.0,
                        // Technically, this does access program memory, but it should
                        // not in any way influence the program.
                        options(pure, nomem, preserves_flags, nostack),
                    };
                } else {
                    b = *p;
                    p = p.add(1);
                }
            }
        }
        NonZeroU8::new(b)
    }
}

impl NanoDisplay for PStr {
    fn fmt<F: NanoWrite>(self, f: &mut F) {
        for b in self {
            f.write_byte(b.get());
        }
    }
}
