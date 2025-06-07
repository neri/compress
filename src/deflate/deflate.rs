//! Deflate compressor

use super::*;
use crate::{
    entropy::{
        entropy_of,
        prefix::{CanonicalPrefixCoder, CanonicalPrefixDecoder, PermutationFlavor},
    },
    lz::{
        Match,
        lzss::{self, LZSS},
    },
    num::{
        bits::{BitStreamWriter, Write},
        math,
    },
};
use core::f64::{self, INFINITY};

/// Minimum block size in literals
const MIN_BLOCK_SIZE: usize = 16 * 1024;

/// Threshold for static vs dynamic encoding
const THRESHOLD_STATIC: usize = 4096;

#[inline]
pub fn deflate_zlib(
    input: &[u8],
    level: CompressionLevel,
    options: Option<OptionConfig>,
) -> Result<Vec<u8>, EncodeError> {
    deflate(input, level, options.unwrap_or_default().zlib().into())
}

pub fn deflate(
    input: &[u8],
    level: CompressionLevel,
    options: Option<OptionConfig>,
) -> Result<Vec<u8>, EncodeError> {
    let mut config = Configuration::DEFAULT;
    config.level = level;
    config.window_size = WindowSize::preferred(input.len());
    let options = options.unwrap_or_default();

    let mut buff = Vec::with_capacity(config.window_size.value());

    LZSS::encode_lcp(input, config.lzss_config(), |lzss| {
        buff.push(DeflateLZIR::from_lzss(lzss));
        Ok(())
    })?;

    let mut blocks = buff
        .chunks(MIN_BLOCK_SIZE)
        .map(|chunk| DeflateIrBlock::new(chunk))
        .collect::<Vec<_>>();
    let last = blocks.last_mut().unwrap();
    last.is_final = true;

    let mut output = BitStreamWriter::new();
    if options.is_zlib {
        let cmf = ((config.window_size.value().trailing_zeros() as u8 - 8) << 4) | 0x08;
        let mut flg = config.level.zlib_flevel() << 6;
        let fcheck = 31 - (cmf as u16 * 256 + flg as u16) % 31;
        flg |= fcheck as u8;
        output.push_byte(cmf);
        output.push_byte(flg);
    }

    for block in blocks {
        if !config.level.is_fast_method() && block.estimated_size() < THRESHOLD_STATIC {
            let mut ref_static = BitStreamWriter::new();
            block.encode(&mut ref_static, true);
            let mut ref_dynamic = BitStreamWriter::new();
            block.encode(&mut ref_dynamic, false);

            // choose the smaller one
            block.encode(
                &mut output,
                ref_static.bit_count() < ref_dynamic.bit_count(),
            );
        } else {
            block.encode(&mut output, false);
        }
    }

    if options.is_zlib {
        output.skip_to_next_byte_boundary();
        let adler32 = adler32::checksum(input);
        output.write(&adler32.to_be_bytes() as &[u8]);
    }

    Ok(output.into_bytes())
}

/// Intermediate representation of deflate data
///
/// format:
/// * bit 0-8: literal and length
/// * bit 9-13: distance
/// * bit 14-18: length extra bits
/// * bit 19-31: distance extra bits
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeflateLZIR(u32);

impl DeflateLZIR {
    #[allow(unused)]
    pub const END_OF_BLOCK: Self = Self(256);

    pub fn from_lzss(lzss: LZSS) -> Self {
        match lzss {
            LZSS::Literal(literal) => Self::with_literal(literal),
            LZSS::Match(matches) => Self::with_match(matches),
        }
    }

    #[inline]
    pub const fn with_literal(value: u8) -> Self {
        Self(value as u32)
    }

    #[inline]
    pub fn with_match(matches: Match) -> Self {
        let len = LenType::new(matches.len as u32).unwrap();
        let dist = DistanceType::new(matches.distance as u32).unwrap();
        let lit_len = len.leading() as u32 + 257;
        let dist_code = dist.leading() as u32;
        let len_extra = len.trailing().map(|v| v.value()).unwrap_or_default();
        let dist_extra = dist.trailing().map(|v| v.value()).unwrap_or_default();
        Self(lit_len | (dist_code << 9) | (len_extra << 14) | (dist_extra << 19))
    }

    #[inline]
    pub const fn literal_value(&self) -> u32 {
        self.0 & 0x1ff
    }

    #[inline]
    pub const fn distance_value(&self) -> u32 {
        (self.0 >> 9) & 0x1f
    }

    #[inline]
    pub const fn length_extra_bits_raw(&self) -> u32 {
        (self.0 >> 14) & 0x1f
    }

    #[inline]
    pub const fn distance_extra_bits_raw(&self) -> u32 {
        self.0 >> 19
    }

    #[inline]
    pub fn length_extra_bit_size(&self) -> Option<BitSize> {
        match self.literal_value() {
            lit_len @ 257..=285 => LenType::trailing_bits_for((lit_len - 257) as u8),
            _ => None,
        }
    }

