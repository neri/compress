//! Canonical Prefix Coder
//!
//! See also: <https://en.wikipedia.org/wiki/Canonical_Huffman_code>

use crate::num::Nibble;
use crate::num::bits::{BitSize, BitStreamReader, VarBitValue};
use crate::stats::*;
use crate::*;
use core::cmp;
use core::convert::Infallible;

pub struct CanonicalPrefixCoder;

impl CanonicalPrefixCoder {
    /// Repeat the previous value `3 + readbits(2)` times
    pub const REP3P2: u8 = 16;
    /// Repeat 0 `3 + readbits(3)` times
    pub const REP3Z3: u8 = 17;
    /// Repeat 0 `11 + readbits(7)` times
    pub const REP11Z7: u8 = 18;

    pub fn make_prefix_table(
        freq_table: &[usize],
        max_len: BitSize,
        min_size: usize,
    ) -> Vec<Option<VarBitValue>> {
        let mut freq_table = freq_table
            .iter()
            .enumerate()
            .filter_map(|(index, &v)| (v > 0).then(|| (index, v)))
            .collect::<Vec<_>>();
        freq_table.sort_by(|a, b| match b.1.cmp(&a.1) {
            cmp::Ordering::Equal => a.0.cmp(&b.0),
            ord => ord,
        });
        let prefix_table = CanonicalPrefixCoder::generate_prefix_table(&freq_table, max_len, None);
        let max_symbol = prefix_table.iter().fold(0usize, |a, v| a.max((v.0).into()));
        let mut prefix_map = Vec::new();
        prefix_map.resize((1 + max_symbol).max(min_size), None);
        for item in prefix_table.iter() {
            prefix_map[item.0] = Some(item.1);
        }
        prefix_map
    }

    pub fn generate_prefix_table<K>(
        freq_table: &[(K, usize)],
        max_len: BitSize,
        ref_tree: Option<&mut Vec<HuffmanTreeNode<K>>>,
    ) -> Vec<(K, VarBitValue)>
    where
        K: Copy + Ord,
    {
        if freq_table.len() <= 2 {
            let mut input = freq_table.to_vec();
            input.sort_by(|a, b| a.0.cmp(&b.0));
            let mut result = Vec::new();
            for (index, item) in input.iter().enumerate() {
                result.push((item.0, VarBitValue::new(BitSize::Bit1, index as u32)));
            }
            return result;
        }

        let mut freq_table = Vec::from_iter(freq_table.iter());
        freq_table.sort_by(|a, b| match b.1.cmp(&a.1) {
            cmp::Ordering::Equal => a.0.cmp(&b.0),
            ord => ord,
        });

        let mut tree = freq_table
            .iter()
            .map(|v| HuffmanTreeNode::Leaf(v.0, v.1))
            .collect::<Vec<_>>();
        while tree.len() > 1 {
            tree.sort_by(|a, b| a.order(b));
            let left = tree.pop().unwrap();
            let right = tree.pop().unwrap();
            let node = HuffmanTreeNode::make_pair(left, right);
            tree.push(node);
        }

        let mut prefix_size_table = BTreeMap::new();
        tree[0].count_prefix_size(&mut prefix_size_table, 0);
        let actual_max_len = 1 + prefix_size_table.iter().fold(0, |a, v| a.max(*v.0));
        let mut prefix_lengths = Vec::new();
        prefix_lengths.resize(actual_max_len as usize, 0);
        for item in prefix_size_table.into_iter() {
            prefix_lengths[item.0 as usize] = item.1;
        }

        if let Some(ref_tree) = ref_tree {
            ref_tree.clear();
            ref_tree.push(tree.remove(0));
            drop(tree);
        }

        Self::_adjust_prefix_lengths(&mut prefix_lengths, max_len);

        let mut acc = 0;
        let mut last_bits = 0;
        let mut prefix_codes: Vec<VarBitValue> = Vec::new();
        for (bit_len, count) in prefix_lengths.into_iter().enumerate() {
            for _ in 0..count {
                let mut adj = bit_len;
                while last_bits < adj {
                    acc <<= 1;
                    adj -= 1;
                }
                last_bits = bit_len;
                prefix_codes.push(VarBitValue::new(BitSize::new(bit_len as u8).unwrap(), acc));
                acc += 1;
            }
        }

        let mut prefix_table = freq_table
            .iter()
            .zip(prefix_codes.iter())
            .map(|(a, &b)| (a.0, b))
            .collect::<Vec<_>>();
        prefix_table.sort_by(|a, b| match a.1.size().cmp(&b.1.size()) {
            cmp::Ordering::Equal => a.0.cmp(&b.0),
            ord => ord,
        });
        for (p, &q) in prefix_table.iter_mut().zip(prefix_codes.iter()) {
            p.1 = q;
        }

        prefix_table
    }

