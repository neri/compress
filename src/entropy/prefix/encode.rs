///! Canonical Prefix Coder
use super::*;
use crate::num::Nibble;
use crate::num::bits::{BitSize, VarBitValue};
use crate::stats::*;
use crate::*;
use core::cmp;
use core::convert::Infallible;

pub struct CanonicalPrefixCoder;

impl CanonicalPrefixCoder {
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
        let mut prev = 0; //8;
        while let Some(current) = input.get(cursor) {
            let current = *current;
            cursor += {
                if current > 0 {
                    if current == prev {
                        let len = Self::rle_match_len(prev, &input, cursor, 6);
                        if len >= 3 {
                            output.push(VarBitValue::with_byte(REP3P2));
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
                    prev = 0;
                    if len >= 11 {
                        output.push(VarBitValue::with_byte(REP11Z7));
                        output.push(VarBitValue::new(BitSize::Bit7, len as u32 - 11));
                        len
                    } else if len >= 3 {
                        output.push(VarBitValue::with_byte(REP3Z3));
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
            intermediate_tables: tables,
        })
    }
}

#[derive(Debug)]
pub struct MetaPrefixTable {
    pub hlits: Vec<usize>,
    pub hclen: Nibble,
    pub prefix_table: Vec<VarBitValue>,
    pub content: Vec<VarBitValue>,
    pub intermediate_tables: Vec<Vec<VarBitValue>>,
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
