//! Canonical Prefix Decoder
use super::*;
use crate::num::bits::{BitSize, BitStreamReader, VarBitValue};
use crate::*;
use core::cmp;

/// Determine the maximum size of the lookup table.
/// It should be approximately 4 times the size of the number of bits specified here. (e.g. 8bits -> 1KB, 10bits -> 4KB)
/// If this size is too small, speed will be reduced, but if it is too large, speed improvement will be slight and memory consumption will increase.
const MAX_LOOKUP_TABLE_BITS: usize = 10;

const LOOKTUP_TABLE_INVAID_VALUE: u16 = u16::MAX;

#[allow(unused)]
pub struct CanonicalPrefixDecoder {
    decode_tree: Vec<u32>,
    lookup_table: Vec<u16>,
    max_lookup_bits: BitSize,
    max_bits: BitSize,
    min_bits: BitSize,
    max_symbol: usize,
    // _table: Vec<(usize, VarBitValue)>,
    // _lengths: Vec<u8>,
}

impl CanonicalPrefixDecoder {
    #[inline]
    fn new(
        max_symbol: usize,
        max_bits: BitSize,
        min_bits: BitSize,
        max_lookup_bits: BitSize,
    ) -> Self {
        Self {
            decode_tree: [0].to_vec(),
            lookup_table: Vec::new(),
            max_symbol,
            max_bits,
            min_bits,
            max_lookup_bits,
            // _table: Vec::new(),
            // _lengths: Vec::new(),
        }
    }

    pub fn with_lengths(lengths: &[u8]) -> Result<Self, DecodeError> {
        let prefix_table = CanonicalPrefixDecoder::reorder_prefix_table(
            lengths.iter().enumerate().map(|(i, &v)| (i, v)),
        )?;

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
            .unwrap_or(0);
        let min_bits = prefix_table
            .iter()
            .map(|(_k, v)| v.size().as_usize())
            .min()
            .unwrap_or(0);
        let min_bits = min_bits.min(MAX_LOOKUP_TABLE_BITS);
        let max_lookup_bits = max_bits.min(MAX_LOOKUP_TABLE_BITS);

        let mut decoder = Self::new(
            max_symbol,
            BitSize::new(max_bits as u8).unwrap(),
            BitSize::new(min_bits as u8).unwrap(),
            BitSize::new(max_lookup_bits as u8).unwrap(),
        );
        // decoder._table = prefix_table.clone();
        // decoder._lengths = lengths.to_vec();

        decoder.decode_tree.reserve(prefix_table.len() * 2);
        for (value, path) in prefix_table.iter().copied() {
            decoder.insert_node(path, value as u32)?;
        }

        decoder
            .lookup_table
            .resize(2 << max_lookup_bits, LOOKTUP_TABLE_INVAID_VALUE);
        for (value, path) in prefix_table.iter().copied() {
            let key = path.reversed().value() as usize | (1 << path.size().as_usize());
            if key >= decoder.lookup_table.len() {
                break;
            }
            decoder.lookup_table[key] = value as u16;
        }

        Ok(decoder)
    }

    fn insert_node(&mut self, path: VarBitValue, value: u32) -> Result<(), DecodeError> {
        let mut index = 0;
        let mut rpath = path.reversed().value();
        for _ in 0..path.size().as_usize() - 1 {
            let bit = rpath & 1;
            let mut next = self.decode_tree[index];
            if bit != 0 {
                next >>= 16;
            } else {
                next &= 0xffff;
            }
            if (next & DecodeTreeNode::LITERAL_FLAG) != 0 {
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
    pub fn decode(&self, reader: &mut BitStreamReader) -> Result<u32, DecodeError> {
        let mut key = reader
            .read_bits(self.min_bits)
            .ok_or(DecodeError::InvalidData)? as usize;
        let mut key_bit = self.min_bits.power_of_two() as usize;
        let max_value = self.max_lookup_bits.power_of_two() as usize;

        while key_bit <= max_value {
            let read = *self
                .lookup_table
                .get(key_bit | key)
                .ok_or(DecodeError::InvalidData)?;
            if read <= self.max_symbol as u16 {
                return Ok(read as u32);
            }
            if reader.read_bool().ok_or(DecodeError::InvalidData)? {
                key |= key_bit
            }
            key_bit <<= 1;
        }

        let mut node = self.root_node();
        for _ in 0..=self.max_lookup_bits.as_usize() {
            match node.next((key & 1) != 0) {
                ChildNode::Leaf(value) => {
                    return Ok(value);
                }
                ChildNode::Node(node_next) => {
                    key >>= 1;
                    node = node_next;
                }
            }
        }

        loop {
            let bit = reader.read_bool().ok_or(DecodeError::InvalidData)?;
            match node.next(bit) {
                ChildNode::Leaf(value) => {
                    return Ok(value);
                }
                ChildNode::Node(node_next) => {
                    node = node_next;
                }
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

    // #[inline(never)]
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
