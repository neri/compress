//! A 4-bit value
use core::{
    fmt,
    mem::transmute,
    ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign},
};

/// A 4-bit value
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Nibble {
    #[default]
    V0 = 0,
    V1,
    V2,
    V3,
    V4,
    V5,
    V6,
    V7,
    V8,
    V9,
    V10,
    V11,
    V12,
    V13,
    V14,
    V15,
}

impl Nibble {
    pub const MIN: Self = Self::V0;

    pub const MAX: Self = Self::V15;

    #[inline]
    pub const fn new(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::V0),
            1 => Some(Self::V1),
            2 => Some(Self::V2),
            3 => Some(Self::V3),
            4 => Some(Self::V4),
            5 => Some(Self::V5),
            6 => Some(Self::V6),
            7 => Some(Self::V7),
            8 => Some(Self::V8),
            9 => Some(Self::V9),
            10 => Some(Self::V10),
            11 => Some(Self::V11),
            12 => Some(Self::V12),
            13 => Some(Self::V13),
            14 => Some(Self::V14),
            15 => Some(Self::V15),
            _ => None,
        }
    }

    /// # Safety
    ///
    /// UB if value is not in the range 0..=15
    #[inline]
    pub const unsafe fn new_unchecked(value: u8) -> Self {
        unsafe { transmute(value) }
    }

    #[inline]
    pub const fn new_truncated(value: u8) -> Self {
        // Safety: This is safe because the value is truncated to 4 bits
        unsafe { transmute(value & 15) }
    }

    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[inline]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    #[inline]
    pub const fn as_usize(self) -> usize {
        self as usize
    }

    #[inline]
    pub const fn clamp(self, min: Self, max: Self) -> Self {
        if (self as u8) < min as u8 {
            min
        } else if self as u8 > max as u8 {
            max
        } else {
            self
        }
    }

    #[inline]
    pub const fn min(self, other: Self) -> Self {
        if (self as u8) < (other as u8) {
            self
        } else {
            other
        }
    }

    #[inline]
    pub const fn max(self, other: Self) -> Self {
        if (self as u8) > (other as u8) {
            self
        } else {
            other
        }
    }

    #[inline]
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        match (self as u8).checked_add(rhs as u8) {
            Some(v) => Self::new(v),
            None => None,
        }
    }

    #[inline]
    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        match (self as u8).checked_sub(rhs as u8) {
            Some(v) => Self::new(v),
            None => None,
        }
    }

    #[inline]
    pub const fn checked_mul(self, rhs: Self) -> Option<Self> {
        match (self as u8).checked_mul(rhs as u8) {
            Some(v) => Self::new(v),
            None => None,
        }
    }

    #[inline]
    pub const fn checked_div(self, rhs: Self) -> Option<Self> {
        match (self as u8).checked_div(rhs as u8) {
            Some(v) => Self::new(v),
            None => None,
        }
    }

    #[inline]
    pub const fn checked_rem(self, rhs: Self) -> Option<Self> {
        match (self as u8).checked_rem(rhs as u8) {
            Some(v) => Self::new(v),
            None => None,
        }
    }

    #[inline]
    pub const fn wrapping_add(self, rhs: Self) -> Self {
        Self::new_truncated((self as u8).wrapping_add(rhs as u8))
    }

    #[inline]
    pub const fn wrapping_sub(self, rhs: Self) -> Self {
        Self::new_truncated((self as u8).wrapping_sub(rhs as u8))
    }

    #[inline]
    pub const fn wrapping_mul(self, rhs: Self) -> Self {
        Self::new_truncated((self as u8).wrapping_mul(rhs as u8))
    }

    #[inline]
    pub const fn saturating_add(self, rhs: Self) -> Self {
        // Safety: This is safe because we ensure the result does not exceed 15
        unsafe { Self::new_unchecked((self as u8).saturating_add(rhs as u8)) }.min(Self::MAX)
    }

    #[inline]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        // Safety: This is safe because we ensure the result does not go below 0
        unsafe { Self::new_unchecked((self as u8).saturating_sub(rhs as u8)) }
    }

    #[inline]
    pub const fn saturating_mul(self, rhs: Self) -> Self {
        // Safety: This is safe because we ensure the result does not exceed 15
        unsafe { Self::new_unchecked((self as u8).saturating_mul(rhs as u8)) }.min(Self::MAX)
    }

    #[inline]
    pub const fn bitand(self, rhs: Self) -> Self {
        Self::new_truncated((self as u8) & (rhs as u8))
    }

    #[inline]
    pub const fn bitor(self, rhs: Self) -> Self {
        Self::new_truncated((self as u8) | (rhs as u8))
    }

    #[inline]
    pub const fn bitxor(self, rhs: Self) -> Self {
        Self::new_truncated((self as u8) ^ (rhs as u8))
    }
}

impl BitAnd<Self> for Nibble {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        self.bitand(rhs)
    }
}

impl BitOr<Self> for Nibble {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        self.bitor(rhs)
    }
}

impl BitXor<Self> for Nibble {
    type Output = Self;

    #[inline]
    fn bitxor(self, rhs: Self) -> Self {
        self.bitxor(rhs)
    }
}

impl BitAndAssign<Self> for Nibble {
    #[inline]
    fn bitand_assign(&mut self, rhs: Self) {
        *self = self.bitand(rhs);
    }
}

impl BitOrAssign<Self> for Nibble {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.bitor(rhs);
    }
}

impl BitXorAssign<Self> for Nibble {
    #[inline]
    fn bitxor_assign(&mut self, rhs: Self) {
        *self = self.bitxor(rhs);
    }
}

impl fmt::Display for Nibble {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u8())
    }
}

impl fmt::Debug for Nibble {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Nibble({})", self.as_u8())
    }
}
