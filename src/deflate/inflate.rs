//! Deflate decompressor

use super::*;
use crate::entropy::prefix::{CanonicalPrefixDecoder, LitLen2};
use crate::lz::LzOutputBuffer;
use crate::num::bits::{BitSize, BitStreamReader};

/// Decompresses a deflate stream into a new vector.
pub fn inflate(input: &[u8], decode_size: usize) -> Result<Vec<u8>, DecodeError> {
    let mut output = Vec::new();
    output.resize(decode_size, 0);
    inflate_in_place(input, &mut output)?;
    Ok(output)
}

/// Decompresses a deflate stream in place into the provided output buffer.
pub fn inflate_in_place(input: &[u8], output: &mut [u8]) -> Result<(), DecodeError> {
    let mut output = LzOutputBuffer::new(output);

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

    while !output.is_eof() {
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
                output
                    .extend_from_slice(
                        reader
                            .read_next_bytes_slice(len as usize)
                            .ok_or(DecodeError::UnexpectedEof)?,
                    )
                    .ok_or(DecodeError::InvalidData)?;
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

                _decode_block(&mut reader, &mut output, &lengths_lit, &lengths_dist)?;
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

                _decode_block(&mut reader, &mut output, lengths_lit, lengths_dist)?;
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

    Ok(())
}

fn _decode_block(
    reader: &mut BitStreamReader,
    output: &mut LzOutputBuffer,
    lengths_lit: &[u8],
    lengths_dist: &[u8],
) -> Result<(), DecodeError> {
    if lengths_dist.len() >= 2 {
        let decoder_lit = CanonicalPrefixDecoder::with_lengths(lengths_lit, true)?;
        let decoder_dist = CanonicalPrefixDecoder::with_lengths(lengths_dist, false)?;

        while !output.is_eof() {
            match decoder_lit.decode2(reader)? {
                LitLen2::Single(lit) => {
                    // literal
                    let _ = output.push_literal(lit);
                }
                LitLen2::Double(lit1, lit2) => {
                    // two literals
                    let _ = output.push_literal(lit1);
                    let _ = output.push_literal(lit2);
                }
                LitLen2::Length(lit) => {
                    // length/distance pair
                    let len = LenType::decode_value(lit, reader).ok_or(DecodeError::InvalidData)?
                        as usize;
                    let distance =
                        DistanceType::decode_value(decoder_dist.decode(reader)? as u8, reader)
                            .ok_or(DecodeError::InvalidData)? as usize;

                    output
                        .copy_lz(distance, len)
                        .ok_or(DecodeError::InvalidData)?;
                }
                LitLen2::EndOfBlock(_, _, _) => {
                    // end of block
                    break;
                }
            }
        }
    } else {
        let decoder_lit = CanonicalPrefixDecoder::with_lengths(lengths_lit, false)?;
        while !output.is_eof() {
            let lit = decoder_lit.decode(reader)?;
            if lit < 256 {
                // literal
                let _ = output.push_literal(lit as u8);
            } else if lit == 256 {
                // end of block
                break;
            }
        }
    }

    Ok(())
}