    #[inline]
    pub fn length_extra_bits(&self) -> Option<VarBitValue> {
        self.length_extra_bit_size()
            .map(|size| VarBitValue::new(size, self.length_extra_bits_raw()))
    }

    #[inline]
    pub fn distance_extra_bit_size(&self) -> Option<BitSize> {
        match self.distance_value() {
            dist @ 0..=29 => DistanceType::trailing_bits_for(dist as u8),
            _ => None,
        }
    }

    #[inline]
    pub fn distance_extra_bits(&self) -> Option<VarBitValue> {
        self.distance_extra_bit_size()
            .map(|size| VarBitValue::new(size, self.distance_extra_bits_raw()))
    }
}

#[derive(Clone)]
pub struct DeflateIrBlock<'a> {
    block: &'a [DeflateLZIR],
    estimated_size: usize,
    freq_count_lit: Box<[usize; 288]>,
    freq_count_dist: Box<[usize; 30]>,
    entropy_lit: f64,
    entropy_dist: f64,

    /// This flag must be set to `true` only for the last block.
    pub is_final: bool,
}

#[allow(unused)]
impl<'a> DeflateIrBlock<'a> {
    pub fn new(block: &'a [DeflateLZIR]) -> Self {
        let mut freq_count_lit = Box::new([0usize; 288]);
        let mut freq_count_dist = Box::new([0usize; 30]);

        for &item in block.iter() {
            let lit = item.literal_value() as usize;
            let dist = item.distance_value() as usize;
            freq_count_lit[lit] += 1;
            freq_count_dist[dist] += 1;
        }
        freq_count_lit[256] = 1; // end of block

        let entropy_lit = entropy_of(freq_count_lit.as_ref());
        let entropy_dist = entropy_of(freq_count_dist.as_ref());

        let estimated_size = Self::_estimated_size(
            freq_count_lit.iter().sum::<usize>(),
            entropy_lit,
            freq_count_dist.iter().sum::<usize>(),
            entropy_dist,
        );

        Self {
            block,
            freq_count_lit,
            freq_count_dist,
            entropy_lit,
            entropy_dist,
            estimated_size,
            is_final: false,
        }
    }

    /// Merge two contiguous blocks.
    ///
    /// # Panics
    ///
    /// Panic if `self` and `next` are not contiguous.
    pub fn merged(&self, next: &Self) -> Self {
        let new_block = unsafe {
            // Safety: `self.block` and `next.block` must be contiguous.
            let self_next = self.block.as_ptr().add(self.block.len());
            assert_eq!(self_next, next.block.as_ptr());
            core::slice::from_raw_parts(self.block.as_ptr(), self.block.len() + next.block.len())
        };

        let mut freq_count_lit = self.freq_count_lit.clone();
        let mut freq_count_dist = self.freq_count_dist.clone();
        for (p, q) in freq_count_lit.iter_mut().zip(next.freq_count_lit.iter()) {
            *p += *q;
        }
        for (p, q) in freq_count_dist.iter_mut().zip(next.freq_count_dist.iter()) {
            *p += *q;
        }
        freq_count_lit[256] = 1; // fix end of block

        let entropy_lit = entropy_of(freq_count_lit.as_ref());
        let entropy_dist = entropy_of(freq_count_dist.as_ref());

        let estimated_size = Self::_estimated_size(
            freq_count_lit.iter().sum::<usize>(),
            entropy_lit,
            freq_count_dist.iter().sum::<usize>(),
            entropy_dist,
        );

        Self {
            block: new_block,
            freq_count_lit,
            freq_count_dist,
            entropy_lit,
            entropy_dist,
            estimated_size,
            is_final: false,
        }
    }

    /// Return the value of `is_final`. This flag must be set to `true` only for the last block.
    #[inline]
    pub const fn is_final(&self) -> bool {
        self.is_final
    }

    fn _estimated_size(
        lit_len: usize,
        entropy_lit: f64,
        dist_len: usize,
        entropy_dist: f64,
    ) -> usize {
        let lit_len_size = math::ceil(lit_len as f64 * entropy_lit.clamp(1.0, INFINITY));
        let dist_size = if entropy_dist > 0.0 {
            math::ceil(dist_len as f64 * entropy_dist.clamp(1.0, INFINITY))
        } else {
            0.0
        };
        0 + math::ceil((lit_len_size + dist_size) / 8.0) as usize
    }

    #[inline]
    pub fn freq_count_lit(&self) -> &[usize] {
        self.freq_count_lit.as_ref()
    }

    #[inline]
    pub fn freq_count_dist(&self) -> &[usize] {
        self.freq_count_dist.as_ref()
    }

    #[inline]
    pub const fn total_entropy(&self) -> f64 {
        self.entropy_lit + self.entropy_dist
    }

