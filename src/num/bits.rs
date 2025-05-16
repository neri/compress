//! Bit manipulation utilities
use crate::*;
use core::fmt;
use core::mem::transmute;
use core::slice::Iter;
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
    pub const BIT: Self = Self::Bit1;

    pub const NIBBLE: Self = Self::Bit4;

    pub const BYTE: Self = Self::Bit8;

    pub const OCTET: Self = Self::Bit8;

    #[inline]
    pub fn as_usize(&self) -> usize {
        *self as usize
    }

    #[inline]
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    #[inline]
    pub fn as_u32(&self) -> u32 {
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

    #[inline]
    pub const unsafe fn new_unchecked(value: u8) -> Self {
        unsafe { transmute(value) }
    }
}

impl core::fmt::Display for BitSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_usize())
    }
}

/// Counts the number of bits set in the byte array
pub fn count_bits(array: &[u8]) -> usize {
    array.chunks(4).fold(0, |a, v| match v.try_into() {
        Ok(v) => a + u32::from_le_bytes(v).count_ones() as usize,
        Err(_) => a + v.iter().fold(0, |a, v| a + v.count_ones() as usize),
    })
}

/// Returns nearest power of two
///
/// # Safety
///
/// UB on `value > usize::MAX/2`
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

/// A Variable-length bit value
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct VarBitValue(u32);

impl VarBitValue {
    #[inline]
    pub fn new(size: BitSize, value: u32) -> Self {
        Self((value & 0x00ff_ffff) | (size.as_u32() << 24))
    }

    #[inline]
    pub fn with_bool(value: bool) -> Self {
        Self::new(BitSize::Bit1, value as u32)
    }

    #[inline]
    pub fn with_nibble(value: Nibble) -> Self {
        Self::new(BitSize::NIBBLE, value.as_u32())
    }

    #[inline]
    pub fn with_byte(value: u8) -> Self {
        Self::new(BitSize::Bit8, value as u32)
    }

    #[inline]
    pub fn size(&self) -> BitSize {
        unsafe { BitSize::new_unchecked((self.0 >> 24) as u8) }
    }

    #[inline]
    pub fn value(&self) -> u32 {
        self.0 & 0xff_ff_ff
    }

    #[inline]
    pub fn to_vec<T>(iter: T) -> Vec<u8>
    where
        T: Iterator<Item = VarBitValue>,
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
        T: IntoIterator<Item = VarBitValue>,
    {
        Self::to_vec(iter.into_iter())
    }

    pub fn reversed(&self) -> Self {
        let size = self.size();
        let mut value = 0;
        let mut input = self.value();
        for _ in 0..size.as_usize() {
            value = (value << 1) | (input & 1);
            input >>= 1;
        }
        Self::new(size, value)
    }

    // #[inline]
    // pub fn set_value(&mut self, value: u32) {
    //     self.0 = (self.0 & 0xff_00_00_00) | (value & 0xff_ff_ff);
    // }

    #[inline]
    pub fn reverse(&mut self) {
        self.0 = self.reversed().0;
    }

    #[inline]
    pub fn total_len<'a, T>(iter: T) -> usize
    where
        T: Iterator<Item = &'a Option<VarBitValue>>,
    {
        (Self::total_bit_count(iter) + 7) / 8
    }

    #[inline]
    pub fn total_bit_count<'a, T>(iter: T) -> usize
    where
        T: Iterator<Item = &'a Option<VarBitValue>>,
    {
        iter.fold(0, |a, v| match v {
            Some(v) => a + v.size() as usize,
            None => a,
        })
    }
}

impl PartialEq for VarBitValue {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value()
    }
}

impl fmt::Display for VarBitValue {
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
        self.push(VarBitValue::with_bool(value));
    }

    #[inline]
    pub fn push_byte(&mut self, value: u8) {
        self.push(VarBitValue::with_byte(value))
    }

    #[inline]
    pub fn push_nibble(&mut self, value: Nibble) {
        self.push(VarBitValue::with_nibble(value))
    }

    #[inline]
    pub fn push_slice(&mut self, value: &[VarBitValue]) {
        for &item in value.iter() {
            self.push(item);
        }
    }

    pub fn push(&mut self, value: VarBitValue) {
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

        assert!(
            remain_bits < 8,
            "BITS < 8 BUT {}, input {:?}",
            remain_bits,
            value
        );
        self.acc = acc;
        self.bit_position += remain_bits;
    }

    fn flush(&mut self) {
        if self.bit_position > 0 {
            self.buf.push(self.acc);
            self.acc = 0;
            self.bit_position = 0;
        }
    }

    #[inline]
    pub fn into_bytes(mut self) -> Vec<u8> {
        self.flush();
        self.buf
    }
}

