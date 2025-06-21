//! Lempel–Ziv–Storer–Szymanski style compression code
//!
//! <https://en.wikipedia.org/wiki/Lempel%E2%80%93Ziv%E2%80%93Storer%E2%80%93Szymanski>
//!

use super::match_finder::MatchFinder;
use crate::{
    EncodeError,
    lz::{cache::*, *},
    *,
};
use core::convert::Infallible;

#[derive(Debug)]
pub struct Configuration {
    pub max_distance: usize,
    pub max_len: usize,
    pub skip_first_literal: usize,
    pub number_of_attempts: usize,
    pub threshold_len: usize,
    pub cache_purge_limit: usize,
}

impl Configuration {
    /// Default Dictionary size
    pub const DEFAULT: Self = Self::new(LZSS::MAX_DISTANCE, LZSS::MAX_LEN);

    // default number of attempts
    pub const DEFAULT_ATTEMPTS: usize = 10;

    pub const LONG_ATTEMPTS: usize = 100;

    pub const THRESHOLD_LEN: usize = 16;

    pub const LONG_THRESHOLD_LEN: usize = 64;

    // 16M = 128MB
    pub const CACHE_PURGE_LIMIT: usize = 16 * 1024 * 1024;

    #[inline]
    pub const fn new(max_distance: usize, max_len: usize) -> Self {
        Self {
            max_distance: if max_distance > LZSS::MAX_DISTANCE {
                LZSS::MAX_DISTANCE
            } else {
                max_distance
            },
            max_len: if max_len > LZSS::MAX_LEN {
                LZSS::MAX_LEN
            } else {
                max_len
            },
            skip_first_literal: 0,
            number_of_attempts: Self::DEFAULT_ATTEMPTS,
            threshold_len: Self::THRESHOLD_LEN,
            cache_purge_limit: Self::CACHE_PURGE_LIMIT,
        }
    }

    #[inline]
    pub const fn skip_first_literal(mut self, skip_first_literal: usize) -> Self {
        self.skip_first_literal = skip_first_literal;
        self
    }

    #[inline]
    pub const fn number_of_attempts(mut self, number_of_attempts: usize) -> Self {
        self.number_of_attempts = number_of_attempts;
        self
    }

    #[inline]
    pub const fn threshold_len(mut self, threshold_len: usize) -> Self {
        self.threshold_len = threshold_len;
        self
    }
}

impl Default for Configuration {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LZSS {
    Literal(u8),
    Match(Match),
}

pub struct LzssBuffer {
    inner: Vec<LZSSIR>,
}

impl LZSS {
    /// Minimum match length in LZSS
    pub const MIN_LEN: usize = 3;

    pub const MAX_LEN: usize = Self::MIN_LEN + 4096;

    pub const MAX_DISTANCE: usize = 0x10_0000;

    const THRESHOLD_LEN_2D: usize = 8;

    /// Encode in the fastest way possible
    pub fn encode_fast<F>(input: &[u8], config: Configuration, mut f: F) -> Result<(), EncodeError>
    where
        F: FnMut(LZSS) -> Result<(), EncodeError>,
    {
        if input.is_empty() || input.len() > i32::MAX as usize {
            return Err(EncodeError::InvalidInput);
        }

        let mut offset3_cache =
            OffsetCache3::new(input, config.max_distance, config.cache_purge_limit);

        let mut current = 1 + config.skip_first_literal;
        for &literal in input.iter().take(current) {
            f(LZSS::Literal(literal))?;
        }
        offset3_cache.advance(current);

        let guaranteed_min_len = offset3_cache.guaranteed_min_len();
        assert_eq!(guaranteed_min_len, 3);
        let max_len = config.max_len;

        while let Some(&literal) = input.get(current) {
            let count = {
                let mut matches = Match::ZERO;

                if let Some(mut iter) = offset3_cache.matches() {
                    if let Some(distance) = iter.next() {
                        let len = lz::matching_len(input, current + guaranteed_min_len, distance);
                        matches = Match::new(len + guaranteed_min_len, distance);
                    }
                }

                if matches.len > 0 {
                    let mut total_len = 0;
                    let mut left = matches.len;
                    loop {
                        if left > max_len {
                            f(LZSS::Match(Match::new(max_len, matches.distance)))?;
                            left -= max_len;
                            total_len += max_len;
                        } else if left >= LZSS::MIN_LEN {
                            f(LZSS::Match(Match::new(left, matches.distance)))?;
                            total_len += left;
                            break;
                        } else {
                            break;
                        }
                    }
                    total_len
                } else {
                    f(LZSS::Literal(literal))?;
                    1
                }
            };
            offset3_cache.advance(count);
            current += count;
        }

        Ok(())
    }

