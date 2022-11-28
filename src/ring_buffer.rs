use core::mem;

/// Fixed size ring buffer.
pub struct RingBuffer<const SIZE: u8>
where
    [(); SIZE as usize]:,
{
    samples: [u8; SIZE as usize],
    index: u8,
}

impl<const SIZE: u8> RingBuffer<SIZE>
where
    [(); SIZE as usize]:,
{
    /// Create a new buffer filled with zeroes.
    pub const fn new() -> Self {
        Self {
            samples: [0; SIZE as usize],
            index: 0,
        }
    }

    /// Put a new value into the buffer returning the discarded value.
    pub fn put(&mut self, value: u8) -> u8 {
        let elem = self.samples.get_mut(self.index as usize).unwrap();
        let old_value = mem::replace(elem, value);

        self.index = if self.index == SIZE - 1 {
            0
        } else {
            self.index + 1
        };
        old_value
    }

    /// Return iterator over `count` values in the buffer.
    pub fn iter(&self, count: u8) -> Iter<SIZE> {
        Iter {
            samples: &self.samples,
            index: self.index,
            count,
        }
    }
}

/// Iterator over ring buffer data.
pub struct Iter<'a, const SIZE: u8>
where
    [(); SIZE as usize]:,
{
    samples: &'a [u8; SIZE as usize],
    index: u8,
    count: u8,
}

impl<const SIZE: u8> Iterator for Iter<'_, SIZE>
where
    [(); SIZE as usize]:,
{
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            return None;
        }
        self.count -= 1;

        debug_assert!((self.index as usize) < self.samples.len());
        self.index = if self.index == 0 {
            SIZE - 1
        } else {
            self.index - 1
        };
        let elem = self.samples.get(self.index as usize).unwrap();
        Some(*elem)
    }
}
