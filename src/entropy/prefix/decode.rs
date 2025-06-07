//! Canonical Prefix Decoder

use super::*;
use crate::num::bits::{BitSize, BitStreamReader, VarBitValue};
use crate::*;
use core::cmp;

const MAX_LOOKUP_TABLE_BITS: usize = 12;

#[allow(unused)]
pub struct CanonicalPrefixDecoder {
    lookup_table: Vec<LookupTableEntry>,
    lookup_table2: Vec<LookupTableEntry2>,
    decode_tree: Vec<u32>,
    peek_bits: BitSize,
    max_bits: BitSize,
    min_bits: BitSize,
    max_symbol: usize,
}

impl CanonicalPrefixDecoder {
    #[inline]
    fn new(max_symbol: usize, peek_bits: BitSize, max_bits: BitSize, min_bits: BitSize) -> Self {
        Self {
            decode_tree: [0].to_vec(),
            lookup_table: Vec::new(),
            lookup_table2: Vec::new(),
            max_symbol,
            peek_bits,
            max_bits,
            min_bits,
        }
    }

    pub fn with_lengths(lengths: &[u8], is_lzss_lit: bool) -> Result<Self, DecodeError> {
        let prefix_table =
            Self::reorder_prefix_table(lengths.iter().enumerate().map(|(i, &v)| (i, v)))?;

        if prefix_table.len() < 2 {
            // The prefix table must have at least two entries
            return Err(DecodeError::InvalidData);
        }

        let max_symbol = prefix_table
            .iter()
            .map(|(k, _v)| *k)
            .max()
            .ok_or(DecodeError::InvalidData)?;
        let max_bits = prefix_table
            .iter()
            .map(|(_k, v)| v.size().as_usize())
            .max()
            .ok_or(DecodeError::InvalidData)?;
        let min_bits = prefix_table
            .iter()
            .map(|(_k, v)| v.size().as_usize())
            .min()
            .ok_or(DecodeError::InvalidData)?;

        let peek_bits = if is_lzss_lit {
            (max_bits * 2).min(MAX_LOOKUP_TABLE_BITS)
        } else {
            max_bits.min(MAX_LOOKUP_TABLE_BITS)
        };

        let mut decoder = Self::new(
            max_symbol,
            BitSize::new(peek_bits as u8).unwrap(),
            BitSize::new(max_bits as u8).unwrap(),
            BitSize::new(min_bits as u8).unwrap(),
        );

        decoder.decode_tree.reserve(prefix_table.len() * 2);
        for (value, path) in prefix_table.iter().copied() {
            decoder.insert_node(path, value as u32)?;
        }

        if !is_lzss_lit {
            decoder
                .lookup_table
                .resize(1 << peek_bits, LookupTableEntry::EMPTY);
            let max_peek_value = decoder.lookup_table.len();
            for (sym1, path1) in prefix_table.iter().copied() {
                if path1.size().as_usize() > peek_bits {
                    continue;
                }
                if let Some(entry) = LookupTableEntry::new(sym1, path1.size()) {
                    let mut path = path1.reversed().value() as usize;
                    let delta = path1.size().power_of_two() as usize;
                    while path < max_peek_value {
                        decoder.lookup_table[path] = entry;
                        path += delta;
                    }
                }
            }
        } else {
            // For LZSS literal codes, we use a different lookup table
            decoder
                .lookup_table2
                .resize(1 << peek_bits, LookupTableEntry2::EMPTY);
            let max_peek_value = decoder.lookup_table2.len();
            let prefix_table2 = prefix_table
                .iter()
                .filter(|(sym, path)| {
                    *sym < 256 && path.size().as_usize() <= (peek_bits - min_bits)
                })
                .map(|(sym, path)| (*sym, path.reversed()))
                .collect::<Vec<_>>();
            for (sym1, path1) in prefix_table.iter().copied() {
                if path1.size().as_usize() > peek_bits {
                    continue;
                }
                let entry =
                    LookupTableEntry2::new(LitLen2::from_lit_len(sym1 as u32), path1.size());
                let rpath1 = path1.reversed();
                let mut path = rpath1.value() as usize;
                let delta = path1.size().power_of_two() as usize;
                while path < max_peek_value {
                    decoder.lookup_table2[path] = entry;
                    path += delta;
                }
                if sym1 > 255 || path1.size().as_usize() + min_bits > peek_bits {
                    continue;
                }
                let sym1 = sym1 as u8;
                for (sym2, path2) in prefix_table2.iter().copied() {
                    let Some(path_len) = path1.size().checked_add(path2.size()) else {
                        continue;
                    };
                    if path_len.as_usize() > peek_bits {
                        continue;
                    }
                    let sym2 = sym2 as u8;
                    let entry = LookupTableEntry2::new(LitLen2::Double(sym1, sym2), path_len);
                    let path2 = rpath1.value() as usize
                        | (path2.value() as usize) << path1.size().as_usize();
                    let mut path = path2;
                    let delta = path_len.power_of_two() as usize;
                    while path < max_peek_value {
                        decoder.lookup_table2[path] = entry;
                        path += delta;
                    }
                }
            }
        }

        Ok(decoder)
    }

