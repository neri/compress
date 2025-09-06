//! Bit manipulation utilities
use super::VarLenInteger;
use crate::*;
use core::fmt;
use core::mem::transmute;
use num::Nibble;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BitSize {
    Bit1 = 1,
    Bit2,
    Bit3,
    Bit4,
    Bit5,
    Bit6,
    Bit7,
    Bit8,
    Bit9,
    Bit10,
    Bit11,
    Bit12,
    Bit13,
    Bit14,
    Bit15,
    Bit16,
    Bit17,
    Bit18,
    Bit19,
    Bit20,
    Bit21,
    Bit22,
    Bit23,
    Bit24,
}

impl BitSize {
    pub const NIBBLE: Self = Self::Bit4;

    pub const BYTE: Self = Self::Bit8;

    pub const OCTET: Self = Self::Bit8;

    /// Currently maximum size
    pub const MAX: Self = Self::Bit24;

    #[inline]
    pub const fn as_usize(&self) -> usize {
        *self as usize
    }

    #[inline]
    pub const fn as_u8(&self) -> u8 {
        *self as u8
    }

    #[inline]
    pub const fn as_u32(&self) -> u32 {
        *self as u32
    }

    #[inline]
    pub const fn new(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Bit1),
            2 => Some(Self::Bit2),
            3 => Some(Self::Bit3),
            4 => Some(Self::Bit4),
            5 => Some(Self::Bit5),
            6 => Some(Self::Bit6),
            7 => Some(Self::Bit7),
            8 => Some(Self::Bit8),
            9 => Some(Self::Bit9),
            10 => Some(Self::Bit10),
            11 => Some(Self::Bit11),
            12 => Some(Self::Bit12),
            13 => Some(Self::Bit13),
            14 => Some(Self::Bit14),
            15 => Some(Self::Bit15),
            16 => Some(Self::Bit16),
            17 => Some(Self::Bit17),
            18 => Some(Self::Bit18),
            19 => Some(Self::Bit19),
            20 => Some(Self::Bit20),
            21 => Some(Self::Bit21),
            22 => Some(Self::Bit22),
            23 => Some(Self::Bit23),
            24 => Some(Self::Bit24),
            _ => None,
        }
    }

    /// # Safety
    ///
    /// UB on invalid value
    #[inline]
    pub const unsafe fn new_unchecked(value: u8) -> Self {
        unsafe { transmute(value) }
    }

    #[inline]
    pub const fn mask(&self) -> u32 {
        1u32.wrapping_shl(*self as u32).wrapping_sub(1)
    }

    #[inline]
    pub const fn power_of_two(&self) -> u32 {
        1u32.wrapping_shl(*self as u32)
    }

    #[inline]
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        Self::new(self.as_u8() + other.as_u8())
    }
}

impl core::fmt::Display for BitSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_usize())
    }
}

// /// Counts the number of bits set in the byte array
// pub fn count_ones(array: &[u8]) -> usize {
//     array.chunks(4).fold(0, |a, v| match v.try_into() {
//         Ok(v) => a + u32::from_le_bytes(v).count_ones() as usize,
//         Err(_) => a + v.iter().fold(0, |a, v| a + v.count_ones() as usize),
//     })
// }

/// Returns nearest power of two
///
/// # SAFETY
///
/// UB on `value > usize::MAX / 2`
pub const fn nearest_power_of_two(value: usize) -> usize {
    if value == 0 {
        return 0;
    }
    let next = value.next_power_of_two();
    if next == value {
        return next;
    }
    let threshold = (next >> 2).wrapping_mul(3);
    if value >= threshold { next } else { next >> 1 }
}

pub struct BitStreamWriter {
    buf: Vec<u8>,
    acc: u8,
    bit_position: u8,
}

impl BitStreamWriter {
    #[inline]
    pub const fn new() -> Self {
        Self {
            buf: Vec::new(),
            acc: 0,
            bit_position: 0,
        }
    }

    #[inline]
    pub fn bit_count(&self) -> usize {
        self.buf.len() * 8 + self.bit_position as usize
    }

    #[inline]
    pub fn push_bool(&mut self, value: bool) {
        self.push(VarLenInteger::with_bool(value));
    }

    #[inline]
    pub fn push_byte(&mut self, value: u8) {
        self.push(VarLenInteger::with_byte(value))
    }

    #[inline]
    pub fn push_nibble(&mut self, value: Nibble) {
        self.push(VarLenInteger::with_nibble(value))
    }

