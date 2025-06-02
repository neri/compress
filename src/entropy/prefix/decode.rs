//! Canonical Prefix Decoder

use super::*;
use crate::num::bits::{BitSize, BitStreamReader, VarBitValue};
use crate::*;
use core::cmp;

const MAX_LOOKUP_TABLE_BITS: usize = 12;

#[allow(unused)]
pub struct CanonicalPrefixDecoder {
    lookup_table: Vec<LookupTableEntry>,
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
            max_symbol,
            peek_bits,
            max_bits,
            min_bits,
        }
    }

    pub fn with_lengths(lengths: &[u8]) -> Result<Self, DecodeError> {
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

        let peek_bits = max_bits.min(MAX_LOOKUP_TABLE_BITS);

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

        decoder
            .lookup_table
            .resize(1 << peek_bits, LookupTableEntry::EMPTY);
        let max_peek_value = decoder.lookup_table.len();
        for (sym1, path1) in prefix_table.iter().copied() {
            if path1.size().as_usize() > peek_bits {
                continue;
            }
            if let Some(item) = LookupTableEntry::new(sym1, path1.size()) {
                let mut path = path1.reversed().value() as usize;
                let delta = path1.size().power_of_two() as usize;
                while path < max_peek_value {
                    decoder.lookup_table[path] = item;
                    path += delta;
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

    // #[inline(never)]
    #[inline]
    pub fn decode(&self, reader: &mut BitStreamReader) -> Result<u32, DecodeError> {
        if let Some(key) = reader.peek_bits(self.peek_bits) {
            let entry = self
                .lookup_table
                .get(key as usize)
                .ok_or(DecodeError::InvalidData)?;
            if let Some(bits) = entry.advance_bits() {
                let symbol1 = entry.symbol1();
                reader.advance(bits);
                return Ok(symbol1);
            }
        }
        self._decode_failthrough(reader)
    }
    fn _decode_failthrough(&self, reader: &mut BitStreamReader) -> Result<u32, DecodeError> {
        let mut node = self.root_node();
        loop {
            let bit = reader.read_bool().ok_or(DecodeError::UnexpectedEof)?;
            match node.next(bit) {
                ChildNode::Leaf(value) => return Ok(value),
                ChildNode::Node(child) => node = child,
            }
        }
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
            let p = lengths
                .get_mut(index as usize)
                .ok_or(DecodeError::InvalidData)?;
            *p = prefix_bit as u8;
        }

        output.reserve(output_size);
        let decoder = CanonicalPrefixDecoder::with_lengths(&lengths)?;
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
        let decoder = CanonicalPrefixDecoder::with_lengths(&lengths)?;
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
/// * bit4-11 symbol1
/// * bit12-15 mbz
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
        Some(Self((bits.as_usize() as u16) | (symbol1 as u16) << 4))
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn symbol1(&self) -> u32 {
        self.0 as u32 >> 4
    }

    #[inline]
    pub const fn advance_bits(&self) -> Option<BitSize> {
        BitSize::new((self.0 & 15) as u8)
    }
}
