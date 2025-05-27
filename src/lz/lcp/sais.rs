//! Suffix Array

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::mem::transmute;
use core::ops::Deref;

/// Suffix Array
pub struct SuffixArray {
    inner: Vec<u32>,
    offset: usize,
}

impl SuffixArray {
    /// Creates a Suffix Array using the SA-IS algorithm.
    ///
    /// # Panics
    ///
    /// Panics if the input length is greater than `i32::MAX`.
    pub fn new(source: &[u8]) -> Self {
        assert!(source.len() < i32::MAX as usize);
        let n = source.len() + 1;

        let mut s = Vec::with_capacity(n);
        let mut alphabet_max = 0;
        for &byte in source {
            let byte = byte as i32;
            if byte > alphabet_max {
                alphabet_max = byte;
            }
            s.push(byte);
        }

        let mut sa = Vec::with_capacity(n);
        sa.resize(n, -1);
        Self::sa_is(&s, &mut sa, alphabet_max);

        return Self {
            inner: unsafe { transmute(sa) },
            offset: 1,
        };
    }

    fn sa_is(s: &[i32], sa: &mut [i32], alphabet_max: i32) {
        let alphabet_size = (alphabet_max + 2) as usize;

        // classify as L and S
        let mut lors_vec = Vec::with_capacity(s.len() + 1);
        let mut prev_data = -1;
        let mut prev_lors = LorS::S;
        lors_vec.push(prev_lors);
        for &data in s.iter().rev() {
            let lors = if data < prev_data {
                LorS::S
            } else if data > prev_data {
                LorS::L
            } else {
                prev_lors
            };
            lors_vec.push(lors);
            prev_lors = lors;
            prev_data = data;
        }
        lors_vec.reverse();

        // classify LMS substrings
        let mut lms_indexes = Vec::new();
        for (index, pair) in windowed2(&lors_vec).enumerate() {
            if *pair.0 == LorS::L && *pair.1 == LorS::S {
                lms_indexes.push(1 + index as i32);
            }
        }
        for &index in &lms_indexes {
            lors_vec[index as usize] = LorS::LMS;
        }
        lms_indexes.push(s.len() as i32); // sentinel

        let mut lms_substrs = BTreeMap::new();

        let mut counts = Vec::new();
        counts.resize(alphabet_size, 0i32);
        counts[0] = 1; // sentinel
        for &byte in s.iter() {
            counts[1 + byte as usize] += 1;
        }

        // phase-1

        // sort LMS
        let mut buckets = Self::make_buckets(&counts);
        for pair in windowed2(&lms_indexes).map(|(a, b)| (*b, *a)).rev() {
            match s.get(pair.1 as usize) {
                Some(&byte) => {
                    let bucket = &mut buckets[1 + byte as usize];
                    let bi = *bucket as usize - 1;
                    sa[bi] = pair.1 as i32;
                    *bucket -= 1;

                    let mut end = pair.0 as usize;
                    if end >= s.len() {
                        end = s.len() - 1;
                    }
                    lms_substrs.insert(pair.1 as u32, &s[(pair.1 as usize)..=end]);
                }
                None => {
                    sa[0] = pair.1 as i32;
                }
            }
        }

        Self::sort_type_l(&counts, s, sa, &lors_vec);

        Self::sort_type_s(&counts, s, sa, &lors_vec);

        // phase-2
        let mut lmssa = Vec::with_capacity(lms_indexes.len());
        for &suffix in sa.iter() {
            if suffix != -1 && lors_vec[suffix as usize].is_lms() {
                lmssa.push(suffix);
            }
        }

        let mut name = 0;
        let mut names = Vec::new();
        names.push(0);
        for (lhs, rhs) in windowed2(&lmssa) {
            let lhs = lms_substrs.get(&(*lhs as u32));
            let rhs = lms_substrs.get(&(*rhs as u32));
            if lhs != rhs {
                name += 1;
            }
            names.push(name);
        }

        if (name as usize) < names.len() - 1 {
            let mut lms_pairs = lmssa
                .iter()
                .zip(names.iter())
                .map(|(&index, &name)| (index as u32, name))
                .collect::<Vec<_>>();
            lms_pairs.sort_by(|a, b| a.0.cmp(&b.0));

            let s = lms_pairs.iter().map(|&(_, v)| v).collect::<Vec<_>>();
            let alphbet_max = *s.iter().max().unwrap_or(&0);

            let sa = &mut sa[..s.len() + 1];
            sa.fill(-1);
            Self::sa_is(&s, sa, alphbet_max);

            lmssa.clear();
            for &suffix in sa.iter().skip(1) {
                lmssa.push(lms_indexes[suffix as usize]);
            }
        }

        // phase-3
        sa.fill(-1);

        // insert LMS
        let mut buckets = Self::make_buckets(&counts);
        for &lms in lmssa.iter().rev() {
            match s.get(lms as usize) {
                Some(&byte) => {
                    let bucket = &mut buckets[1 + byte as usize];
                    let bi = *bucket as usize - 1;
                    sa[bi] = lms;
                    *bucket -= 1;
                }
                None => {
                    sa[0] = lms;
                }
            }
        }

        Self::sort_type_l(&counts, s, sa, &lors_vec);

        Self::sort_type_s(&counts, s, sa, &lors_vec);
    }