    #[inline]
    pub fn push_slice(&mut self, value: &[VarLenInteger]) {
        for &item in value.iter() {
            self.push(item);
        }
    }

    pub fn push(&mut self, value: VarLenInteger) {
        let lowest_bits = 8 - self.bit_position;
        let lowest_bit_mask = ((1u32 << value.size().as_u8().min(lowest_bits)) - 1) as u8;
        let mut acc = self.acc | ((value.value() as u8 & lowest_bit_mask) << self.bit_position);
        let mut remain_bits = value.size().as_u8();
        if self.bit_position + remain_bits >= 8 {
            self.buf.push(acc);
            acc = 0;
            remain_bits -= lowest_bits;
            self.bit_position = 0;

            if remain_bits > 0 {
                let value_mask = (1u32 << value.size().as_usize()) - 1;
                let mut acc32 = (value.value() & value_mask) >> lowest_bits;
                while remain_bits >= 8 {
                    self.buf.push(acc32 as u8);
                    acc32 >>= 8;
                    remain_bits -= 8;
                }
                acc = acc32 as u8;
            }
        }

        debug_assert!(
            remain_bits < 8,
            "BITS < 8 BUT {}, input {:?}",
            remain_bits,
            value
        );
        self.acc = acc;
        self.bit_position += remain_bits;
    }

    #[inline]
    pub fn skip_to_next_byte_boundary(&mut self) {
        if self.bit_position > 0 {
            self.buf.push(self.acc);
            self.acc = 0;
            self.bit_position = 0;
        }
    }

    #[inline]
    pub fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.skip_to_next_byte_boundary();
        self.buf.extend_from_slice(bytes);
    }

    #[inline]
    pub fn into_bytes(mut self) -> Vec<u8> {
        self.skip_to_next_byte_boundary();
        self.buf
    }
}

pub trait Write<T> {
    fn write(&mut self, value: T);
}

impl Write<bool> for BitStreamWriter {
    #[inline]
    fn write(&mut self, value: bool) {
        self.push_bool(value);
    }
}

impl Write<Nibble> for BitStreamWriter {
    #[inline]
    fn write(&mut self, value: Nibble) {
        self.push_nibble(value);
    }
}

impl Write<u8> for BitStreamWriter {
    #[inline]
    fn write(&mut self, value: u8) {
        self.push_byte(value);
    }
}

impl Write<&[u8]> for BitStreamWriter {
    #[inline]
    fn write(&mut self, value: &[u8]) {
        for &byte in value.iter() {
            self.push_byte(byte);
        }
    }
}

impl Write<VarLenInteger> for BitStreamWriter {
    #[inline]
    fn write(&mut self, value: VarLenInteger) {
        self.push(value);
    }
}

impl Write<&[VarLenInteger]> for BitStreamWriter {
    #[inline]
    fn write(&mut self, value: &[VarLenInteger]) {
        self.push_slice(value);
    }
}

type AccRepr = usize;

#[repr(C)]
pub struct BitStreamReader<'a> {
    acc: AccRepr,
    left: usize,
    slice: &'a [u8],
}

impl<'a> BitStreamReader<'a> {
    #[inline]
    pub fn new(slice: &'a [u8]) -> Self {
        Self {
            slice,
            left: 0,
            acc: 0,
        }
    }

    #[inline]
    fn _iter_next(&mut self) -> Option<u8> {
        let (left, right) = self.slice.split_first()?;
        self.slice = right;
        Some(*left)
    }

    #[inline]
    pub fn advance(&mut self, bits: BitSize) -> Option<()> {
        if bits.as_usize() <= self.left {
            unsafe {
                // Safety: The value is checked
                self._advance(bits.as_usize());
            }
            Some(())
        } else {
            let mut bits_left = bits.as_usize() - self.left;
            while bits_left >= 8 {
                self._iter_next()?;
                bits_left -= 8;
            }
            if bits_left > 0 {
                self.acc = self._iter_next()? as AccRepr >> bits_left;
                self.left = 8 - bits_left;
            } else {
                self.acc = 0;
                self.left = 0;
            }
            Some(())
        }
    }

    /// # SAFETY
    ///
    /// The `bits` must be less than or equal to `self.left`. Otherwise, UB
    #[inline]
    pub unsafe fn _advance(&mut self, bits: usize) {
        self.acc >>= bits;
        self.left -= bits;
    }

    #[inline]
    pub fn read_bool(&mut self) -> Option<bool> {
        let result = self.peek_bits(BitSize::Bit1)? != 0;
        unsafe {
            // Safety: By calling peek_bits first, the value should be guaranteed.
            self._advance(1);
        }
        Some(result)
    }

