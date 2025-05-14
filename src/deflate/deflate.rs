//! Deflate compression algorithm
//!
//! See also: <https://www.ietf.org/rfc/rfc1951.txt>

use crate::entropy::prefix::{CanonicalPrefixDecoder, PermutationFlavor};
use crate::num::bits::{BitSize, BitStreamReader, VarBitValue};
use crate::*;

#[cfg(test)]
mod tests;

pub struct Deflate;

impl Deflate {
    // /// Compresses a deflate stream.
    // pub fn deflate(input: &[u8],) -> Result<Vec<u8>, EncodeError> {
    //     todo!()
    // }

    /// Decompresses a deflate stream.
    pub fn inflate(input: &[u8], limit_len: usize) -> Result<Vec<u8>, DecodeError> {
        // In zlib, the first byte is always 08, 78, etc., but a pure deflate stream will never have such a value.
        let leading = *input.get(0).ok_or(DecodeError::InvalidData)?;
        let skip = if leading & 0x0f == 0x08 {
            // zlib header
            let cmf = leading;
            let flg = *input.get(1).ok_or(DecodeError::InvalidData)?;
            let cmf_flg = cmf as u16 * 256 + flg as u16;
            if (flg & 0x20) != 0 {
                return Err(DecodeError::UnsupportedFormat);
            }
            if (cmf_flg % 31) != 0 {
                return Err(DecodeError::InvalidData);
            }
            Some(BitSize::Bit16)
        } else {
            None
        };

        let mut reader = BitStreamReader::new(input);
        if let Some(skip) = skip {
            reader.read(skip).ok_or(DecodeError::InvalidData)?;
        }

        let mut output = Vec::new();
        while output.len() < limit_len {
            let bfinal = reader.read_bool().ok_or(DecodeError::InvalidData)?;
            let btype = reader.read(BitSize::Bit2).ok_or(DecodeError::InvalidData)?;
            match btype {
                0b00 => {
                    // uncompressed block
                    let len = reader.read_next_bytes().ok_or(DecodeError::InvalidData)?;
                    let nlen = reader.read_next_bytes().ok_or(DecodeError::InvalidData)?;
                    let len = u16::from_le_bytes(len);
                    let nlen = u16::from_le_bytes(nlen);
                    if len != !nlen {
                        return Err(DecodeError::InvalidData);
                    }
                    output.extend_from_slice(
                        reader
                            .read_next_bytes_slice(len as usize)
                            .ok_or(DecodeError::InvalidData)?,
                    );
                }
                0b01 => {
                    // fixed Huffman block
                    let mut lengths_lit = [0; 288];
                    for i in 0..288 {
                        lengths_lit[i] = if i < 144 {
                            8
                        } else if i < 256 {
                            9
                        } else if i < 280 {
                            7
                        } else {
                            8
                        };
                    }
                    let lengths_dist = [5; 32];
                    Self::_decode_block(
                        &mut reader,
                        &mut output,
                        limit_len,
                        &lengths_lit,
                        &lengths_dist,
                    )?;
                }
                0b10 => {
                    // dynamic Huffman block
                    let hlit =
                        257 + reader.read(BitSize::Bit5).ok_or(DecodeError::InvalidData)? as usize;
                    let hdist =
                        1 + reader.read(BitSize::Bit5).ok_or(DecodeError::InvalidData)? as usize;
                    let mut prefix_table = Vec::new();
                    CanonicalPrefixDecoder::decode_prefix_tables(
                        &mut reader,
                        &mut prefix_table,
                        &[hlit, hdist],
                        PermutationFlavor::Deflate,
                    )?;
                    let (lengths_lit, lengths_dist) = prefix_table.split_at(hlit);
                    Self::_decode_block(
                        &mut reader,
                        &mut output,
                        limit_len,
                        lengths_lit,
                        lengths_dist,
                    )?;
                }
                _ => {
                    // reserved (error)
                    todo!();
                    // return Err(DecodeError::InvalidData);
                }
            }
            if bfinal {
                break;
            }
        }

        if output.len() <= limit_len {
            Ok(output)
        } else {
            // If the decoding result is larger than expected, an error is generated.
            Err(DecodeError::InvalidInput)
        }
    }

    fn _decode_block(
        reader: &mut BitStreamReader,
        output: &mut Vec<u8>,
        limit_len: usize,
        lengths_lit: &[u8],
        lengths_dist: &[u8],
    ) -> Result<(), DecodeError> {
        let decoder_lit = CanonicalPrefixDecoder::with_lengths(lengths_lit);
        let decoder_dist = CanonicalPrefixDecoder::with_lengths(lengths_dist);
        while output.len() < limit_len {
            let lit = decoder_lit.decode(reader)?;
            if lit < 256 {
                // literal
                output.push(lit as u8);
            } else if lit == 256 {
                // end of block
                break;
            } else {
                // length/distance pair
                let len = 3 + LenType::decode((lit - 257) as u8, reader)
                    .ok_or(DecodeError::InvalidData)?
                    .value() as usize;
                let dist = 1 + OffsetType::decode(decoder_dist.decode(reader)? as u8, reader)
                    .ok_or(DecodeError::InvalidData)?
                    .value() as usize;

                assert!(output.len() >= dist);
                let len = len.min(limit_len - output.len());
                if dist > output.len() {
                    return Err(DecodeError::InvalidData);
                }
                let base = output.len() - dist;
                for i in 0..len {
                    output.push(output[base + i]);
                }
            }
        }
        Ok(())
    }
}

