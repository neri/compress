//! Lempel–Ziv–Storer–Szymanski style compression code
//!
//! <https://en.wikipedia.org/wiki/Lempel%E2%80%93Ziv%E2%80%93Storer%E2%80%93Szymanski>

use super::lcp::LcpArray;
use crate::{
    EncodeError,
    lz::{cache::*, *},
    *,
};
use core::convert::Infallible;

#[derive(Debug)]
pub struct Configuration {
    max_distance: usize,
    max_len: usize,
    skip_first_literal: usize,
    search_attempts: usize,
    threshold_len: usize,
    cache_purge_limit: usize,
}

impl Configuration {
    /// Default Dictionary size
    pub const DEFAULT: Self = Self::new(LZSS::MAX_DISTANCE, LZSS::MAX_LEN, 0, 0, 0, 0);

    // default attempts
    pub const SEARCH_ATTEMPTS: usize = 16;

    pub const THRESHOLD_LEN: usize = 16;

    // 16M = 128MB
    pub const CACHE_PURGE_LIMIT: usize = 16 * 1024 * 1024;

    #[inline]
    pub const fn new(
        max_distance: usize,
        max_len: usize,
        skip_first_literal: usize,
        search_attempts: usize,
        threshold_len: usize,
        cache_purge_limit: usize,
    ) -> Self {
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
            skip_first_literal,
            search_attempts: if search_attempts > 0 {
                search_attempts
            } else {
                Self::SEARCH_ATTEMPTS
            },
            threshold_len: if threshold_len > 0 {
                threshold_len
            } else {
                Self::THRESHOLD_LEN
            },
            cache_purge_limit: if cache_purge_limit > 0 {
                cache_purge_limit
            } else {
                Self::CACHE_PURGE_LIMIT
            },
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

    #[inline]
    pub fn search_attempts(&self) -> usize {
        self.search_attempts
    }

    #[inline]
    pub fn threshold_len(&self) -> usize {
        self.threshold_len
    }

    #[inline]
    pub fn cache_purge_limit(&self) -> usize {
        self.cache_purge_limit
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
    Match(Matches),
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
            OffsetCache3::new(input, config.max_distance(), config.cache_purge_limit());

        let mut cursor = 1 + config.skip_first_literal;
        for &literal in input.iter().take(cursor) {
            f(LZSS::Literal(literal))?;
        }
        offset3_cache.advance(cursor);

        let guaranteed_min_len = offset3_cache.guaranteed_min_len();
        assert_eq!(guaranteed_min_len, 3);

        while let Some(&literal) = input.get(cursor) {
            let count = {
                let mut matches = Matches::ZERO;

                if let Some(mut iter) = offset3_cache.matches() {
                    if let Some(distance) = iter.next() {
                        let len = lz::matching_len(
                            input,
                            cursor + guaranteed_min_len,
                            distance,
                            usize::MAX,
                        );
                        matches = Matches::new(len + guaranteed_min_len, distance);
                    }
                }

                if matches.len >= LZSS::MIN_LEN as usize {
                    if matches.len < config.max_len() {
                        f(LZSS::Match(matches))?;
                        matches.len
                    } else {
                        let mut total_len = 0;
                        let mut left = matches.len;
                        loop {
                            if left > config.max_len() {
                                f(LZSS::Match(Matches::new(
                                    config.max_len(),
                                    matches.distance,
                                )))?;
                                left -= config.max_len();
                                total_len += config.max_len();
                            } else if left >= LZSS::MIN_LEN {
                                f(LZSS::Match(Matches::new(left, matches.distance)))?;
                                total_len += left;
                                break;
                            } else {
                                break;
                            }
                        }
                        total_len
                    }
                } else {
                    f(LZSS::Literal(literal))?;
                    1
                }
            };
            offset3_cache.advance(count);
            cursor += count;
        }

        Ok(())
    }

    pub fn encode<F>(
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
        let max_distance = config.max_distance() - distance_base;
        let search_attempts = config.search_attempts();
        let threshold_len = config.threshold_len();

        let mut offset3_cache = OffsetCache3::new(input, max_distance, config.cache_purge_limit());

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
                    let mut matches = Matches::ZERO;

                    // Find 2d distance matches
                    for (dist_code, distance) in distance_mapping.iter().enumerate() {
                        if *distance > cursor {
                            continue;
                        }
                        let len = lz::matching_len(input, cursor, *distance, config.max_len());
                        if matches.len < len {
                            matches = Matches::new(len, dist_code + 1);
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
                                config.max_len(),
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
                    let mut matches = Matches::ZERO;

                    if matches.is_zero() {
                        if let Some(iter) = offset3_cache.matches() {
                            match lz::find_distance_matches(
                                input,
                                cursor,
                                config.max_len(),
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

    /// Encode LZSS with Longest Common Prefix (LCP) compression
    pub fn encode_lcp<F>(input: &[u8], config: Configuration, mut f: F) -> Result<(), EncodeError>
    where
        F: FnMut(LZSS) -> Result<(), EncodeError>,
    {
        if config.search_attempts() == 1 {
            return Self::encode_fast(input, config, f);
        } else {
            if input.is_empty() || input.len() > i32::MAX as usize {
                return Err(EncodeError::InvalidInput);
            }

            let lcpa = LcpArray::new(input);
            let mut cursor = 1 + config.skip_first_literal;
            for &literal in input.iter().take(cursor) {
                f(LZSS::Literal(literal))?;
            }
            while let Some(&literal) = input.get(cursor) {
                let count = {
                    let mut matches = Matches::ZERO;
                    let min_offset = cursor.saturating_sub(config.max_distance());
                    let sa_base_index = lcpa.rank()[cursor] as usize;
                    if let Some(_) = lcpa.lcp().get(sa_base_index) {
                        let mut lcp_limit = usize::MAX;
                        for (&lcp, &offset) in lcpa
                            .lcp()
                            .iter()
                            .zip(lcpa.sa().iter().skip(1))
                            .skip(sa_base_index)
                        {
                            let lcp = lcp as usize;
                            let offset = offset as usize;
                            if lcp < LZSS::MIN_LEN {
                                break;
                            }
                            if offset >= min_offset && offset < cursor {
                                let len = lcp_limit.min(lcp);
                                let distance = cursor - offset;
                                if matches.is_zero() {
                                    matches = Matches::new(len, distance);
                                } else if matches.len > len {
                                    break;
                                } else if matches.len < len {
                                    matches = Matches::new(len, distance);
                                } else if matches.len == len && matches.distance > distance {
                                    matches.distance = distance;
                                }
                            }
                            lcp_limit = lcp_limit.min(lcp);
                        }
                    }
                    if sa_base_index > 0 {
                        let mut matches2 = Matches::ZERO;
                        let lcp = &lcpa.lcp()[..sa_base_index];
                        let sa = &lcpa.sa()[..sa_base_index];
                        let mut lcp_limit = usize::MAX;
                        for (&lcp, &offset) in lcp.iter().zip(sa.iter()).rev() {
                            let lcp = lcp as usize;
                            let offset = offset as usize;
                            if lcp < LZSS::MIN_LEN {
                                break;
                            }
                            if offset >= min_offset && offset < cursor {
                                let len = lcp_limit.min(lcp);
                                let distance = cursor - offset;
                                if matches2.is_zero() {
                                    matches2 = Matches::new(len, distance);
                                } else if matches2.len > len {
                                    break;
                                } else if matches2.len < len {
                                    matches2 = Matches::new(len, distance);
                                } else if matches2.len == len && matches2.distance > distance {
                                    matches2.distance = distance;
                                }
                            }
                            lcp_limit = lcp_limit.min(lcp);
                        }

                        if matches2.len > matches.len
                            || matches2.len == matches.len && matches2.distance < matches.distance
                        {
                            matches = matches2;
                        }
                    }

                    if matches.len >= LZSS::MIN_LEN as usize {
                        if matches.len < config.max_len() {
                            f(LZSS::Match(matches))?;
                            matches.len
                        } else {
                            let mut total_len = 0;
                            let mut left = matches.len;
                            loop {
                                if left > config.max_len() {
                                    f(LZSS::Match(Matches::new(
                                        config.max_len(),
                                        matches.distance,
                                    )))?;
                                    left -= config.max_len();
                                    total_len += config.max_len();
                                } else if left >= LZSS::MIN_LEN {
                                    f(LZSS::Match(Matches::new(left, matches.distance)))?;
                                    total_len += left;
                                    break;
                                } else {
                                    break;
                                }
                            }
                            total_len
                        }
                    } else {
                        f(LZSS::Literal(literal))?;
                        1
                    }
                };
                cursor += count;
            }
        }
        Ok(())
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
    fn with_match(matches: Matches) -> Self {
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
            f(LZSS::Match(Matches::new(len, offset)))?;
        }
        Ok(())
    }
}