    /// Returns the number of elements in the block.
    #[inline]
    pub const fn n_elements(&self) -> usize {
        self.block.len()
    }

    /// Desired data size estimated from entropy.
    ///
    /// Since the actual encoded data size will be larger than this due to the following factors,
    /// it is used only to estimate the compression effect.
    /// * Block header overhead
    /// * Huffman code overhead
    /// * Additional bits for length and distance
    #[inline]
    pub const fn estimated_size(&self) -> usize {
        self.estimated_size
    }

    /// Encode the block to the output stream.
    pub fn encode(&self, output: &mut BitStreamWriter, use_static: bool) {
        let (prefix_table_lit, prefix_table_dist) = if use_static {
            let mut lengths_lit = [0u8; 288];
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
            let mut prefix_table_lit = Vec::with_capacity(288);
            prefix_table_lit.resize(288, None);
            for (index, value) in
                CanonicalPrefixDecoder::reorder_prefix_table(lengths_lit.into_iter().enumerate())
                    .unwrap()
            {
                prefix_table_lit[index] = Some(value);
            }

            let prefix_table_dist = (0..30)
                .map(|v| Some(VarBitValue::new(BitSize::Bit5, v as u32)))
                .collect::<Vec<_>>();

            (prefix_table_lit, prefix_table_dist)
        } else {
            let prefix_table_lit =
                CanonicalPrefixCoder::make_prefix_table(self.freq_count_lit(), BitSize::Bit15, 257);
            let mut prefix_table_dist =
                CanonicalPrefixCoder::make_prefix_table(self.freq_count_dist(), BitSize::Bit15, 1);

            // fix prefix table for dist
            let prefix_table_dist_count = prefix_table_dist.iter().filter(|v| v.is_some()).count();
            if prefix_table_dist_count == 0 {
                prefix_table_dist.push(Some(VarBitValue::with_bool(true)));
                prefix_table_dist.push(Some(VarBitValue::with_bool(true)));
            } else if prefix_table_dist_count < 2 {
                prefix_table_dist.push(Some(VarBitValue::with_bool(true)));
            }

            (prefix_table_lit, prefix_table_dist)
        };

        output.write(self.is_final()); // bfinal
        if use_static {
            output.write(VarBitValue::new(BitSize::Bit2, 0b01)); // btype
        } else {
            let prefix_tables = prefix_table_lit
                .iter()
                .chain(prefix_table_dist.iter())
                .map(|v| v.map(|v| v.size().as_u8()).unwrap_or_default())
                .collect::<Vec<_>>();
            let prefix_tables = CanonicalPrefixCoder::encode_prefix_tables(
                &[&prefix_tables],
                PermutationFlavor::Deflate,
            )
            .unwrap();

            output.write(VarBitValue::new(BitSize::Bit2, 0b10)); // btype
            output.write(VarBitValue::new(
                BitSize::Bit5,
                prefix_table_lit.len() as u32 - 257,
            )); // hlit
            output.write(VarBitValue::new(
                BitSize::Bit5,
                prefix_table_dist.len() as u32 - 1,
            )); // hdist
            output.write(prefix_tables.hclen); // hclen
            output.write(prefix_tables.prefix_table.as_slice());
            output.write(prefix_tables.content.as_slice());
        }

        for lzir in self.block.iter() {
            let lit_len = lzir.literal_value();
            output.write(prefix_table_lit[lit_len as usize].unwrap().reversed());
            if lit_len > 256 {
                if let Some(len_extra) = lzir.length_extra_bits() {
                    output.write(len_extra);
                }
                let dist = lzir.distance_value();
                output.write(prefix_table_dist[dist as usize].unwrap().reversed());
                if let Some(dist_extra) = lzir.distance_extra_bits() {
                    output.write(dist_extra);
                }
            }
        }
        output.write(prefix_table_lit[256].unwrap().reversed()); // end of block
    }
}

#[allow(unused)]
pub struct Configuration {
    pub level: CompressionLevel,
    pub window_size: WindowSize,
}

impl Configuration {
    pub const DEFAULT: Self = Self {
        level: CompressionLevel::Default,
        window_size: WindowSize::Size32768,
    };

    pub fn lzss_config(&self) -> lzss::Configuration {
        let window_size = self.window_size.value();
        let max_len = window_size.min(258);
        let skip_first_literal = 1;
        if self.level.is_fast_method() {
            lzss::Configuration::new(window_size, max_len, skip_first_literal, 1, 3, 0)
        } else {
            lzss::Configuration::new(window_size, max_len, skip_first_literal, 0, 0, 0)
        }
    }
}

pub struct OptionConfig {
    is_zlib: bool,
}

impl OptionConfig {
    #[inline]
    pub const fn new() -> Self {
        Self { is_zlib: false }
    }

    #[inline]
    pub const fn zlib(mut self) -> Self {
        self.is_zlib = true;
        self
    }
}

impl Default for OptionConfig {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
