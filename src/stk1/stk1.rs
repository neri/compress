//! A compatible library for subset of stk1
//!
//! # CAUTION
//!
//! **THIS LIBRARY IS AN ALPHA VERSION**.
//! Compression and decompression itself is possible, but you will need to provide your own processing for file headers and data size outside the library.
//!
//! # NOTE
//!
//! * stk1 is a simple and easy to decompress compression format.
//! * Since no formal specification exists, this implementation is based on reverse engineering of the decoder.
//!
//! The following _incompatibilities_ exist:
//! * The various limits are not official values.
//! * Only compressed data is supported; headers are not.
//!
//! # Original specifications
//!
//! (C) Kawai Hidemi
//!
//! Related Documents: <http://osask.net/w/196.html> (But different from known final specifications)

use crate::lz::{
    self, Match, SliceWindow,
    cache::{OffsetCache, OffsetCache3},
};
use crate::*;
use alloc::{format, string::String, vec::Vec};

mod s7s;
use s7s::S7s;

const LZ_MAX_LEN: usize = 0x80_00_00;
const LZ_MAX_DISTANCE: usize = 0x02_00_00;

const THRESHOLD_LEN1: usize = 16;

const LZ_SHORT_MIN_LEN: usize = 2;
const LZ_SHORT_MAX_DIST: usize = 8;

const LZ_MIN_MID_LEN: usize = 3;

/// Stk1 coder
pub struct Stk1;

/// Stk1 configuration
#[derive(Debug)]
pub struct Configuration {
    max_distance: usize,
    max_len: usize,
}

impl Configuration {
    /// Tiny Dictionary size (16KB)
    pub const TINY: Self = Self::new(0x4000, 0x4000);

    /// Default Dictionary size (128KB, 8MB)
    pub const DEFAULT: Self = Self::new(LZ_MAX_DISTANCE, LZ_MAX_LEN);

    pub const MAX: Self = Self::new(LZ_MAX_DISTANCE, 0xFF_FF_FF_FF);

    #[inline]
    const fn new(max_distance: usize, max_len: usize) -> Self {
        Self {
            max_distance,
            max_len,
        }
    }

    #[inline]
    pub fn max_distance(&self) -> usize {
        self.max_distance
    }

    #[inline]
    pub fn max_len(&self) -> usize {
        self.max_len
    }
}

impl Default for Configuration {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Stk1 {
    /// Tests if decoding is successful after encoding.
    /// This will take additional execution time and memory consumption compared to normal encoding.
    pub fn encode_with_test(src: &[u8], config: Configuration) -> Result<Vec<u8>, String> {
        let dst = Self::encode(src, config).map_err(|e| format!("ENCODE ERROR: {:?}", e))?;

        let size = src.len();
        let mut temp = Vec::new();
        temp.reserve_exact(size);
        temp.resize(size, 0);

        Self::decode(&dst, &mut temp).map_err(|e| format!("DECODE ERROR: {:?}", e))?;
        if &temp != &src {
            for (index, (p, q)) in src.iter().zip(temp.iter()).enumerate() {
                if *p != *q {
                    return Err(format!(
                        "DECODE ERROR: expected {:02x} but {:02x} at {:08x}",
                        *p, *q, index
                    ));
                }
            }
            return Err(format!("DECODE ERROR: unknown match error"));
        }

        Ok(dst)
    }

    pub fn encode(input: &[u8], config: Configuration) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();

        let mut offset_cache = OffsetCache3::new(input, config.max_distance(), 0);
        let mut lit_buf = SliceWindow::new(input, 0);
        let mut lz_buf = Vec::new();

        let mut cursor = 1;
        offset_cache.advance(cursor);

        while let Some(_) = input.get(cursor) {
            let count = {
                let mut matches = Match::ZERO;

                // Find a long-distance match
                if let Some(iter) = offset_cache.matches() {
                    match lz::find_distance_matches(
                        input,
                        cursor,
                        LZ_MIN_MID_LEN,
                        THRESHOLD_LEN1,
                        offset_cache.guaranteed_min_len(),
                        iter,
                    ) {
                        Some(v) => matches = v,
                        None => {}
                    }
                }

                // Find a short-distance match
                if matches.is_zero() {
                    for distance in 1..=cursor.min(LZ_SHORT_MAX_DIST) {
                        let len = lz::matching_len(input, cursor, distance);
                        if len >= LZ_SHORT_MIN_LEN && matches.len < len {
                            matches = Match { len, distance };
                        }
                    }
                }

                if matches.is_zero() {
                    if lz_buf.len() > 0 {
                        Self::_flush(&mut output, lit_buf, &mut lz_buf)?;
                        lit_buf = SliceWindow::new(input, cursor);
                    } else {
                        lit_buf.expand(1);
                    }
                    1
                } else {
                    matches.len = matches.len.min(config.max_len());
                    lz_buf.push(matches);
                    matches.len
                }
            };
            offset_cache.advance(count);
            cursor += count;
        }
        Self::_flush(&mut output, lit_buf, &mut lz_buf)?;