    fn _adjust_prefix_lengths(prefix_len_table: &mut [usize], max_len: BitSize) {
        let max_len = max_len as usize;
        if prefix_len_table.len() <= max_len {
            return;
        }
        let mut extra_bits = 0;
        for item in prefix_len_table.iter_mut().skip(max_len) {
            extra_bits += *item;
            *item = 0;
        }
        prefix_len_table[max_len] += extra_bits;

        let mut total = 0;
        for i in (1..=max_len).rev() {
            total += prefix_len_table[i] << (max_len - i);
        }

        let one = 1usize << max_len;
        while total > one {
            prefix_len_table[max_len] -= 1;

            for i in (1..=max_len - 1).rev() {
                if prefix_len_table[i] > 0 {
                    prefix_len_table[i] -= 1;
                    prefix_len_table[i + 1] += 2;
                    break;
                }
            }

            total -= 1;
        }
    }

    pub fn rle_match_len(prev_value: u8, data: &[u8], cursor: usize, max_len: usize) -> usize {
        let max_len = (data.len() - cursor).min(max_len);
        for len in 0..max_len {
            if data[cursor + len] != prev_value {
                return len;
            }
        }
        max_len
    }

    fn rle_compress_prefix_table(input: &[u8]) -> Vec<VarBitValue> {
        let mut output = Vec::new();
        let mut cursor = 0;
        let mut prev = 8;
        while let Some(current) = input.get(cursor) {
            let current = *current;
            cursor += {
                if current > 0 {
                    if current == prev {
                        let len = Self::rle_match_len(prev, &input, cursor, 6);
                        if len >= 3 {
                            output.push(VarBitValue::with_byte(Self::REP3P2));
                            output.push(VarBitValue::new(BitSize::Bit2, len as u32 - 3));
                            len
                        } else {
                            output.push(VarBitValue::with_byte(current));
                            1
                        }
                    } else {
                        output.push(VarBitValue::with_byte(current));
                        prev = current;
                        1
                    }
                } else {
                    let len = Self::rle_match_len(0, &input, cursor, 138);
                    if len >= 11 {
                        output.push(VarBitValue::with_byte(Self::REP11Z7));
                        output.push(VarBitValue::new(BitSize::Bit7, len as u32 - 11));
                        len
                    } else if len >= 3 {
                        output.push(VarBitValue::with_byte(Self::REP3Z3));
                        output.push(VarBitValue::new(BitSize::Bit3, len as u32 - 3));
                        len
                    } else {
                        output.push(VarBitValue::with_byte(current));
                        1
                    }
                }
            };
        }
        output
    }

    pub fn encode_single_prefix_table(
        input: &[Option<VarBitValue>],
        permutation_flavor: PermutationFlavor,
    ) -> Result<MetaPrefixTable, Infallible> {
        let table0 = input
            .iter()
            .map(|v| match v {
                Some(v) => v.size().as_u8(),
                None => 0,
            })
            .collect::<Vec<_>>();
        Self::encode_prefix_tables(&[&table0], permutation_flavor)
    }