    fn insert_node(&mut self, path: VarBitValue, value: u32) -> Result<(), DecodeError> {
        let mut index = 0;
        let mut rpath = path.reversed().value();
        for _ in 1..path.size().as_usize() {
            let bit = rpath & 1;
            let mut next = self.decode_tree[index];
            if bit != 0 {
                next >>= 16;
            } else {
                next &= 0xffff;
            }
            if (next & DecodeTreeNode::LITERAL_FLAG) != 0 {
                // Perhaps the prefix table is invalid or decoding failed.
                return Err(DecodeError::InvalidData);
            }
            if next == 0 {
                let new_index = self.decode_tree.len();
                if bit != 0 {
                    self.decode_tree[index] |= (new_index as u32) << 16;
                } else {
                    self.decode_tree[index] |= new_index as u32;
                }
                self.decode_tree.push(0);
                index = new_index;
            } else {
                index = next as usize;
            }
            rpath >>= 1;
        }
        let bit = rpath & 1;
        if bit != 0 {
            self.decode_tree[index] |= (DecodeTreeNode::LITERAL_FLAG | value) << 16;
        } else {
            self.decode_tree[index] |= DecodeTreeNode::LITERAL_FLAG | value;
        }
        Ok(())
    }

    #[inline]
    pub fn root_node<'a>(&'a self) -> DecodeTreeNode<'a> {
        DecodeTreeNode::new(&self.decode_tree, 0)
    }

    /// Decodes a symbol using the lookup table.
    ///
    /// This function is fast but cannot process prefix code that is not in the lookup table,
    /// so it falls back to the slow version.
    #[inline]
    pub fn decode(&self, reader: &mut BitStreamReader) -> Result<u32, DecodeError> {
        if let Some(key) = reader.peek_bits(self.peek_bits) {
            let entry = self
                .lookup_table
                .get(key as usize)
                .ok_or(DecodeError::InvalidData)?;
            if let Some(bits) = entry.bit_len() {
                let symbol1 = entry.symbol1();
                reader.advance(bits);
                return Ok(symbol1);
            }
        }
        self.decode_slow(reader)
    }

    /// Decode up to 2 literals using the lookup table.
    #[inline]
    pub fn decode2(&self, reader: &mut BitStreamReader) -> Result<LitLen2, DecodeError> {
        if let Some(key) = reader.peek_bits(self.peek_bits) {
            let entry = self
                .lookup_table2
                .get(key as usize)
                .ok_or(DecodeError::InvalidData)?;
            if let Some(bits) = entry.bit_len() {
                reader.advance(bits);
                return Ok(entry.into_lit_len());
            }
        }
        self.decode2_slow(reader)
    }

    /// Decodes a symbol.
    ///
    /// This function is slower than the lookup version, but can process all prefix codes.
    pub fn decode_slow(&self, reader: &mut BitStreamReader) -> Result<u32, DecodeError> {
        let mut node = self.root_node();
        loop {
            let bit = reader.read_bool().ok_or(DecodeError::UnexpectedEof)?;
            match node.next(bit) {
                ChildNode::Leaf(value) => return Ok(value),
                ChildNode::Node(child) => node = child,
            }
        }
    }

    pub fn decode2_slow(&self, reader: &mut BitStreamReader) -> Result<LitLen2, DecodeError> {
        self.decode_slow(reader).map(|v| LitLen2::from_lit_len(v))
    }

    pub fn reorder_prefix_table<K>(
        prefixes: impl Iterator<Item = (K, u8)>,
    ) -> Result<Vec<(K, VarBitValue)>, DecodeError>
    where
        K: Copy + Ord,
    {
        let mut prefixes = prefixes.filter(|(_k, v)| *v > 0).collect::<Vec<_>>();
        prefixes.sort_by(|a, b| match a.1.cmp(&b.1) {
            cmp::Ordering::Equal => a.0.cmp(&b.0),
            ord => ord,
        });

        let mut prefix_table = Vec::new();
        let mut acc = 0;
        let mut last_bits = 0;
        for item in prefixes.iter() {
            let bits = item.1;
            let mut adj = bits;
            while last_bits < adj {
                acc <<= 1;
                adj -= 1;
            }
            last_bits = bits;
            prefix_table.push((
                item.0,
                VarBitValue::new_checked(BitSize::new(bits).unwrap(), acc)
                    .ok_or(DecodeError::InvalidData)?,
            ));
            acc += 1;
        }

        Ok(prefix_table)
    }

    pub fn decode_prefix_table_deflate(
        reader: &mut BitStreamReader,
        output: &mut Vec<u8>,
        output_size: usize,
    ) -> Result<(), DecodeError> {
        let num_prefixes = 4 + reader.read_nibble().ok_or(DecodeError::InvalidData)? as usize;
        let mut lengths = [0; 19];
        for &index in PermutationFlavor::Deflate
            .permutation_order()
            .iter()
            .take(num_prefixes)
        {
            let prefix_bit = reader
                .read_bits(BitSize::Bit3)
                .ok_or(DecodeError::InvalidData)?;
            lengths[index as usize] = prefix_bit as u8;
        }

        output.reserve(output_size);
        let decoder = CanonicalPrefixDecoder::with_lengths(&lengths, false)?;
        let mut prev = 0; // not strictly defined
        while output.len() < output_size {
            let decoded = decoder.decode(reader)? as u8;
            match decoded {
                0..=15 => {
                    output.push(decoded);
                    prev = decoded;
                }
                REP3P2 => {
                    let ext_bits = 3 + reader
                        .read_bits(BitSize::Bit2)
                        .ok_or(DecodeError::InvalidData)?;
                    for _ in 0..ext_bits {
                        output.push(prev);
                    }
                }
                REP3Z3 => {
                    let ext_bits = 3 + reader
                        .read_bits(BitSize::Bit3)
                        .ok_or(DecodeError::InvalidData)?;
                    for _ in 0..ext_bits {
                        output.push(0);
                    }
                    prev = 0;
                }
                REP11Z7 => {
                    let ext_bits = 11
                        + reader
                            .read_bits(BitSize::Bit7)
                            .ok_or(DecodeError::InvalidData)?;
                    for _ in 0..ext_bits {
                        output.push(0);
                    }
                    prev = 0;
                }
                _ => return Err(DecodeError::InvalidData),
            }
        }

        Ok(())
    }

    pub fn decode_prefix_table_webp(
        reader: &mut BitStreamReader,
        output: &mut Vec<u8>,
        output_size: usize,
    ) -> Result<(), DecodeError> {
        let num_prefixes = 4 + reader.read_nibble().ok_or(DecodeError::InvalidData)? as usize;
        let mut lengths = [0; 19];
        for &index in PermutationFlavor::WebP
            .permutation_order()
            .iter()
            .take(num_prefixes)
        {
            let prefix_bit = reader
                .read_bits(BitSize::Bit3)
                .ok_or(DecodeError::InvalidData)?;
            let p = lengths
                .get_mut(index as usize)
                .ok_or(DecodeError::InvalidData)?;
            *p = prefix_bit as u8;
        }

        output.reserve(output_size);
        let decoder = CanonicalPrefixDecoder::with_lengths(&lengths, false)?;
        let mut prev = 8;
        while output.len() < output_size {
            let decoded = decoder.decode(reader)? as u8;
            match decoded {
                0 => {
                    output.push(decoded);
                }
                1..=15 => {
                    output.push(decoded);
                    prev = decoded;
                }
                REP3P2 => {
                    let ext_bits = 3 + reader
                        .read_bits(BitSize::Bit2)
                        .ok_or(DecodeError::InvalidData)?;
                    for _ in 0..ext_bits {
                        output.push(prev);
                    }
                }
                REP3Z3 => {
                    let ext_bits = 3 + reader
                        .read_bits(BitSize::Bit3)
                        .ok_or(DecodeError::InvalidData)?;
                    for _ in 0..ext_bits {
                        output.push(0);
                    }
                }
                REP11Z7 => {
                    let ext_bits = 11
                        + reader
                            .read_bits(BitSize::Bit7)
                            .ok_or(DecodeError::InvalidData)?;
                    for _ in 0..ext_bits {
                        output.push(0);
                    }
                }
                _ => return Err(DecodeError::InvalidData),
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct DecodeTreeNode<'a> {
    tree: &'a [u32],
    index: u16,
}

impl<'a> DecodeTreeNode<'a> {
    const LITERAL_FLAG: u32 = 0x8000;
    const LITERAL_MASK: u32 = 0x7fff;

    #[inline]
    fn new(tree: &'a [u32], index: u16) -> Self {
        Self { tree, index }
    }

    pub fn next(&self, bit: bool) -> ChildNode<'a> {
        let mut next = self.tree[self.index as usize];
        if bit {
            next >>= 16;
        }
        if next & Self::LITERAL_FLAG == 0 {
            ChildNode::Node(DecodeTreeNode::new(self.tree, next as u16))
        } else {
            ChildNode::Leaf(next & Self::LITERAL_MASK)
        }
    }
}

#[derive(Debug)]
pub enum ChildNode<'a> {
    Leaf(u32),
    Node(DecodeTreeNode<'a>),
}

/// A lookup table entry for the canonical prefix decoder.
///
/// format:
/// * bit0-3 bit lengths to advance (1-15)
/// * bit4-6 reserved, mbz
/// * bit7-15 symbol1
///
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct LookupTableEntry(u16);

impl LookupTableEntry {
    pub const EMPTY: Self = Self(0);

    #[inline]
    pub fn new(symbol1: usize, bits: BitSize) -> Option<Self> {
        if bits > BitSize::Bit15 {
            return None;
        }
        Some(Self((bits.as_usize() as u16) | (symbol1 as u16) << 7))
    }

    #[inline]
    pub const fn symbol1(&self) -> u32 {
        self.0 as u32 >> 7
    }

    #[inline]
    pub const fn bit_len(&self) -> Option<BitSize> {
        BitSize::new((self.0 & 15) as u8)
    }
}

/// A lookup table entry for the literal 2 decoder.
///
/// format:
/// * bit0-7 literal type (0-3)
/// * bit8-15 symbol1
/// * bit16-23 symbol2
/// * bit24-31 number of bits to advance (1-24)
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default)]
pub struct LookupTableEntry2(u32);

