/// A Variable-length integer
use super::bits::{BitSize, BitStreamWriter};
use super::*;
use crate::*;
use core::fmt;
use core::num::NonZero;

/// A Variable-length integer
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VarLenInteger(NonZero<u32>);

impl VarLenInteger {
    /// # Safety
    ///
    /// The `value` must fit within the `size`.
    #[inline]
    pub const unsafe fn from_raw_parts(size: BitSize, value: u32) -> Self {
        Self(unsafe { NonZero::new_unchecked(value | (size.as_u32() << 24)) })
    }

    /// TODO: Remove this method in the future
    // #[deprecated(
    //     note = "Use `new_truncated` or `new_checked` instead. This method does not check the value range."
    // )]
    #[inline]
    pub const fn new(size: BitSize, value: u32) -> Self {
        unsafe { Self::from_raw_parts(size, value & 0x00ff_ffff) }
    }

    #[inline]
    pub const fn new_checked(size: BitSize, value: u32) -> Option<Self> {
        if value <= size.mask() {
            // Safety: The value is checked
            Some(unsafe { Self::from_raw_parts(size, value) })
        } else {
            None
        }
    }

    #[inline]
    pub const fn new_truncated(size: BitSize, value: u32) -> Self {
        // Safety: The value is truncated.
        unsafe { Self::from_raw_parts(size, value & size.mask()) }
    }

    #[inline]
    pub const fn with_bool(value: bool) -> Self {
        // Safety: The value is guaranteed to be 0 or 1
        unsafe { Self::from_raw_parts(BitSize::Bit1, value as u32) }
    }

    #[inline]
    pub const fn with_nibble(value: Nibble) -> Self {
        // Safety: The value is guaranteed to be in the range of Nibble
        unsafe { Self::from_raw_parts(BitSize::NIBBLE, value.as_u32()) }
    }

    #[inline]
    pub const fn with_byte(value: u8) -> Self {
        // Safety: The value is guaranteed to be in the range of u8
        unsafe { Self::from_raw_parts(BitSize::Bit8, value as u32) }
    }

    #[inline]
    pub const fn size(&self) -> BitSize {
        // Safety: The value is guaranteed at initialization
        unsafe { BitSize::new_unchecked((self.0.get() >> 24) as u8) }
    }

    #[inline]
    pub const fn value(&self) -> u32 {
        self.0.get() & 0xff_ff_ff
    }

    #[inline]
    pub const fn canonical_value(&self) -> u32 {
        self.value() & self.size().mask()
    }

    pub const fn reversed(&self) -> Self {
        let size = self.size();
        let value = self.0.get().reverse_bits() >> (32 - size.as_usize());
        // Safety: The value is guaranteed to be in the range of size.
        unsafe { Self::from_raw_parts(size, value) }
    }

    #[inline]
    pub fn reverse(&mut self) {
        self.0 = self.reversed().0;
    }

    #[inline]
    pub fn to_vec<T>(iter: T) -> Vec<u8>
    where
        T: Iterator<Item = VarLenInteger>,
    {
        let mut bs = BitStreamWriter::new();
        for ext_bit in iter {
            bs.push(ext_bit);
        }
        bs.into_bytes()
    }

    #[inline]
    pub fn into_vec<T>(iter: T) -> Vec<u8>
    where
        T: IntoIterator<Item = VarLenInteger>,
    {
        Self::to_vec(iter.into_iter())
    }

    #[inline]
    pub fn total_len<'a, T>(iter: T) -> usize
    where
        T: Iterator<Item = &'a Option<VarLenInteger>>,
    {
        (Self::total_bit_count(iter) + 7) / 8
    }

    #[inline]
    pub fn total_bit_count<'a, T>(iter: T) -> usize
    where
        T: Iterator<Item = &'a Option<VarLenInteger>>,
    {
        iter.fold(0, |a, v| match v {
            Some(v) => a + v.size() as usize,
            None => a,
        })
    }
}

impl From<bool> for VarLenInteger {
    #[inline]
    fn from(value: bool) -> Self {
        Self::with_bool(value)
    }
}

impl From<Nibble> for VarLenInteger {
    #[inline]
    fn from(value: Nibble) -> Self {
        Self::with_nibble(value)
    }
}

impl From<u8> for VarLenInteger {
    #[inline]
    fn from(value: u8) -> Self {
        Self::with_byte(value)
    }
}

impl fmt::Display for VarLenInteger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let size = self.size().as_usize();
        if let Some(width) = f.width() {
            if width > size {
                for _ in 0..width - size {
                    write!(f, " ")?;
                }
            }
        }
        for i in (0..size).rev() {
            let bit = self.value().wrapping_shr(i as u32) & 1;
            write!(f, "{}", bit)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_bits() {
        for (size, lhs, rhs) in [
            (8, 0x00, 0x00),
            (8, 0x03, 0xc0),
            (8, 0x55, 0xaa),
            (8, 0xc0, 0x03),
            (8, 0xf0, 0x0f),
            (8, 0xff, 0xff),
            (16, 0x0000, 0x0000),
            (16, 0x00ff, 0xff00),
            (16, 0x0f0f, 0xf0f0),
            (16, 0x1234, 0x2c48),
            (16, 0x3333, 0xcccc),
            (16, 0x5555, 0xaaaa),
            (16, 0xffff, 0xffff),
            (24, 0x000000, 0x000000),
            (24, 0x123456, 0x6a2c48),
            (24, 0x555555, 0xaaaaaa),
            (24, 0xcccccc, 0x333333),
            (24, 0xff0000, 0x0000ff),
            (24, 0xfff000, 0x000fff),
            (24, 0xffff00, 0x00ffff),
            (24, 0xffffff, 0xffffff),
        ] {
            let size = BitSize::new(size).unwrap();
            let lhs = VarLenInteger::new_checked(size, lhs).unwrap();
            let rhs = VarLenInteger::new_checked(size, rhs).unwrap();

            assert_eq!(lhs.reversed(), rhs);
            assert_eq!(lhs, rhs.reversed());

            assert_eq!(lhs.reversed().reversed(), lhs);
            assert_eq!(rhs.reversed().reversed(), rhs);
        }
    }
}
