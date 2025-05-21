//! Deflate decompressor

use super::*;
use crate::entropy::prefix::CanonicalPrefixDecoder;
use crate::num::bits::{BitSize, BitStreamReader};

/// Decompresses a deflate stream.
pub fn inflate(input: &[u8], decode_size: usize) -> Result<Vec<u8>, DecodeError> {
    // In zlib, the first byte is always 08, 78, etc., but a pure deflate stream will never have such a value.
    let leading = *input.get(0).ok_or(DecodeError::UnexpectedEof)?;
    let (skip, _window_size) = if leading & 0x0f == 0x08 {
        // zlib header
        let cmf = leading;
        let flg = *input.get(1).ok_or(DecodeError::UnexpectedEof)?;
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
        reader.advance(skip);
    }

    let mut output = Vec::new();
    output.reserve(decode_size);

    while output.len() < decode_size {
        let bfinal = reader.read_bool().ok_or(DecodeError::UnexpectedEof)?;
        let btype = reader
            .read_bits(BitSize::Bit2)
            .ok_or(DecodeError::UnexpectedEof)?;
        match btype {
            0b00 => {
                // uncompressed block
                let len =
                    u16::from_le_bytes(reader.read_next_bytes().ok_or(DecodeError::UnexpectedEof)?);
                let nlen =
                    u16::from_le_bytes(reader.read_next_bytes().ok_or(DecodeError::UnexpectedEof)?);
                if len != !nlen {
                    return Err(DecodeError::InvalidData);
                }
                output.extend_from_slice(
                    reader
                        .read_next_bytes_slice(len as usize)
                        .ok_or(DecodeError::UnexpectedEof)?,
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

                _decode_block(
                    &mut reader,
                    &mut output,
                    decode_size,
                    &lengths_lit,
                    &lengths_dist,
                )?;
            }
            0b10 => {
                // dynamic Huffman block
                let hlit = 257
                    + reader
                        .read_bits(BitSize::Bit5)
                        .ok_or(DecodeError::UnexpectedEof)? as usize;
                let hdist = 1 + reader
                    .read_bits(BitSize::Bit5)
                    .ok_or(DecodeError::UnexpectedEof)? as usize;
                let mut prefix_table = Vec::new();
                CanonicalPrefixDecoder::decode_prefix_table_deflate(
                    &mut reader,
                    &mut prefix_table,
                    hlit + hdist,
                )?;
                let (lengths_lit, lengths_dist) = prefix_table.split_at(hlit);

                _decode_block(
                    &mut reader,
                    &mut output,
                    decode_size,
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

    if output.len() <= decode_size {
        Ok(output)
    } else {
        // If the decoding result is larger than expected, an error is generated.
        Err(DecodeError::InvalidInput)
    }
}

fn _decode_block(
    reader: &mut BitStreamReader,
    output: &mut Vec<u8>,
    decode_size: usize,
    lengths_lit: &[u8],
    lengths_dist: &[u8],
) -> Result<(), DecodeError> {
    if lengths_dist.len() >= 2 {
        let decoder_lit = CanonicalPrefixDecoder::with_lengths(lengths_lit)?;
        let decoder_dist = CanonicalPrefixDecoder::with_lengths(lengths_dist)?;

        while output.len() < decode_size {
            let lit = decoder_lit.decode(reader)?;
            if lit < 256 {
                // literal
                output.push(lit as u8);
            } else if lit == 256 {
                // end of block
                break;
            } else {
                // length/distance pair
                let len = LenType::decode_value((lit - 257) as u8, reader)
                    .ok_or(DecodeError::InvalidData)? as usize;
                let offset = DistanceType::decode_value(decoder_dist.decode(reader)? as u8, reader)
                    .ok_or(DecodeError::InvalidData)? as usize;

                if offset > output.len() {
                    return Err(DecodeError::InvalidData);
                }
                let copy_len = len.min(decode_size - output.len());
                output.reserve(copy_len);
                for _ in 0..copy_len {
                    output.push(output[output.len() - offset]);
                }
            }
        }
    } else {
        let decoder_lit = CanonicalPrefixDecoder::with_lengths(lengths_lit)?;
        while output.len() < decode_size {
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
