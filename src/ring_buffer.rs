use core::mem;

/// Fixed size ring buffer.
pub struct RingBuffer<const SIZE: usize> {
    samples: [u8; SIZE],
    index: u8,
}

impl<const SIZE: usize> RingBuffer<SIZE> {
    /// Create a new buffer filled with zeroes.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            samples: [0; SIZE],
            index: 0,
        }
    }

    /// Put a new value into the buffer returning the discarded value.
    pub fn put(&mut self, value: u8) -> u8 {
        debug_assert!((self.index as usize) < self.samples.len());
        // SAFETY: `self.index` is always in bounds of `self.samples`.
        let elem = unsafe { self.samples.get_unchecked_mut(self.index as usize) };
        let old_value = mem::replace(elem, value);

        self.index = if self.index as usize == SIZE - 1 {
            0
        } else {
            self.index + 1
        };
        old_value
    }

    /// Returns iterator over values in the buffer.
    #[must_use]
    pub fn iter(&self) -> Iter<SIZE> {
        Iter {
            samples: &self.samples,
            index: self.index,
        }
    }
}

/// Iterator over ring buffer data.
pub struct Iter<'a, const SIZE: usize> {
    samples: &'a [u8; SIZE],
    index: u8,
}

impl<const SIZE: usize> Iterator for Iter<'_, SIZE> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        self.index = if self.index == 0 {
            (SIZE - 1) as u8
        } else {
            self.index - 1
        };
        debug_assert!((self.index as usize) < self.samples.len());
        // SAFETY: `self.index` is always in bounds of `self.samples`.
        let elem = unsafe { self.samples.get_unchecked(self.index as usize) };
        Some(*elem)
    }
}