#[derive(Debug, Clone, Copy)]
pub enum LitLen2 {
    /// End of block marker in deflate
    /// Takes a dummy argument for performance reasons, but the value is not saved.
    EndOfBlock([u8; 3]),
    /// Single literal value
    Single(u8),
    /// Two literal values
    Double(u8, u8),
    /// Length value, used for length/distance pairs
    Length(u8),
}

impl LookupTableEntry2 {
    pub const EMPTY: Self = Self(0);

    #[inline]
    pub fn new(lit: LitLen2, bit_len: BitSize) -> Self {
        let mut lit: [u8; 4] = unsafe { core::mem::transmute(lit) };
        lit[3] = bit_len.as_u8();
        Self(u32::from_le_bytes(lit))
    }

    #[inline]
    pub fn bit_len(&self) -> Option<BitSize> {
        BitSize::new((self.0 >> 24) as u8)
    }

    #[inline]
    pub fn into_lit_len(self) -> LitLen2 {
        let lit: [u8; 4] = self.0.to_le_bytes();
        unsafe { core::mem::transmute(lit) }
    }
}

impl LitLen2 {
    #[inline]
    pub fn from_lit_len(value: u32) -> Self {
        if value < 256 {
            Self::Single(value as u8)
        } else if value == 256 {
            Self::EndOfBlock([0, 0, 0])
        } else {
            Self::Length((value - 257) as u8)
        }
    }
}

