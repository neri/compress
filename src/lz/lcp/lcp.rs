//! Longest Common Prefix array

use crate::*;
use sais::SuffixArray;

mod sais;

#[cfg(test)]
mod tests;

/// Longest Common Prefix array
pub struct LcpArray<'a> {
    s: &'a [u8],
    sa: SuffixArray,
    lcp: Vec<u32>,
    rank: Vec<u32>,
}

impl<'a> LcpArray<'a> {
    /// Make a new LCP array from the given slice.
    pub fn new(s: &'a [u8]) -> Self {
        let sa = SuffixArray::new(s);

        let mut rank = Vec::with_capacity(s.len());
        rank.resize(s.len(), 0);
        for (i, &suffix) in sa.as_slice().iter().enumerate() {
            rank[suffix as usize] = i as u32;
        }

        let lcp = Self::_kasai(s, sa.as_slice(), &rank);

        Self { s, sa, lcp, rank }
    }

    /// Returns the longest common prefix array.
    #[inline]
    pub fn lcp(&self) -> &[u32] {
        &self.lcp
    }

    /// Returns the rank array.
    #[inline]
    pub fn rank(&self) -> &[u32] {
        &self.rank
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

    /// Kasai's algorithm to compute the LCP array.
    fn _kasai(s: &[u8], sa: &[u32], rank: &[u32]) -> Vec<u32> {
        let n = s.len();
        let mut k = 0usize;
        let mut lcp = Vec::with_capacity(n);
        lcp.resize(n, 0u32);

        for (i, &rank) in rank.iter().enumerate() {
            if rank == n as u32 - 1 {
                k = 0;
                lcp[rank as usize] = k as u32;
                continue;
            }
            let j = sa[rank as usize + 1] as usize;
            while i + k < n && j + k < n && s[i + k] == s[j + k] {
                k += 1;
            }
            lcp[rank as usize] = k as u32;
            if k > 0 {
                k -= 1;
            }
        }
        lcp
    }

    /// naive implementation for testing purposes
    pub fn naive(s: &'a [u8]) -> Self {
        let sa = SuffixArray::naive(s);

        let mut rank = Vec::with_capacity(s.len());
        rank.resize(s.len(), 0);
        for (i, &s) in sa.as_slice().iter().enumerate() {
            rank[s as usize] = i as u32;
        }

        let mut lcp = Vec::with_capacity(s.len());
        for window in sa.as_slice().windows(2) {
            let (lhs, rhs) = (window[0] as usize, window[1] as usize);
            let mut lcp_length = 0;
            let lhs = &s[lhs..];
            let rhs = &s[rhs..];
            for (l, r) in lhs.iter().zip(rhs.iter()) {
                if l == r {
                    lcp_length += 1;
                } else {
                    break;
                }
            }
            lcp.push(lcp_length as u32);
        }
        lcp.push(0); // last suffix has no next suffix to compare

        Self { s, sa, lcp, rank }
    }
}
