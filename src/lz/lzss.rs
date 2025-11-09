//! Lempel–Ziv–Storer–Szymanski style compression code
//!
//! <https://en.wikipedia.org/wiki/Lempel%E2%80%93Ziv%E2%80%93Storer%E2%80%93Szymanski>
//!

use super::match_finder::MatchFinder;
use crate::EncodeError;
use crate::lz::{cache::*, *};
use crate::*;

#[derive(Debug)]
pub struct Configuration {
    pub max_distance: usize,
    pub max_len: NonZero<usize>,
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
            max_len: NonZero::new(if max_len > LZSS::MAX_LEN {
                LZSS::MAX_LEN
            } else {
                max_len
            })
            .unwrap(),
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

impl LZSS {
    /// Minimum match length in LZSS
    pub const MIN_LEN: usize = 3;

    pub const MAX_LEN: usize = Self::MIN_LEN + 4096;

    pub const MAX_DISTANCE: usize = 0x10_0000;

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
                let mut matches = MaybeMatch::default();

                if let Some(mut iter) = offset3_cache.matches() {
                    if let Some(distance) = iter.next() {
                        let len = lz::matching_len(input, current + guaranteed_min_len, distance);
                        matches =
                            Match::new(NonZero::new(len + guaranteed_min_len).unwrap(), distance)
                                .into();
                    }
                }

                if let Some(matches) = matches.get() {
                    let mut total_len = 0;
                    let mut left = matches.len.get();
                    loop {
                        if left > max_len.get() {
                            f(LZSS::Match(Match::new(max_len, matches.distance)))?;
                            left -= max_len.get();
                            total_len += max_len.get();
                        } else if left >= LZSS::MIN_LEN {
                            f(LZSS::Match(Match::new(
                                NonZero::new(left).unwrap(),
                                matches.distance,
                            )))?;
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
                let mut matches = MaybeMatch::default();

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
                            matches = v.into();
                        }
                        None => {}
                    }
                }

                if let Some(matches) = matches.get() {
                    let mut total_len = 0;
                    let mut left = matches.len.get();
                    loop {
                        if left > max_len.get() {
                            f(LZSS::Match(Match::new(max_len, matches.distance)))?;
                            left -= max_len.get();
                            total_len += max_len.get();
                        } else if left >= LZSS::MIN_LEN {
                            f(LZSS::Match(Match::new(
                                NonZero::new(left).unwrap(),
                                matches.distance,
                            )))?;
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

    /// Encode LZSS with Suffix Array and Longest Common Prefix array compression (experimental)
    pub fn encode_sa_lcp<F>(
        input: &[u8],
        config: Configuration,
        mut f: F,
    ) -> Result<(), EncodeError>
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
        let low_base = config.max_distance.min(window_size / 2);
        let mut low = 0;
        let mut high = input.len().min(window_size);
        let mut threshold = if window_size >= input.len() {
            input.len()
        } else {
            window_size - config.max_len.get()
        };
        loop {
            let input2 = &input[low..high];
            let finder: MatchFinder<'_> = MatchFinder::new(input2);
            while let Some(&literal) = input2.get(current) {
                let count = {
                    let matches = finder.matches(current, LZSS::MIN_LEN, config.max_distance);

                    if let Some(matches) = matches {
                        let mut total_len = 0;
                        let mut left = matches.len.get();
                        loop {
                            if left > config.max_len.get() {
                                f(LZSS::Match(Match::new(config.max_len, matches.distance)))?;
                                left -= config.max_len.get();
                                total_len += config.max_len.get();
                                if current + total_len >= threshold {
                                    break;
                                }
                            } else if left >= LZSS::MIN_LEN {
                                f(LZSS::Match(Match::new(
                                    NonZero::new(left).unwrap(),
                                    matches.distance,
                                )))?;
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
            low += current - low_base;
            high = (low + window_size).min(input.len());
            current = low_base;
            threshold = window_size;
        }

        Ok(())
    }
}
