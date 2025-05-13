//! Finite State Entropy coder
//!
//! See also: <https://en.wikipedia.org/wiki/Asymmetric_numeral_systems>

use crate::{num::bits::BitSize, *};

const INIT_PROB: u8 = 0x80;
const INIT_STATE: u32 = 0x1000;

pub const CONTEXT_INITIAL: usize = 0;
pub const CONTEXT_BYTE: usize = 1;
pub const CONTEXT_BYTE_MAX: usize = CONTEXT_BYTE + 255;

/// Finite State Entropy coder
pub struct FSE;

impl FSE {
    pub fn encode_bytes(input: &[u8]) -> Vec<u8> {
        let mut encoder = FseEncoder::new(CONTEXT_BYTE_MAX);
        for &byte in input {
            encoder.encode_byte(byte);
        }
        encoder.finish()
    }

    pub fn decode_bytes(input: &[u8], len: usize) -> Option<Vec<u8>> {
        let mut iter = input.iter().copied();
        let mut decoder = FseDecoder::new(&mut iter, CONTEXT_BYTE_MAX)?;
        let mut result = Vec::with_capacity(len);
        while result.len() < len {
            result.push(decoder.decode_byte()?);
        }
        Some(result)
    }
}

pub struct FseEncoder {
    bits: Vec<(bool, u8)>,
    contexts: ContextState,
}

impl FseEncoder {
    #[inline]
    pub fn new(size: usize) -> Self {
        FseEncoder {
            bits: Vec::new(),
            contexts: ContextState::new(size),
        }
    }

    #[inline]
    pub fn encode_bit(&mut self, bit: bool, context_index: usize) {
        let mut context = self.contexts.context_mut(context_index);
        self.bits.push((bit, context.prob()));
        context.update(bit);
    }

    #[inline]
    pub fn encode_byte(&mut self, value: u8) {
        self.encode_byte_with(value, CONTEXT_BYTE);
    }

    #[inline]
    pub fn encode_byte_with(&mut self, value: u8, context_base: usize) {
        self.encode_consecutive_bits(value as u32, BitSize::OCTET, context_base);
    }

    /// Encodes the specified number of consecutive bit values.
    ///
    /// The value is encoded in big-endian. The context must have a size of a bit power of 2.
    #[inline]
    pub fn encode_consecutive_bits(&mut self, value: u32, bits: BitSize, context_base: usize) {
        let context_base = context_base - 1;
        let mut context_index = 1;
        let mut bit_position = 1u32 << (bits.as_usize() - 1);
        while bit_position > 0 {
            let bit = (value & bit_position) != 0;
            self.encode_bit(bit, context_base + context_index);
            context_index = (context_index << 1) | bit as usize;
            bit_position >>= 1;
        }
    }

    /// Encodes a value with a given number of bits.
    ///
    /// The value is encoded in little-endian.
    #[inline]
    pub fn encode_bit_array(&mut self, value: u32, bits: BitSize, context_base: usize) {
        let mut acc = value;
        for context_index in 0..bits.as_usize() {
            self.encode_bit((acc & 1) != 0, context_base + context_index);
            acc >>= 1;
        }
    }

    pub fn finish(self) -> Vec<u8> {
        let mut result = Vec::new();
        let mut state = INIT_STATE;
        for &data in self.bits.iter().rev() {
            let bit = data.0;
            let prob = data.1 as u32;
            let (start, prob) = if bit { (0, prob) } else { (prob, 256 - prob) };
            let max_state = prob << 12;
            while state >= max_state {
                result.push((state & 0xff) as u8);
                state >>= 8;
            }
            state = ((state / prob) << 8) + (state % prob) + start;
        }
        while state > 0 {
            result.push((state & 0xff) as u8);
            state >>= 8;
        }
        result.reverse();
        result
    }
}

pub struct ContextState {
    contexts: Vec<u8>,
}

impl ContextState {
    #[inline]
    pub fn new(size: usize) -> Self {
        let mut vec = Vec::with_capacity(size);
        vec.resize(size, INIT_PROB);
        Self { contexts: vec }
    }

    #[inline]
    pub fn context_mut(&mut self, index: usize) -> Context {
        Context(&mut self.contexts[index])
    }
}

pub struct Context<'a>(&'a mut u8);

impl Context<'_> {
    #[inline]
    pub fn prob(&self) -> u8 {
        *self.0
    }

    pub fn update(&mut self, bit: bool) {
        let prob = *self.0 as u32;
        if bit {
            *self.0 = (prob + ((256 - prob + 8) >> 4)) as u8;
        } else {
            *self.0 = (prob - ((prob + 8) >> 4)) as u8;
        }
    }
}

pub struct FseDecoder<'a> {
    state: u32,
    contexts: ContextState,
    reader: &'a mut dyn Iterator<Item = u8>,
}

impl FseDecoder<'_> {
    #[inline]
    pub fn new<'a>(reader: &'a mut dyn Iterator<Item = u8>, size: usize) -> Option<FseDecoder<'a>> {
        let mut fse = FseDecoder {
            state: 0,
            contexts: ContextState::new(size),
            reader,
        };
        fse.refill()?;
        Some(fse)
    }

    pub fn refill(&mut self) -> Option<()> {
        while self.state < INIT_STATE {
            let byte = self.reader.next()?;
            self.state = (self.state << 8) | byte as u32;
        }
        Some(())
    }

    pub fn decode_bit(&mut self, context_index: usize) -> Option<bool> {
        self.refill()?;

        let mut context = self.contexts.context_mut(context_index);
        let prob = context.prob() as u32;
        let bit = (self.state & 255) < prob;

        if bit {
            self.state = (prob as u32) * (self.state >> 8) + (self.state & 255);
        } else {
            self.state =
                (256 - prob as u32) * (self.state >> 8) + (self.state & 255) - (prob as u32);
        }
        context.update(bit);

        Some(bit)
    }

    #[inline]
    pub fn decode_byte(&mut self) -> Option<u8> {
        self.decode_byte_with(CONTEXT_BYTE)
    }

    #[inline]
    pub fn decode_byte_with(&mut self, context_base: usize) -> Option<u8> {
        self.decode_consecutive_bits(BitSize::OCTET, context_base)
            .map(|v| v as u8)
    }

    #[inline]
    pub fn decode_consecutive_bits(&mut self, bits: BitSize, context_base: usize) -> Option<u32> {
        let context_base = context_base - 1;
        let mut context_index = 1;
        for _ in 0..bits.as_usize() {
            let bit = self.decode_bit(context_base + context_index)?;
            context_index = (context_index << 1) | bit as usize;
        }
        Some((context_index & 1usize.wrapping_shl(bits.as_u32()).wrapping_sub(1)) as u32)
    }

    #[inline]
    pub fn decode_bit_array(&mut self, bits: BitSize, context_base: usize) -> Option<u32> {
        let mut acc = 0;
        for context_index in 0..bits.as_usize() {
            let bit = self.decode_bit(context_base + context_index)?;
            if bit {
                acc |= 1 << context_index;
            }
        }
        Some(acc)
    }
}