    #[inline]
    pub fn read_nibble(&mut self) -> Option<Nibble> {
        self.read_bits(BitSize::NIBBLE)
            .and_then(|v| Nibble::new(v as u8))
    }

    #[inline]
    pub fn read_byte(&mut self) -> Option<u8> {
        self.read_bits(BitSize::BYTE).map(|v| v as u8)
    }

    pub fn read_bits(&mut self, bits: BitSize) -> Option<u32> {
        if bits.as_usize() <= self.left {
            let result = self.acc as u32 & bits.mask();
            unsafe {
                // Safety: The value is checked
                self._advance(bits.as_usize());
            }
            Some(result)
        } else {
            while bits.as_usize() > self.left {
                self.acc |= (self._iter_next()? as AccRepr) << self.left;
                self.left += 8;
            }
            let result = self.acc as u32 & bits.mask();
            unsafe {
                // Safety: The value is checked
                self._advance(bits.as_usize());
            }
            Some(result)
        }
    }

    #[inline]
    pub fn peek_bits(&mut self, bits: BitSize) -> Option<u32> {
        if bits.as_usize() <= self.left {
            Some(self.acc as u32 & bits.mask())
        } else {
            self._peek_bits2(bits)
        }
    }

    /// # Safety
    ///
    /// `bits` must be less than or equal to 24
    fn _peek_bits2(&mut self, bits: BitSize) -> Option<u32> {
        while self.left <= size_of::<AccRepr>() * 8 - 8 {
            let Some((data, next)) = self.slice.split_first() else {
                return (bits.as_usize() <= self.left).then(|| self.acc as u32 & bits.mask());
            };
            self.acc |= (*data as AccRepr) << self.left;
            self.left += 8;
            self.slice = next;
        }
        Some(self.acc as u32 & bits.mask())
    }

    #[inline]
    pub fn skip_to_next_byte_boundary(&mut self) {
        if self.left & 7 != 0 {
            unsafe {
                // Safety: The value is checked
                self._advance(self.left & 7);
            }
        }
    }

    /// Skip to the next byte boundary and read the next byte
    #[inline]
    pub fn read_next_byte(&mut self) -> Option<u8> {
        self.skip_to_next_byte_boundary();
        self._read_next_byte()
    }

    #[inline]
    fn _read_next_byte(&mut self) -> Option<u8> {
        if self.left == 0 {
            self._iter_next()
        } else {
            self.read_byte()
        }
    }

    /// Skip to the next byte boundary and read the specified number of bytes
    #[inline]
    pub fn read_next_bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.skip_to_next_byte_boundary();
        let mut result = [0; N];
        for p in result.iter_mut() {
            *p = self._read_next_byte()?;
        }
        Some(result)
    }

    /// Skips to the next byte boundary and returns a slice with the specified number of bytes
    #[inline]
    pub fn read_next_bytes_slice(&mut self, size: usize) -> Option<&[u8]> {
        self.skip_to_next_byte_boundary();
        if size == 0 {
            return Some(&[]);
        }
        if self.left > 0 {
            let rewind = self.left / 8;
            self.left = 0;
            self.slice = unsafe {
                // Safety: The value is checked, and the slice is guaranteed to be valid.
                core::slice::from_raw_parts(
                    self.slice.as_ptr().sub(rewind),
                    self.slice.len() + rewind,
                )
            }
        }
        let (left, right) = self.slice.split_at_checked(size)?;
        self.slice = right;
        Some(left)
    }
}

