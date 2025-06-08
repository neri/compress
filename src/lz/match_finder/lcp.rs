//! Longest Common Prefix array

#[allow(unused_imports)]
use super::sais::SuffixArray;
use crate::*;

/// Longest Common Prefix array
pub struct LcpArray;

impl LcpArray {
    /// Creates a new LCP array using the Kasai's algorithm.
    pub fn new(s: &[u8], sa: &[u32], rev_sa: &[u32]) -> Vec<u32> {
        let n = s.len();
        let mut k = 0usize;
        let mut lcp = Vec::with_capacity(n);
        lcp.resize(n, 0u32);

        for (i, &rank) in rev_sa.iter().enumerate() {
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
}

/// naive implementation for testing purposes
#[cfg(test)]
pub struct LcpArrayNaive {
    pub sa: SuffixArray,
    pub lcp: Vec<u32>,
}

#[cfg(test)]
impl LcpArrayNaive {
    pub fn new(s: &[u8]) -> Self {
        let sa = SuffixArray::naive(s);

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

        Self { sa, lcp }
    }

    #[inline]
    pub fn sa(&self) -> &[u32] {
        self.sa.as_slice()
    }

    #[inline]
    pub fn lcp(&self) -> &[u32] {
        &self.lcp
    }
}