    fn make_buckets(counts: &[i32]) -> Vec<i32> {
        let mut buckets = Vec::new();
        buckets.resize(counts.len(), 0i32);
        let mut acc = 0;
        for (&count, bucket) in counts.iter().zip(buckets.iter_mut()) {
            acc += count;
            *bucket = acc;
        }
        buckets
    }

    /// sort L-type
    fn sort_type_l(counts: &[i32], s: &[i32], sa: &mut [i32], lors_vec: &[LorS]) {
        let mut buckets = Self::make_buckets(counts);

        for i in 0..sa.len() {
            let sa_i = sa[i];
            let Some(index) = (sa_i as usize).checked_sub(1) else {
                continue;
            };
            if sa_i != -1 && lors_vec[index].is_l() {
                let byte = s[index];
                let bucket = &mut buckets[byte as usize];
                let bi = *bucket as usize;
                sa[bi] = index as i32;
                *bucket += 1;
            }
        }
    }

    /// sort S-type
    fn sort_type_s(counts: &[i32], s: &[i32], sa: &mut [i32], lors_vec: &[LorS]) {
        let mut buckets = Self::make_buckets(counts);

        for i in (0..sa.len()).rev() {
            let sa_i = sa[i];
            let Some(index) = (sa_i as usize).checked_sub(1) else {
                continue;
            };
            if sa[i] != -1 && lors_vec[index].is_s() {
                let byte = s[index];
                let bucket = &mut buckets[1 + byte as usize];
                let bi = *bucket as usize - 1;
                sa[bi] = index as i32;
                *bucket -= 1;
            }
        }
    }

    /// naive implementation for testing purposes
    pub fn naive(input: &[u8]) -> Self {
        let mut sa = (0..input.len() as u32).collect::<Vec<_>>();
        sa.sort_by(|&a, &b| input[(a as usize)..].cmp(&input[(b as usize)..]));
        Self {
            inner: sa,
            offset: 0,
        }
    }

    /// Returns the suffix array.
    #[inline]
    pub fn as_slice(&self) -> &[u32] {
        &self.inner[self.offset..]
    }
}

impl AsRef<[u32]> for SuffixArray {
    #[inline]
    fn as_ref(&self) -> &[u32] {
        self.as_slice()
    }
}

impl Deref for SuffixArray {
    type Target = [u32];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LorS {
    L,
    S,
    LMS,
}

impl LorS {
    #[inline]
    pub const fn is_l(&self) -> bool {
        matches!(self, Self::L)
    }

    #[inline]
    pub const fn is_s(&self) -> bool {
        !matches!(self, Self::L)
    }

    #[inline]
    pub const fn is_lms(&self) -> bool {
        matches!(self, Self::LMS)
    }
}

#[inline(always)]
fn windowed2<T>(
    slice: &[T],
) -> impl Iterator<Item = (&T, &T)> + ExactSizeIterator + DoubleEndedIterator {
    slice.windows(2).map(|a| (&a[0], &a[1]))
}