impl Iterator for BitStreamReader<'_> {
    type Item = bool;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.read_bool()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_test() {
        let tail = b"Lorem ipsum";
        for padding_size in 1..=16 {
            let padding_mask = (1u32 << padding_size) - 1;
            for value_size in 1..=16 {
                let mask = (1u32 << value_size) - 1;
                for pattern in [
                    0x0u32,
                    u32::MAX,
                    0x55555555,
                    0xAAAAAAAA,
                    0x5A5A5A5A,
                    0xA5A5A5A5,
                    0x0F0F0F0F,
                    0xF0F0F0F0,
                    0xE5E5E5E5,
                    1234578,
                    87654321,
                    0xEDB88320,
                    0x04C11DB7,
                ] {
                    let padding_size = BitSize::new(padding_size).unwrap();
                    let value_size = BitSize::new(value_size).unwrap();
                    // println!("PADDING {padding_size} VALUE {value_size} PATTERN {pattern:08x}");
                    let pattern_n = !pattern & mask;

                    let mut writer = BitStreamWriter::new();
                    writer.push(VarLenInteger::new_checked(padding_size, 0).unwrap());
                    writer.push(VarLenInteger::new_truncated(value_size, pattern));
                    writer.push(VarLenInteger::new_truncated(padding_size, u32::MAX));
                    writer.push(VarLenInteger::new_truncated(value_size, pattern_n));
                    writer.push(VarLenInteger::new_checked(padding_size, 0).unwrap());
                    writer.push(VarLenInteger::with_bool(true));
                    writer.extend_from_slice(tail);
                    let stream = writer.into_bytes();
                    // println!("DATA: {:02x?}", &stream);

                    // test for read_bits
                    let mut reader = BitStreamReader::new(&stream);
                    assert_eq!(reader.read_bits(padding_size).unwrap(), 0);
                    assert_eq!(reader.read_bits(value_size).unwrap(), pattern & mask);
                    assert_eq!(reader.read_bits(padding_size).unwrap(), padding_mask);
                    assert_eq!(reader.read_bits(value_size).unwrap(), pattern_n & mask);
                    assert_eq!(reader.read_bits(padding_size).unwrap(), 0);
                    assert_eq!(reader.read_bool().unwrap(), true);
                    assert_eq!(reader.read_next_bytes_slice(tail.len()).unwrap(), tail);

                    // test for peek_bits + read_bits
                    let mut reader = BitStreamReader::new(&stream);
                    assert_eq!(reader.peek_bits(padding_size).unwrap(), 0);
                    assert_eq!(reader.read_bits(padding_size).unwrap(), 0);
                    assert_eq!(reader.peek_bits(value_size).unwrap(), pattern & mask);
                    assert_eq!(reader.read_bits(value_size).unwrap(), pattern & mask);
                    assert_eq!(reader.peek_bits(padding_size).unwrap(), padding_mask);
                    assert_eq!(reader.read_bits(padding_size).unwrap(), padding_mask);
                    assert_eq!(reader.peek_bits(value_size).unwrap(), pattern_n & mask);
                    assert_eq!(reader.read_bits(value_size).unwrap(), pattern_n & mask);
                    assert_eq!(reader.peek_bits(padding_size).unwrap(), 0);
                    assert_eq!(reader.read_bits(padding_size).unwrap(), 0);
                    assert_eq!(reader.peek_bits(BitSize::Bit1).unwrap(), 1);
                    assert_eq!(reader.read_bits(BitSize::Bit1).unwrap(), 1);
                    assert_eq!(reader.read_next_bytes_slice(tail.len()).unwrap(), tail);

                    // test for peek_bits + advance
                    let mut reader = BitStreamReader::new(&stream);
                    assert_eq!(reader.peek_bits(padding_size).unwrap(), 0);
                    reader.advance(padding_size).unwrap();
                    assert_eq!(reader.peek_bits(value_size).unwrap(), pattern & mask);
                    reader.advance(value_size).unwrap();
                    assert_eq!(reader.peek_bits(padding_size).unwrap(), padding_mask);
                    reader.advance(padding_size).unwrap();
                    assert_eq!(reader.peek_bits(value_size).unwrap(), pattern_n & mask);
                    reader.advance(value_size).unwrap();
                    assert_eq!(reader.peek_bits(padding_size).unwrap(), 0);
                    reader.advance(padding_size).unwrap();
                    assert_eq!(reader.peek_bits(BitSize::Bit1).unwrap(), 1);
                    reader.advance(BitSize::Bit1).unwrap();
                    assert_eq!(reader.read_next_bytes_slice(tail.len()).unwrap(), tail);
                }
            }
        }
    }

    #[test]
    fn nearest() {
        for (value, expected) in [
            (0usize, 0usize),
            (1, 1),
            (2, 2),
            (3, 4),
            (4, 4),
            (5, 4),
            (6, 8),
            (7, 8),
            (8, 8),
            (9, 8),
            (10, 8),
            (11, 8),
            (12, 16),
            (13, 16),
            (14, 16),
            (15, 16),
            (16, 16),
        ] {
            let test = nearest_power_of_two(value);

            assert_eq!(test, expected);
        }
    }

    #[test]
    fn bit_mask() {
        for i in 1..=24 {
            let mask = (1u32 << i) - 1;
            assert_eq!(mask, BitSize::new(i).unwrap().mask());
            let shifted = 1 << i;
            assert_eq!(shifted, BitSize::new(i).unwrap().power_of_two());
        }
    }
}