    pub fn encode_prefix_tables(
        tables: &[&[u8]],
        permutation_flavor: PermutationFlavor,
    ) -> Result<MetaPrefixTable, Infallible> {
        let permutation_order = permutation_flavor.permutation_order();

        let hlits = tables.iter().map(|v| v.len()).collect::<Vec<_>>();

        let tables = tables
            .iter()
            .map(|v| Self::rle_compress_prefix_table(v))
            .collect::<Vec<_>>();

        let mut freq_table = BTreeMap::new();
        for table in tables.iter() {
            for bits in table.iter() {
                if bits.size() == BitSize::OCTET {
                    freq_table.count_freq(bits.value())
                }
            }
        }
        let freq_table = freq_table.into_freq_table(true);

        let prefix_table =
            CanonicalPrefixCoder::generate_prefix_table(&freq_table, BitSize::Bit7, None);
        let mut prefix_map = [None; 20];
        for prefix in prefix_table.iter() {
            assert!(prefix.1.size() < BitSize::OCTET);
            prefix_map[prefix.0 as usize] = Some(prefix.1);
        }

        let mut compressed_table = Vec::new();
        for table in tables.iter() {
            for &item in table.iter() {
                if item.size() == BitSize::OCTET {
                    let prefix_code = prefix_map[item.value() as usize].unwrap();
                    compressed_table.push(prefix_code.reversed());
                } else {
                    compressed_table.push(item);
                }
            }
        }

        let mut prefix_sizes = [None; 19];
        let mut max_index = 3;
        for (p, &q) in permutation_order.iter().enumerate() {
            if let Some(item) = prefix_map[q as usize] {
                max_index = max_index.max(p);
                prefix_sizes[p] = Some(item.size());
            }
        }
        let mut prefix_table = Vec::new();
        for &item in prefix_sizes.iter().take(1 + max_index) {
            prefix_table.push(VarBitValue::new(
                BitSize::Bit3,
                item.map(|v| v as u32).unwrap_or_default(),
            ));
        }

        Ok(MetaPrefixTable {
            hlits,
            hclen: Nibble::new(max_index as u8 - 3).unwrap(),
            prefix_table,
            content: compressed_table,
        })
    }
}

#[derive(Debug)]
pub struct MetaPrefixTable {
    pub hlits: Vec<usize>,
    pub hclen: Nibble,
    pub prefix_table: Vec<VarBitValue>,
    pub content: Vec<VarBitValue>,
}

pub struct CanonicalPrefixDecoder {
    table: Vec<u32>,
}

impl CanonicalPrefixDecoder {
    #[inline]
    pub fn new() -> Self {
        Self {
            table: [0].to_vec(),
        }
    }

    pub fn with_lengths(lengths: &[u8]) -> Self {
        let prefix_table = CanonicalPrefixDecoder::reorder_prefix_table(
            lengths.iter().enumerate().map(|(i, &v)| (i, v)),
        );

        let mut decoder = Self::new();
        for (value, path) in prefix_table.into_iter() {
            decoder.insert_node(path, value as u32);
        }

        decoder
    }

    fn insert_node(&mut self, path: VarBitValue, value: u32) {
        let mut index = 0;
        let mut rpath = path.reversed().value();
        for _ in 0..path.size().as_usize() - 1 {
            let bit = rpath & 1;
            let mut next = self.table[index];
            if bit != 0 {
                next >>= 16;
            }
            if next == 0 {
                let new_index = self.table.len();
                if bit != 0 {
                    self.table[index] |= (new_index as u32) << 16;
                } else {
                    self.table[index] |= new_index as u32;
                }
                self.table.push(0);
                index = new_index;
            } else {
                index = next as usize;
            }
            rpath >>= 1;
        }
        let bit = rpath & 1;
        if bit != 0 {
            self.table[index] |= (0x8000 | value) << 16;
        } else {
            self.table[index] |= 0x8000 | value;
        }
    }