macro_rules! var_uint32 {
    ($class_name:ident, $base_table:ident, $max_value:expr) => {
        #[repr(align(8))]
        #[derive(Debug, PartialEq)]
        pub struct $class_name {
            pub trailing: Option<VarBitValue>,
            pub leading: u8,
        }

        impl $class_name {
            pub const MAX_VALUE: u32 = $max_value;

            #[inline]
            pub fn new(value: u32) -> Option<Self> {
                Self::split(value).map(|(leading, trailing)| Self { leading, trailing })
            }

            #[inline]
            pub fn value(&self) -> u32 {
                $base_table[self.leading as usize].1
                    + self.trailing.map(|v| v.value()).unwrap_or_default()
            }

            #[inline]
            pub const fn symbol_size() -> u8 {
                $base_table.len().next_power_of_two().trailing_zeros() as u8
            }

            pub fn decode(leading: u8, reader: &mut BitStreamReader) -> Option<Self> {
                let (ext_bit, _min_value) = *($base_table.get(leading as usize)?);
                if ext_bit > 0 {
                    let ext_bit = BitSize::new(ext_bit).unwrap();
                    let trailing = reader.read(ext_bit).map(|v| VarBitValue::new(ext_bit, v))?;
                    Some(Self {
                        leading,
                        trailing: Some(trailing),
                    })
                } else {
                    Some(Self {
                        leading,
                        trailing: None,
                    })
                }
            }

            pub fn push_to(&self, cmd_buf: &mut Vec<u8>, ext_buf: &mut Vec<VarBitValue>) {
                cmd_buf.push(self.leading);
                if let Some(trailing) = self.trailing {
                    ext_buf.push(trailing);
                }
            }

            pub fn write_to(&self, writer: &mut Vec<VarBitValue>) {
                writer.push(VarBitValue::new(BitSize::Bit6, self.leading as u32));
                if let Some(trailing) = self.trailing {
                    writer.push(trailing);
                }
            }

            fn split(value: u32) -> Option<(u8, Option<VarBitValue>)> {
                let mut prev_item = (0, $base_table[0]);
                for (index, item) in $base_table.iter().enumerate().skip(1) {
                    if value < item.1 {
                        let (size, min_value) = prev_item.1;
                        let mask = (1u32 << size) - 1;
                        let value = mask & value.checked_sub(min_value)?;
                        let trailing = BitSize::new(size).map(|size| VarBitValue::new(size, value));
                        return Some((prev_item.0, trailing));
                    }
                    prev_item = (index as u8, *item);
                }
                let (size, min_value) = prev_item.1;
                let mask = (1u32 << size) - 1;
                let value = value.checked_sub(min_value)?;
                let trailing = BitSize::new(size).map(|size| VarBitValue::new(size, value));
                ((value & mask) == value).then(|| (prev_item.0, trailing))
            }
        }

        impl VarInt for $class_name {
            fn from_raw(leading: u8, trailing: Option<VarBitValue>) -> Self {
                Self { leading, trailing }
            }

            fn leading(&self) -> u8 {
                self.leading
            }

            fn trailing(&self) -> Option<VarBitValue> {
                self.trailing
            }

            fn trailing_bits_for(leading: u8) -> Option<u8> {
                let (size, _) = $base_table.get(leading as usize)?;
                Some(*size)
            }
        }
    };
}

pub trait VarInt {
    fn from_raw(leading: u8, trailing: Option<VarBitValue>) -> Self;

    fn leading(&self) -> u8;

    fn trailing(&self) -> Option<VarBitValue>;

    fn trailing_bits_for(leading: u8) -> Option<u8>;
}

var_uint32!(OffsetType, VARIABLE_OFFSET_BASE_TABLE, 32767);

const VARIABLE_OFFSET_BASE_TABLE: [(u8, u32); 30] = [
    (0, 0),
    (0, 1),
    (0, 2),
    (0, 3),
    (1, 4),
    (1, 6),
    (2, 8),
    (2, 12),
    (3, 16),
    (3, 24),
    (4, 32),
    (4, 48),
    (5, 64),
    (5, 96),
    (6, 128),
    (6, 192),
    (7, 256),
    (7, 384),
    (8, 512),
    (8, 768),
    (9, 1024),
    (9, 1536),
    (10, 2048),
    (10, 3072),
    (11, 4096),
    (11, 6144),
    (12, 8192),
    (12, 12288),
    (13, 16384),
    (13, 24576),
];

var_uint32!(LenType, VARIABLE_LENGTH_BASE_TABLE, 255);

#[allow(dead_code)]
const VARIABLE_LENGTH_BASE_TABLE: [(u8, u32); 29] = [
    (0, 0),
    (0, 1),
    (0, 2),
    (0, 3),
    (0, 4),
    (0, 5),
    (0, 6),
    (0, 7),
    (1, 8),
    (1, 10),
    (1, 12),
    (1, 14),
    (2, 16),
    (2, 20),
    (2, 24),
    (2, 28),
    (3, 32),
    (3, 40),
    (3, 48),
    (3, 56),
    (4, 64),
    (4, 80),
    (4, 96),
    (4, 112),
    (5, 128),
    (5, 160),
    (5, 192),
    (5, 224),
    (0, 255),
];
