//! Lempel–Ziv–Storer–Szymanski style compression code
//!
//! <https://en.wikipedia.org/wiki/Lempel%E2%80%93Ziv%E2%80%93Storer%E2%80%93Szymanski>

use crate::{
    EncodeError,
    lz::{cache::*, *},
    slice_window::SliceWindow,
    *,
};
use core::convert::Infallible;

#[derive(Debug)]
pub struct Configuration {
    max_distance: usize,
    max_len: usize,
    cache_purge_limit: usize,
}

impl Configuration {
    /// Maximum window size in deflate (32K, 258)
    pub const DEFLATE: Self = Self::new(32768, 258, 0);

    /// Fast and Tiny Dictionary size
    pub const FAST: Self = Self::new(16 * 1024, LZSS::MAX_LEN, 0);

    /// Default Dictionary size
    pub const DEFAULT: Self = Self::new(LZSS::MAX_DISTANCE, LZSS::MAX_LEN, 0);

    pub const MAX: Self = Self::new(LZSS::MAX_DISTANCE, LZSS::MAX_LEN, 0);

    // 16M = 128MB
    pub const CACHE_PURGE_LIMIT: usize = 16 * 1024 * 1024;

    #[inline]
    pub const fn new(max_distance: usize, max_len: usize, cache_purge_limit: usize) -> Self {
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

    const ATTEMPT_MATCHES: usize = 16;

    const THRESHOLD_LEN_SHORT: usize = 8;

    const THRESHOLD_LEN_2D: usize = 8;

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

        let mut offset3_cache = OffsetCache3::new(input, max_distance, config.cache_purge_limit());

        let mut buf = Vec::new();
        let lit_buf = SliceWindow::new(input, 0);
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
                                Self::THRESHOLD_LEN_SHORT,
                                offset3_cache.guaranteed_min_len(),
                                iter.take(Self::ATTEMPT_MATCHES),
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
                                Self::THRESHOLD_LEN_SHORT,
                                offset3_cache.guaranteed_min_len(),
                                iter.take(Self::ATTEMPT_MATCHES),
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