impl PartialEq for LitLen2 {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (LitLen2::EndOfBlock(_), LitLen2::EndOfBlock(_)) => true,
            (LitLen2::Single(a), LitLen2::Single(b)) => a == b,
            (LitLen2::Double(a1, a2), LitLen2::Double(b1, b2)) => a1 == b1 && a2 == b2,
            (LitLen2::Length(a), LitLen2::Length(b)) => a == b,
            _ => false,
        }
    }
}

#[test]
fn literal2_repr() {
    let lit_len = LitLen2::Length(0x12);
    let bits = BitSize::Bit5;
    let entry = LookupTableEntry2::new(lit_len, bits);
    assert_eq!(entry.bit_len(), Some(bits));
    assert_eq!(entry.into_lit_len(), lit_len);

    let lit_len = LitLen2::Single(0x34);
    let bits = BitSize::Bit7;
    let entry = LookupTableEntry2::new(lit_len, bits);
    assert_eq!(entry.bit_len(), Some(bits));
    assert_eq!(entry.into_lit_len(), lit_len);

    let lit_len = LitLen2::Double(0x56, 0x78);
    let bits = BitSize::Bit11;
    let entry = LookupTableEntry2::new(lit_len, bits);
    assert_eq!(entry.bit_len(), Some(bits));
    assert_eq!(entry.into_lit_len(), lit_len);

    let lit_len = LitLen2::EndOfBlock([0, 0, 0]);
    let bits = BitSize::Bit13;
    let entry = LookupTableEntry2::new(lit_len, bits);
    assert_eq!(entry.bit_len(), Some(bits));
    assert_eq!(entry.into_lit_len(), lit_len);
}