    #[inline]
    pub fn root_node<'a>(&'a self) -> DecodeTreeNode<'a> {
        DecodeTreeNode::new(&self.table, 0)
    }

    // #[inline(never)]
    pub fn decode(&self, reader: &mut BitStreamReader) -> Result<u32, DecodeError> {
        let mut node = self.root_node();
        loop {
            let bit = reader.read_bool().ok_or(DecodeError::InvalidData)?;
            match node.next(bit) {
                ChildNode::Leaf(value) => {
                    return Ok(value);
                }
                ChildNode::Node(next) => {
                    node = next;
                }
            }
        }
    }

    pub fn reorder_prefix_table<K>(prefixes: impl Iterator<Item = (K, u8)>) -> Vec<(K, VarBitValue)>
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
            prefix_table.push((item.0, VarBitValue::new(BitSize::new(bits).unwrap(), acc)));
            acc += 1;
        }
        prefix_table
    }

    pub fn decode_prefix_table_from_bytes(
        bytes: &[u8],
        output_size: usize,
        permutation_flavor: PermutationFlavor,
    ) -> Result<Vec<u8>, DecodeError> {
        let mut reader = BitStreamReader::new(bytes);
        let mut output = Vec::<u8>::new();
        Self::decode_prefix_tables(&mut reader, &mut output, &[output_size], permutation_flavor)?;
        Ok(output)
    }

    pub fn decode_prefix_tables(
        reader: &mut BitStreamReader,
        output: &mut Vec<u8>,
        output_sizes: &[usize],
        permutation_flavor: PermutationFlavor,
    ) -> Result<(), DecodeError> {
        output.reserve(output_sizes.iter().fold(0, |a, v| a + v));

        let num_prefixes = 4 + reader.read_nibble().ok_or(DecodeError::InvalidData)? as usize;
        let mut lengths = [0; 19];
        for &index in permutation_flavor
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

        let decoder = CanonicalPrefixDecoder::with_lengths(&lengths);
        let mut limit = 0;
        for size in output_sizes {
            limit += size;
            let mut prev = 8;
            while output.len() < limit {
                let decoded = decoder.decode(reader)? as u8;
                match decoded {
                    0 => {
                        output.push(decoded);
                    }
                    1..=15 => {
                        output.push(decoded);
                        prev = decoded;
                    }
                    CanonicalPrefixCoder::REP3P2 => {
                        let ext_bits = 3 + reader
                            .read_bits(BitSize::Bit2)
                            .ok_or(DecodeError::InvalidData)?;
                        for _ in 0..ext_bits {
                            output.push(prev);
                        }
                    }
                    CanonicalPrefixCoder::REP3Z3 => {
                        let ext_bits = 3 + reader
                            .read_bits(BitSize::Bit3)
                            .ok_or(DecodeError::InvalidData)?;
                        for _ in 0..ext_bits {
                            output.push(0);
                        }
                    }
                    CanonicalPrefixCoder::REP11Z7 => {
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
        }

        Ok(())
    }
}

/// In deflate, Huffman tables are sorted in a specific order to keep their size small.
#[derive(Debug, Clone, Copy, Default)]
pub enum PermutationFlavor {
    /// 16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    #[default]
    Deflate,
    /// 17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    WebP,
}

impl PermutationFlavor {
    const ORDER_DEFLATE: &[u8; 19] = &[
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];

    const ORDER_WEBP: &[u8; 19] = &[
        17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    ];

    pub fn permutation_order(&self) -> &'static [u8; 19] {
        match self {
            Self::Deflate => Self::ORDER_DEFLATE,
            Self::WebP => Self::ORDER_WEBP,
        }
    }
}

pub enum HuffmanTreeNode<K> {
    Leaf(K, usize),
    Pair(usize, Box<HuffmanTreeNode<K>>, Box<HuffmanTreeNode<K>>),
}

impl<K> HuffmanTreeNode<K> {
    #[inline]
    pub fn make_pair(left: Self, right: Self) -> Self {
        let freq = left.freq() + right.freq();
        Self::Pair(freq, Box::new(left), Box::new(right))
    }

    #[inline]
    pub fn is_leaf(&self) -> bool {
        matches!(self, Self::Leaf(_, _))
    }

    #[inline]
    pub const fn freq(&self) -> usize {
        match self {
            Self::Leaf(_, freq) => *freq,
            Self::Pair(freq, _, _) => *freq,
        }
    }

    #[inline]
    pub fn symbol(&self) -> Option<&K> {
        match self {
            Self::Leaf(symbol, _) => Some(symbol),
            Self::Pair(_, _, _) => None,
        }
    }

    #[inline]
    pub fn left<'a>(&'a self) -> Option<&'a Self> {
        match self {
            Self::Leaf(_, _) => None,
            Self::Pair(_, left, _right) => Some(left.as_ref()),
        }
    }

    #[inline]
    pub fn right<'a>(&'a self) -> Option<&'a Self> {
        match self {
            Self::Leaf(_, _) => None,
            Self::Pair(_, _left, right) => Some(right.as_ref()),
        }
    }

    fn count_prefix_size(&self, map: &mut BTreeMap<u8, usize>, chc_bit: u8) {
        match self {
            Self::Leaf(_, _) => {
                map.entry(chc_bit).and_modify(|v| *v += 1).or_insert(1);
            }
            Self::Pair(_, left, right) => {
                left.count_prefix_size(map, chc_bit + 1);
                right.count_prefix_size(map, chc_bit + 1);
            }
        }
    }

    fn order(&self, other: &Self) -> cmp::Ordering
    where
        K: Ord,
    {
        match other.freq().cmp(&self.freq()) {
            cmp::Ordering::Equal => match (self.symbol(), other.symbol()) {
                (Some(lhs), Some(rhs)) => rhs.cmp(&lhs),
                (Some(_), None) => cmp::Ordering::Greater,
                (None, Some(_)) => cmp::Ordering::Less,
                (None, None) => cmp::Ordering::Equal,
            },
            ord => ord,
        }
    }
}

