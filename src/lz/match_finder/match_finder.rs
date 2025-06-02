//! Match Finder
use crate::*;
use core::ops::Range;
use lcp::LcpArray;
use sais::SuffixArray;

mod lcp;
mod sais;

#[cfg(test)]
mod tests;

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

    // pub fn matches<'b>(&'b self, pos: usize) -> Matches<'b> {
    //     let literal = self.s[pos];
    //     let range = self.bucket(literal);
    //     let center = self.rev_sa[pos] as usize;

    //     let matches = Matches {
    //         finder: self,
    //         range,
    //         center,
    //     };

    //     matches
    // }
}

// #[allow(unused)]
// pub struct Matches<'a> {
//     finder: &'a MatchFinder<'a>,
//     range: Range<usize>,
//     center: usize,
// }

// impl<'a> Matches<'a> {
//     //
// }
