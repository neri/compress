//! Deflate compression algorithm
//!
//! See also: <https://www.ietf.org/rfc/rfc1951.txt>

use crate::entropy::prefix::CanonicalPrefixDecoder;
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
        let (skip, window_size) = if leading & 0x0f == 0x08 {
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
            let window_size = 256usize << ((cmf_flg >> 4) & 0x0f);
            (Some(BitSize::Bit16), window_size)
        } else {
            (None, 0x8000)
        };

        let mut reader = BitStreamReader::new(input);
        if let Some(skip) = skip {
            reader.read_bits(skip).ok_or(DecodeError::InvalidData)?;
        }

        let mut output = Vec::new();
        if limit_len < usize::MAX {
            output.reserve(limit_len);
        }

        while output.len() < limit_len {
            let bfinal = reader.read_bool().ok_or(DecodeError::InvalidData)?;
            let btype = reader
                .read_bits(BitSize::Bit2)
                .ok_or(DecodeError::InvalidData)?;
            match btype {
                0b00 => {
                    // uncompressed block
                    let len = u16::from_le_bytes(
                        reader.read_next_bytes().ok_or(DecodeError::InvalidData)?,
                    );
                    let nlen = u16::from_le_bytes(
                        reader.read_next_bytes().ok_or(DecodeError::InvalidData)?,
                    );
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
                        window_size,
                        limit_len,
                        &lengths_lit,
                        &lengths_dist,
                    )?;
                }
                0b10 => {
                    // dynamic Huffman block
                    let hlit = 257
                        + reader
                            .read_bits(BitSize::Bit5)
                            .ok_or(DecodeError::InvalidData)? as usize;
                    let hdist = 1 + reader
                        .read_bits(BitSize::Bit5)
                        .ok_or(DecodeError::InvalidData)?
                        as usize;
                    let mut prefix_table = Vec::new();
                    CanonicalPrefixDecoder::decode_prefix_table_deflate(
                        &mut reader,
                        &mut prefix_table,
                        hlit + hdist,
                    )?;
                    let (lengths_lit, lengths_dist) = prefix_table.split_at(hlit);

                    Self::_decode_block(
                        &mut reader,
                        &mut output,
                        window_size,
                        limit_len,
                        lengths_lit,
                        lengths_dist,
                    )?;
                }
                _ => {
                    // reserved (error)
                    return Err(DecodeError::InvalidData);
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
        window_size: usize,
        limit_len: usize,
        lengths_lit: &[u8],
        lengths_dist: &[u8],
    ) -> Result<(), DecodeError> {
        output.reserve(window_size);
        let decoder_lit = CanonicalPrefixDecoder::with_lengths(lengths_lit)?;
        if lengths_dist.len() >= 2 {
            let decoder_dist = CanonicalPrefixDecoder::with_lengths(lengths_dist)?;

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
                    let len = 3 + LenType::decode_value((lit - 257) as u8, reader)
                        .ok_or(DecodeError::InvalidData)?
                        as usize;
                    let offset =
                        1 + OffsetType::decode_value(decoder_dist.decode(reader)? as u8, reader)
                            .ok_or(DecodeError::InvalidData)? as usize;

                    if offset > output.len() {
                        return Err(DecodeError::InvalidData);
                    }
                    let copy_len = len.min(limit_len - output.len());
                    output.reserve(copy_len);
                    for _ in 0..copy_len {
                        output.push(output[output.len() - offset]);
                    }
                }
            }
        } else {
            while output.len() < limit_len {
                let lit = decoder_lit.decode(reader)?;
                if lit < 256 {
                    // literal
                    output.push(lit as u8);
                } else if lit == 256 {
                    // end of block
                    break;
                }
            }
        }

        Ok(())
    }
}

