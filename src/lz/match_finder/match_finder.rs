//! Match Finder using Suffix Array and LCP Array
use crate::lz::{Match, MaybeMatch};
use crate::*;
use core::{num::NonZero, ops::Range};
use lcp::LcpArray;
use sais::SuffixArray;

mod lcp;
mod sais;

#[cfg(test)]
mod tests;

/// Match Finder using Suffix Array and LCP Array
pub struct MatchFinder<'a> {
    s: &'a [u8],
    sa: SuffixArray,
    lcp: Vec<u32>,
    rev_sa: Vec<u32>,
    counts: Box<[u32; 256]>,
    buckets: Box<[u32; 257]>,
}

impl<'a> MatchFinder<'a> {
    pub fn new(s: &'a [u8]) -> Self {
        let mut counts = [0; 256];
        for &byte in s {
            counts[byte as usize] += 1;
        }

        let mut buckets = [0; 257];
        let mut acc = 0;
        for (&count, bucket) in counts.iter().zip(buckets.iter_mut()) {
            acc += count;
            *bucket = acc;
        }
        buckets[256] = s.len() as u32;

        let sa = SuffixArray::new(s);

        let mut rev_sa = Vec::with_capacity(s.len());
        rev_sa.resize(s.len(), 0);
        for (i, &suffix) in sa.as_slice().iter().enumerate() {
            rev_sa[suffix as usize] = i as u32;
        }

        let lcp = LcpArray::new(s, sa.as_slice(), &rev_sa);

        Self {
            s,
            sa,
            lcp,
            rev_sa,
            counts: counts.into(),
            buckets: buckets.into(),
        }
    }

    /// Returns the longest common prefix array.
    #[inline]
    pub fn lcp(&self) -> &[u32] {
        &self.lcp
    }

    /// Returns the reverse lookup suffix array
    #[inline]
    pub fn rev_sa(&self) -> &[u32] {
        &self.rev_sa
    }

    /// Returns the suffix array.
    #[inline]
    pub fn sa(&self) -> &[u32] {
        self.sa.as_slice()
    }

    /// Returns the original string.
    #[inline]
    pub fn s(&self) -> &[u8] {
        self.s
    }

    #[inline]
    pub fn counts(&self) -> &[u32; 256] {
        &self.counts
    }

    #[inline]
    pub fn buckets(&self) -> &[u32; 257] {
        &self.buckets
    }

    #[inline]
    pub fn bucket(&self, byte: u8) -> Range<usize> {
        self.buckets[byte as usize] as usize..self.buckets[1 + byte as usize] as usize
    }

    pub fn matches<'b>(&'b self, pos: usize, min_len: usize, max_distance: usize) -> Option<Match> {
        let min_offset = pos.saturating_sub(max_distance);
        let sa_base_index = self.rev_sa[pos] as usize;
        let takes = 200;

        let iter1 = (self.lcp().get(sa_base_index)).map(|_| {
            self.lcp()
                .iter()
                .zip(self.sa().iter().skip(1))
                .skip(sa_base_index)
                .take(takes)
        });
        let iter2 = (sa_base_index > 0).then(|| {
            let lcp2 = &self.lcp()[..sa_base_index];
            let sa2 = &self.sa()[..sa_base_index];
            lcp2.iter().zip(sa2.iter()).rev().take(takes)
        });

        let mut matches = MaybeMatch::default();

        let mut kernel = |lcp: &u32, offset: &u32, min_len: usize, lcp_limit: &mut usize| -> bool {
            let lcp = *lcp as usize;
            let offset = *offset as usize;
            if lcp < min_len {
                return false;
            }
            if offset >= min_offset && offset < pos {
                let len = (*lcp_limit).min(lcp);
                let distance = pos - offset;

                if let Some(ref mut matches2) = matches.get() {
                    if matches.len() > len {
                        return false;
                    } else if matches.len() < len {
                        matches = MaybeMatch::new(len, distance);
                    } else if matches.len() == len && matches.distance() > distance {
                        matches2.distance = NonZero::new(distance).unwrap();
                    }
                } else {
                    matches = MaybeMatch::new(len, distance);
                }

                // if matches.is_zero() {
                //     matches = Match::new(len, distance).into();
                // } else if matches.len() > len {
                //     return false;
                // } else if matches.len() < len {
                //     matches = Match::new(len, distance).into();
                // } else if matches.len() == len && matches.distance() > distance {
                //     matches.distance = distance;
                // }
            }
            *lcp_limit = (*lcp_limit).min(lcp);
            true
        };

        let mut lcp_limit1 = usize::MAX;
        let mut lcp_limit2 = usize::MAX;

        match (iter1, iter2) {
            (None, None) => {
                // No iterators available
                return None;
            }
            (Some(mut iter1), None) => {
                while let Some((lcp, offset)) = iter1.next() {
                    if !kernel(lcp, offset, min_len, &mut lcp_limit1) {
                        break;
                    }
                }
                return matches.get();
            }
            (None, Some(mut iter2)) => {
                while let Some((lcp, offset)) = iter2.next() {
                    if !kernel(lcp, offset, min_len, &mut lcp_limit2) {
                        break;
                    }
                }
                return matches.get();
            }
            (Some(mut iter1), Some(mut iter2)) => {
                let (iter1, iter2) = loop {
                    if let Some((lcp, offset)) = iter1.next() {
                        if !kernel(lcp, offset, min_len, &mut lcp_limit1) {
                            break (None, Some(iter2));
                        }
                    } else {
                        break (None, Some(iter2));
                    }
                    if let Some((lcp, offset)) = iter2.next() {
                        if !kernel(lcp, offset, min_len, &mut lcp_limit2) {
                            break (Some(iter1), None);
                        }
                    } else {
                        break (Some(iter1), None);
                    }
                };
                match (iter1, iter2) {
                    (Some(mut iter1), None) => {
                        while let Some((lcp, offset)) = iter1.next() {
                            if !kernel(lcp, offset, min_len, &mut lcp_limit1) {
                                break;
                            }
                        }
                        return matches.get();
                    }
                    (None, Some(mut iter2)) => {
                        while let Some((lcp, offset)) = iter2.next() {
                            if !kernel(lcp, offset, min_len, &mut lcp_limit2) {
                                break;
                            }
                        }
                        return matches.get();
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}