    /// Encode LZSS using hash algorithm
    pub fn encode<F>(input: &[u8], config: Configuration, mut f: F) -> Result<(), EncodeError>
    where
        F: FnMut(LZSS) -> Result<(), EncodeError>,
    {
        if input.is_empty() || input.len() > i32::MAX as usize {
            return Err(EncodeError::InvalidInput);
        }

        let mut offset3_cache =
            OffsetCache3::new(input, config.max_distance, config.cache_purge_limit);

        let mut current = 1 + config.skip_first_literal;
        for &literal in input.iter().take(current) {
            f(LZSS::Literal(literal))?;
        }
        offset3_cache.advance(current);

        let guaranteed_min_len = offset3_cache.guaranteed_min_len();
        assert_eq!(guaranteed_min_len, 3);
        let max_len = config.max_len;

        while let Some(&literal) = input.get(current) {
            let count = {
                let mut matches = Match::ZERO;

                if let Some(iter) = offset3_cache.matches() {
                    match lz::find_distance_matches(
                        input,
                        current,
                        Self::MIN_LEN,
                        config.threshold_len,
                        offset3_cache.guaranteed_min_len(),
                        iter.take(config.number_of_attempts),
                    ) {
                        Some(v) => {
                            matches = v;
                        }
                        None => {}
                    }
                }

                if matches.len > 0 {
                    let mut total_len = 0;
                    let mut left = matches.len;
                    loop {
                        if left > max_len {
                            f(LZSS::Match(Match::new(max_len, matches.distance)))?;
                            left -= max_len;
                            total_len += max_len;
                        } else if left >= LZSS::MIN_LEN {
                            f(LZSS::Match(Match::new(left, matches.distance)))?;
                            total_len += left;
                            break;
                        } else {
                            break;
                        }
                    }
                    total_len
                } else {
                    f(LZSS::Literal(literal))?;
                    1
                }
            };
            offset3_cache.advance(count);
            current += count;
        }

        Ok(())
    }

    /// Encode LZSS with Longest Common Prefix array compression (experimental)
    pub fn encode_lcp<F>(input: &[u8], config: Configuration, mut f: F) -> Result<(), EncodeError>
    where
        F: FnMut(LZSS) -> Result<(), EncodeError>,
    {
        if input.is_empty() || input.len() > i32::MAX as usize {
            return Err(EncodeError::InvalidInput);
        }

        let mut current = 1 + config.skip_first_literal;
        for &literal in input.iter().take(current) {
            f(LZSS::Literal(literal))?;
        }

        let window_size = 0x100000;
        let mut low = 0;
        let mut high = input.len().min(window_size);
        let mut threshold = if window_size >= input.len() {
            input.len()
        } else {
            window_size - config.max_len
        };
        loop {
            let input2 = &input[low..high];
            let finder: MatchFinder<'_> = MatchFinder::new(input2);
            while let Some(&literal) = input2.get(current) {
                let count = {
                    let matches = finder.matches(current, LZSS::MIN_LEN, config.max_distance);

                    if matches.len > 0 {
                        let mut total_len = 0;
                        let mut left = matches.len;
                        loop {
                            if left > config.max_len {
                                f(LZSS::Match(Match::new(config.max_len, matches.distance)))?;
                                left -= config.max_len;
                                total_len += config.max_len;
                                if current + total_len >= threshold {
                                    break;
                                }
                            } else if left >= LZSS::MIN_LEN {
                                f(LZSS::Match(Match::new(left, matches.distance)))?;
                                total_len += left;
                                break;
                            } else {
                                break;
                            }
                        }
                        total_len
                    } else {
                        f(LZSS::Literal(literal))?;
                        1
                    }
                };
                current += count;
            }
            if low + current == input.len() {
                break;
            }
            low += current - window_size / 2;
            high = (low + window_size).min(input.len());
            current = window_size / 2;
            threshold = window_size;
        }

        Ok(())
    }