macro_rules! var_uint32 {
    ($class_name:ident, $base_table:ident, $max_value:expr) => {
        #[derive(Debug, PartialEq)]
        pub struct $class_name {
            pub trailing: Option<VarBitValue>,
            pub leading: u8,
        }

        impl $class_name {
            pub const MAX: u32 = $max_value;

            #[inline]
            pub fn new(value: u32) -> Option<Self> {
                for (index, &item) in $base_table.iter().enumerate().rev() {
                    let (size, min_value) = item;
                    if value < min_value {
                        continue;
                    }
                    let leading = index as u8;
                    let value = value.checked_sub(min_value)?;
                    let max_value = (1u32 << size.map(|v| v as u32).unwrap_or_default()) - 1;
                    if value > max_value {
                        return None;
                    }
                    let trailing = size.map(|size| VarBitValue::new(size, value));
                    return Some(Self { leading, trailing });
                }
                None
            }

            #[inline]
            pub fn value(&self) -> u32 {
                $base_table[self.leading as usize].1
                    + self.trailing.map(|v| v.value()).unwrap_or_default()
            }

            pub fn decode(leading: u8, reader: &mut BitStreamReader) -> Option<Self> {
                let (ext_bit, _min_value) = *($base_table.get(leading as usize)?);
                if let Some(ext_bit) = ext_bit {
                    let trailing = reader
                        .read_bits(ext_bit)
                        .map(|v| VarBitValue::new(ext_bit, v))?;
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

            #[inline]
            pub fn decode_value(leading: u8, reader: &mut BitStreamReader) -> Option<u32> {
                let (ext_bit, min_value) = *($base_table.get(leading as usize)?);
                if let Some(ext_bit) = ext_bit {
                    let trailing = reader.read_bits(ext_bit)?;
                    Some(min_value + trailing)
                } else {
                    Some(min_value)
                }
            }

            pub fn from_raw(leading: u8, trailing: Option<VarBitValue>) -> Self {
                Self { leading, trailing }
            }

            pub fn leading(&self) -> u8 {
                self.leading
            }

            pub fn trailing(&self) -> Option<VarBitValue> {
                self.trailing
            }

            pub fn trailing_bits_for(leading: u8) -> Option<BitSize> {
                let (size, _) = $base_table.get(leading as usize)?;
                *size
            }
        }
    };
}

var_uint32!(OffsetType, VARIABLE_OFFSET_BASE_TABLE, 32767);

static VARIABLE_OFFSET_BASE_TABLE: [(Option<BitSize>, u32); 30] = [
    (None, 0),
    (None, 1),
    (None, 2),
    (None, 3),
    (Some(BitSize::Bit1), 4),
    (Some(BitSize::Bit1), 6),
    (Some(BitSize::Bit2), 8),
    (Some(BitSize::Bit2), 12),
    (Some(BitSize::Bit3), 16),
    (Some(BitSize::Bit3), 24),
    (Some(BitSize::Bit4), 32),
    (Some(BitSize::Bit4), 48),
    (Some(BitSize::Bit5), 64),
    (Some(BitSize::Bit5), 96),
    (Some(BitSize::Bit6), 128),
    (Some(BitSize::Bit6), 192),
    (Some(BitSize::Bit7), 256),
    (Some(BitSize::Bit7), 384),
    (Some(BitSize::Bit8), 512),
    (Some(BitSize::Bit8), 768),
    (Some(BitSize::Bit9), 1024),
    (Some(BitSize::Bit9), 1536),
    (Some(BitSize::Bit10), 2048),
    (Some(BitSize::Bit10), 3072),
    (Some(BitSize::Bit11), 4096),
    (Some(BitSize::Bit11), 6144),
    (Some(BitSize::Bit12), 8192),
    (Some(BitSize::Bit12), 12288),
    (Some(BitSize::Bit13), 16384),
    (Some(BitSize::Bit13), 24576),
];

var_uint32!(LenType, VARIABLE_LENGTH_BASE_TABLE, 255);

static VARIABLE_LENGTH_BASE_TABLE: [(Option<BitSize>, u32); 29] = [
    (None, 0),
    (None, 1),
    (None, 2),
    (None, 3),
    (None, 4),
    (None, 5),
    (None, 6),
    (None, 7),
    (Some(BitSize::Bit1), 8),
    (Some(BitSize::Bit1), 10),
    (Some(BitSize::Bit1), 12),
    (Some(BitSize::Bit1), 14),
    (Some(BitSize::Bit2), 16),
    (Some(BitSize::Bit2), 20),
    (Some(BitSize::Bit2), 24),
    (Some(BitSize::Bit2), 28),
    (Some(BitSize::Bit3), 32),
    (Some(BitSize::Bit3), 40),
    (Some(BitSize::Bit3), 48),
    (Some(BitSize::Bit3), 56),
    (Some(BitSize::Bit4), 64),
    (Some(BitSize::Bit4), 80),
    (Some(BitSize::Bit4), 96),
    (Some(BitSize::Bit4), 112),
    (Some(BitSize::Bit5), 128),
    (Some(BitSize::Bit5), 160),
    (Some(BitSize::Bit5), 192),
    (Some(BitSize::Bit5), 224),
    (None, 255),
];
