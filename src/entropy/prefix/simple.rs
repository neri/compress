//! Simple Prefix Coding
//!
//! Special Huffman code encoder available only when the number of symbols is two.
//!
//! Note that if the number of symbols is one, it is not strictly a Huffman code.

use crate::*;

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
        let mut key1 = None;
        let mut key2 = None;
        for &byte in input.iter() {
            let Some(key1) = key1 else {
                key1 = Some(byte);
                continue;
            };
            let Some(key2) = key2 else {
                if byte != key1 {
                    key2 = Some(byte);
                }
                continue;
            };
            if byte != key1 && byte != key2 {
                return None;
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
        let (key1, key2) = if key1 < key2 {
            (key1, key2)
        } else {
            (key2, key1)
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

#[test]
fn simple_prefix() {
    let input = vec![1, 2, 1, 2, 1, 2, 1, 2, 1, 1, 1, 1, 2, 2, 2, 2];
    let coder = SimplePrefixCoder::encode(&input, true).unwrap();
    assert_eq!(coder.len, input.len());
    assert_eq!(coder.data, [0b10101010, 0b11110000]);
    assert_eq!(coder.table, SimplePrefixTable::Binary(1, 2));

    let input = vec![2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2];
    let coder = SimplePrefixCoder::encode(&input, true).unwrap();
    assert_eq!(coder.len, input.len());
    assert_eq!(coder.data, [0b00001111, 0b11110000]);
    assert_eq!(coder.table, SimplePrefixTable::Binary(1, 2));

    let input = vec![2, 1, 2, 1, 2, 1, 2, 3];
    assert!(SimplePrefixCoder::encode(&input, true).is_none());

    let input = vec![1, 1, 1, 1];
    let coder = SimplePrefixCoder::encode(&input, true).unwrap();
    assert_eq!(coder.len, input.len());
    assert_eq!(coder.data.len(), 0);
    assert_eq!(coder.table, SimplePrefixTable::Repeat(1));
}