    /// Encode LZSS
    pub fn encode_old<F>(
        input: &[u8],
        distance_mapping: Option<&[usize]>,
        config: Configuration,
        needs_buffer: bool,
        mut f: F,
    ) -> Result<LzssBuffer, EncodeError>
    where
        F: FnMut(LZSS) -> Result<(), EncodeError>,
    {
        if input.is_empty() {
            return Err(EncodeError::InvalidInput);
        }

        let distance_base = if let Some(distance_mapping) = distance_mapping {
            distance_mapping.len() + 1
        } else {
            0
        };
        let max_distance = config.max_distance - distance_base;
        let search_attempts = config.number_of_attempts;
        let threshold_len = config.threshold_len;

        let mut offset3_cache = OffsetCache3::new(input, max_distance, config.cache_purge_limit);

        let mut buf = Vec::new();
        let mut lit_buf = SliceWindow::new(input, 0);
        lit_buf.expand(config.skip_first_literal);
        let mut cursor = lit_buf.len();
        offset3_cache.advance(cursor);

        let flush_lit = |lit_buf: SliceWindow<u8>, buf: &mut Vec<LZSSIR>, f: &mut F| {
            let slice = lit_buf.into_slice();
            for &byte in slice {
                f(LZSS::Literal(byte))?;
            }
            if needs_buffer {
                LZSSIR::encode_literals(slice, buf);
            }
            Result::<(), EncodeError>::Ok(())
        };

        let mut lit_buf = Some(lit_buf);
        if let Some(distance_mapping) = distance_mapping {
            while let Some(_) = input.get(cursor) {
                let count = {
                    let mut matches = Match::ZERO;

                    // Find 2d distance matches
                    for (dist_code, distance) in distance_mapping.iter().enumerate() {
                        if *distance > cursor {
                            continue;
                        }
                        let len = lz::matching_len(input, cursor, *distance);
                        if matches.len < len {
                            matches = Match::new(len, dist_code + 1);
                            if matches.len >= Self::THRESHOLD_LEN_2D {
                                break;
                            }
                        }
                    }

                    if matches.is_zero() {
                        if let Some(iter) = offset3_cache.matches() {
                            match lz::find_distance_matches(
                                input,
                                cursor,
                                Self::MIN_LEN,
                                threshold_len,
                                offset3_cache.guaranteed_min_len(),
                                iter.take(search_attempts),
                            ) {
                                Some(mut v) => {
                                    v.distance += distance_base;
                                    matches = v;
                                }
                                None => {}
                            }
                        }
                    }

                    if matches.len >= LZSS::MIN_LEN as usize {
                        if let Some(lit_buf) = lit_buf {
                            flush_lit(lit_buf, &mut buf, &mut f)?;
                        }
                        lit_buf = None;

                        matches.len = matches.len.min(config.max_len);
                        f(LZSS::Match(matches))?;
                        if needs_buffer {
                            buf.push(LZSSIR::with_match(matches));
                        }

                        matches.len
                    } else {
                        if let Some(ref mut lit_buf) = lit_buf {
                            lit_buf.expand(1);
                        } else {
                            lit_buf = Some(SliceWindow::new(input, cursor));
                        }
                        1
                    }
                };
                offset3_cache.advance(count);
                cursor += count;
            }
        } else {
            while let Some(_) = input.get(cursor) {
                let count = {
                    let mut matches = Match::ZERO;

                    if matches.is_zero() {
                        if let Some(iter) = offset3_cache.matches() {
                            match lz::find_distance_matches(
                                input,
                                cursor,
                                Self::MIN_LEN,
                                threshold_len,
                                offset3_cache.guaranteed_min_len(),
                                iter.take(search_attempts),
                            ) {
                                Some(v) => {
                                    matches = v;
                                }
                                None => {}
                            }
                        }
                    }

                    if matches.len >= LZSS::MIN_LEN as usize {
                        if let Some(lit_buf) = lit_buf {
                            flush_lit(lit_buf, &mut buf, &mut f)?;
                        }
                        lit_buf = None;

                        matches.len = matches.len.min(config.max_len);
                        f(LZSS::Match(matches))?;
                        if needs_buffer {
                            buf.push(LZSSIR::with_match(matches));
                        }

                        matches.len
                    } else {
                        if let Some(ref mut lit_buf) = lit_buf {
                            lit_buf.expand(1);
                        } else {
                            lit_buf = Some(SliceWindow::new(input, cursor));
                        }
                        1
                    }
                };
                offset3_cache.advance(count);
                cursor += count;
            }
        }

        if let Some(lit_buf) = lit_buf {
            flush_lit(lit_buf, &mut buf, &mut f)?;
        }

        if !needs_buffer {
            assert_eq!(buf.len(), 0);
        }

        Ok(LzssBuffer { inner: buf })
    }
}

impl LzssBuffer {
    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(LZSS),
    {
        self.try_for_each(|lzss| {
            f(lzss);
            Result::<(), Infallible>::Ok(())
        })
        .unwrap();
    }