pub struct BitStreamReader<'a> {
    iter: Iter<'a, u8>,
    acc: u32,
}

impl<'a> BitStreamReader<'a> {
    #[inline]
    pub fn new(slice: &'a [u8]) -> Self {
        Self {
            iter: slice.iter(),
            acc: Self::DEFAULT_ACC,
        }
    }
}

impl BitStreamReader<'_> {
    const DEFAULT_ACC: u32 = 1;

    #[inline]
    pub fn refill(&mut self) -> Option<()> {
        if self.acc <= Self::DEFAULT_ACC {
            self.acc = 0x100 | *self.iter.next()? as u32;
        }
        Some(())
    }

    #[inline]
    pub fn read_bit(&mut self) -> Option<bool> {
        self.refill()?;
        let result = self.acc & 1 != 0;
        self.acc >>= 1;
        Some(result)
    }

    #[inline]
    pub fn read_bool(&mut self) -> Option<bool> {
        self.read_bit()
    }

    #[inline]
    pub fn read_nibble(&mut self) -> Option<Nibble> {
        self.read_bits(BitSize::NIBBLE)
            .and_then(|v| Nibble::new(v as u8))
    }

    #[inline]
    pub fn read_byte(&mut self) -> Option<u8> {
        self.read_bits(BitSize::Bit8).map(|v| v as u8)
    }

    pub fn read_bits(&mut self, bits: BitSize) -> Option<u32> {
        let max = 1 << bits.as_u8() as u32;
        let mut value = 0;
        let mut position = 1;
        while position < max {
            let read = self.read_bit()? as u32;
            if read != 0 {
                value |= position;
            }
            position += position;
        }
        Some(value)
    }

    #[inline]
    pub fn skip_to_next_byte_boundary(&mut self) {
        self.acc = Self::DEFAULT_ACC;
    }

    /// Skip to the next byte boundary and read the next byte
    #[inline]
    pub fn read_next_byte(&mut self) -> Option<u8> {
        self.skip_to_next_byte_boundary();
        self.iter.next().map(|&v| v)
    }

    /// Skip to the next byte boundary and read the specified number of bytes
    #[inline]
    pub fn read_next_bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        let mut result = [0; N];
        for p in result.iter_mut() {
            *p = self.read_next_byte()?;
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
        let slice = self.iter.as_slice();
        self.iter.nth(size - 1)?;
        slice.get(..size)
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
                    println!("PADDING {padding_size} VALUE {value_size} PATTERN {pattern:08x}");
                    let pattern_n = !pattern & mask;

                    let mut writer = BitStreamWriter::new();
                    writer.push(VarBitValue::new(padding_size, 0));
                    writer.push(VarBitValue::new(value_size, pattern));
                    writer.push(VarBitValue::new(padding_size, u32::MAX));
                    writer.push(VarBitValue::new(value_size, pattern_n));
                    writer.push(VarBitValue::new(padding_size, 0));
                    writer.push(VarBitValue::with_bool(true));
                    let stream = writer.into_bytes();
                    println!("DATA: {:02x?}", &stream);

                    let mut reader = BitStreamReader::new(&stream);
                    assert_eq!(reader.read_bits(padding_size).unwrap(), 0);
                    assert_eq!(reader.read_bits(value_size).unwrap(), pattern & mask);
                    assert_eq!(reader.read_bits(padding_size).unwrap(), padding_mask);
                    assert_eq!(reader.read_bits(value_size).unwrap(), pattern_n & mask);
                    assert_eq!(reader.read_bits(padding_size).unwrap(), 0);
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
            (16, 16),
            (16, 16),
        ] {
            let test = nearest_power_of_two(value);

            assert_eq!(test, expected);
        }
    }

    #[test]
    fn reverse() {
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
            let lhs = VarBitValue::new(size, lhs);
            let rhs = VarBitValue::new(size, rhs);

            assert_eq!(lhs.reversed(), rhs);
            assert_eq!(lhs, rhs.reversed());

            assert_eq!(lhs.reversed().reversed(), lhs);
            assert_eq!(rhs.reversed().reversed(), rhs);
        }
    }
}