/// Simple Prefix Coding
pub struct SimplePrefixCoder {
    pub table: SimplePrefixTable,
    pub data: Vec<u8>,
    pub len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimplePrefixTable {
    Repeat(u8),
    Binary(u8, u8),
    NestedRepeat(u8, u8, u8),
    NestedBinary(u8, u8, u8, u8),
}

impl SimplePrefixCoder {
    pub fn encode(input: &[u8], allows_nest: bool) -> Option<Self> {
        let mut freq_table = Vec::with_capacity(256);
        freq_table.resize(256, 0usize);
        input.iter().for_each(|&byte| {
            freq_table[byte as usize] += 1;
        });

        let mut key1 = None;
        let mut key2 = None;
        for (index, &freq) in freq_table.iter().enumerate() {
            if freq > 0 {
                if key1.is_none() {
                    key1 = Some(index as u8);
                } else if key2.is_none() {
                    key2 = Some(index as u8);
                } else {
                    // More than 2 unique values
                    return None;
                }
            }
        }
        let Some(key1) = key1 else {
            return None;
        };
        let key2 = match key2 {
            Some(key2) => key2,
            None => {
                // Only one unique value
                return Some(Self {
                    table: SimplePrefixTable::Repeat(key1),
                    data: Vec::new(),
                    len: input.len(),
                });
            }
        };

        let mut data = Vec::new();
        let mut acc = 0;
        let mut bit = 0x01;
        for &byte in input.iter() {
            if byte == key2 {
                acc |= bit;
            }
            if bit == 0x80 {
                data.push(acc);
                acc = 0;
                bit = 0x01;
            } else {
                bit <<= 1;
            }
        }
        if bit != 0x01 {
            data.push(acc);
        }

        let mut table = SimplePrefixTable::Binary(key1, key2);
        if allows_nest && data.len() >= 4 {
            if let Some(nested) = Self::encode(&data, false) {
                match nested.table {
                    SimplePrefixTable::Repeat(key3) => {
                        table = SimplePrefixTable::NestedRepeat(key1, key2, key3);
                        data = nested.data;
                    }
                    SimplePrefixTable::Binary(key3, key4) => {
                        table = SimplePrefixTable::NestedBinary(key1, key2, key3, key4);
                        data = nested.data;
                    }
                    SimplePrefixTable::NestedRepeat(_, _, _)
                    | SimplePrefixTable::NestedBinary(_, _, _, _) => {
                        unreachable!()
                    }
                }
            }
        }

        let encoded = Self {
            table,
            data,
            len: input.len(),
        };

        // if false {
        //     let decoded = encoded.decode();
        //     assert_eq!(decoded, input);
        // }

        Some(encoded)
    }

    // pub fn decode(&self) -> Vec<u8> {
    //     todo!()
    // }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut vec = Vec::new();
        match self.table {
            SimplePrefixTable::Repeat(key) => {
                vec.push(0);
                vec.push(key);
            }
            SimplePrefixTable::Binary(key1, key2) => {
                vec.push(1);
                vec.push(key1);
                vec.push(key2);
                vec.extend_from_slice(&self.data);
            }
            SimplePrefixTable::NestedRepeat(key1, key2, key3) => {
                vec.push(2);
                vec.push(key1);
                vec.push(key2);
                vec.push(key3);
                vec.extend_from_slice(&self.data);
            }
            SimplePrefixTable::NestedBinary(key1, key2, key3, key4) => {
                vec.push(3);
                vec.push(key1);
                vec.push(key2);
                vec.push(key3);
                vec.push(key4);
                vec.extend_from_slice(&self.data);
            }
        }
        vec
    }
}

#[derive(Debug)]
pub struct DecodeTreeNode<'a> {
    tree: &'a [u32],
    index: u16,
}

impl<'a> DecodeTreeNode<'a> {
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
        if next & 0x8000 == 0 {
            ChildNode::Node(DecodeTreeNode::new(self.tree, next as u16))
        } else {
            ChildNode::Leaf(next & 0x7fff)
        }
    }
}

#[derive(Debug)]
pub enum ChildNode<'a> {
    Leaf(u32),
    Node(DecodeTreeNode<'a>),
}