    pub fn try_for_each<F, E>(&self, mut f: F) -> Result<(), E>
    where
        F: FnMut(LZSS) -> Result<(), E>,
    {
        for ir in self.inner.iter() {
            ir.try_interpret(&mut f)?;
        }
        Ok(())
    }
}

/// LZSS Intermediate Representation
struct LZSSIR(u64);

impl LZSSIR {
    const MAX_LITERALS: usize = 7;

    #[track_caller]
    fn with_literals(literals: &[u8]) -> Self {
        assert!(!literals.is_empty());
        assert!(literals.len() <= Self::MAX_LITERALS);
        let mut acc = 0;
        literals.iter().rev().for_each(|&v| {
            acc = (acc << 8) | v as u64;
        });
        Self(acc << 8 | (literals.len() as u64))
    }

    fn encode_literals(literals: &[u8], buf: &mut Vec<Self>) {
        for bytes in literals.chunks(Self::MAX_LITERALS) {
            buf.push(LZSSIR::with_literals(bytes));
        }
    }

    #[track_caller]
    fn with_match(matches: Match) -> Self {
        assert!(
            matches.len >= LZSS::MIN_LEN,
            "len {} > {}",
            matches.len,
            LZSS::MIN_LEN
        );
        assert!(
            matches.len <= LZSS::MAX_LEN,
            "len {} < {}",
            matches.len,
            LZSS::MAX_LEN
        );
        assert!(
            matches.distance <= LZSS::MAX_DISTANCE,
            "distance {}",
            matches.distance
        );
        let len = matches.len - LZSS::MIN_LEN;
        let offset = matches.distance;
        Self(((len as u64) << 16) | ((offset as u64) << 32))
    }

    fn try_interpret<F, E>(&self, mut f: F) -> Result<(), E>
    where
        F: FnMut(LZSS) -> Result<(), E>,
    {
        let len = (self.0 & 0xff) as usize;
        if len > 0 {
            let slice = self.0.to_le_bytes();
            for &byte in slice.iter().skip(1).take(len) {
                f(LZSS::Literal(byte))?;
            }
        } else {
            let len = (self.0 >> 16) as usize + LZSS::MIN_LEN as usize;
            let offset = (self.0 >> 32) as usize;
            f(LZSS::Match(Match::new(len, offset)))?;
        }
        Ok(())
    }
}