        Ok(output)
    }

    fn _flush(
        output: &mut Vec<u8>,
        lit_buf: SliceWindow<u8>,
        lz_buf: &mut Vec<Match>,
    ) -> Result<(), EncodeError> {
        // Literals of length 0 are impossible.
        assert!(lit_buf.len() > 0);

        let lit_len = lit_buf.len();
        let lz_count = lz_buf.len();
        let leading = ((if lit_len > 15 { 0 } else { lit_len })
            | ((if lz_count > 15 { 0 } else { lz_count }) << 4)) as u8;
        output.push(leading);
        if lit_len > 15 {
            S7s::write(output, lit_len);
        }
        if lz_count > 15 || lz_count == 0 {
            // Usually the number of LZs is not zero, but may be generated as a set with the last literal in the file
            S7s::write(output, lz_count);
        }

        output.extend_from_slice(lit_buf.into_slice());

        for matches in lz_buf.iter() {
            let lz_len = matches.len - 1;
            let distance = matches.distance - 1;
            let (dist_lead, dist_len, dist_trail) = if distance < 8 {
                ((distance << 1) as u8 | 0x01, 0, 0)
            } else if distance < 0x4_00 {
                (((distance >> 6) & 0x0E) as u8, 1, distance & 0x7F)
            } else if distance < 0x2_00_00 {
                (((distance >> 13) & 0x0E) as u8, 2, distance & 0x3F_FF)
            } else {
                unreachable!()
            };
            let leading = dist_lead | (if lz_len > 15 { 0 } else { (lz_len << 4) as u8 });
            output.push(leading);
            match dist_len {
                1 => {
                    output.push(((dist_trail << 1) as u8) | 1);
                }
                2 => {
                    output.push(((dist_trail >> 6) as u8) & 0xFE);
                    output.push(((dist_trail << 1) as u8) | 1);
                }
                _ => {}
            }
            if lz_len > 15 {
                S7s::write(output, lz_len);
            }
        }

        lz_buf.clear();

        Ok(())
    }

    pub fn decode(input: &[u8], output: &mut [u8]) -> Result<(), DecodeError> {
        let mut iter = input.iter();
        let iter = &mut iter;
        let mut cursor = 0;
        while cursor < output.len() {
            let lead_lz = iter.next().ok_or(DecodeError::InvalidData)?;
            let by = lead_lz & 0x0F;
            let lz = lead_lz >> 4;
            let by = if by == 0 {
                S7s::read(iter).ok_or(DecodeError::InvalidData)?
            } else {
                by as usize
            };
            let lz = if lz == 0 {
                S7s::read(iter).ok_or(DecodeError::InvalidData)?
            } else {
                lz as usize
            };
            for p in iter.take(by) {
                output[cursor] = *p;
                cursor += 1;
            }
            if cursor >= output.len() {
                break;
            }
            for _ in 0..lz {
                let lead_cp = *iter.next().ok_or(DecodeError::InvalidData)?;
                let ds = S7s::read_with_acc(iter, lead_cp as usize & 0x0F)
                    .ok_or(DecodeError::InvalidData)?;
                let ds = ds + 1;
                let cp = lead_cp >> 4;
                let cp = if cp == 0 {
                    S7s::read(iter).ok_or(DecodeError::InvalidData)?
                } else {
                    cp as usize
                };
                let cp = cp + 1;
                if ds > cursor {
                    return Err(DecodeError::InvalidData);
                }
                let cp = cp.min(output.len() - cursor);
                for _ in 0..cp {
                    output[cursor] = output[cursor - ds];
                    cursor += 1;
                }
            }
        }
        Ok(())
    }

    pub fn decode_to_vec(input: &[u8], size: usize) -> Result<Vec<u8>, DecodeError> {
        let mut vec = Vec::new();
        vec.try_reserve_exact(size)
            .map_err(|_| DecodeError::OutOfMemory)?;
        vec.resize(size, 0);
        Self::decode(input, &mut vec).map(|_| vec)
    }
}
