//! Statistics and frequency table functions

use crate::*;
use core::cmp;

pub trait CountFreq<K: Ord> {
    fn count_freq(&mut self, key: K);
}

impl<K: Ord> CountFreq<K> for BTreeMap<K, usize> {
    #[inline]
    fn count_freq(&mut self, key: K) {
        self.entry(key).and_modify(|count| *count += 1).or_insert(1);
    }
}

pub trait IntoFreqTable<K: Ord> {
    fn into_freq_table(self, sort_by_freq: bool) -> Vec<(K, usize)>;
}

impl<K: Ord> IntoFreqTable<K> for BTreeMap<K, usize> {
    #[inline]
    fn into_freq_table(self, sort_by_freq: bool) -> Vec<(K, usize)> {
        let mut vec = self.into_iter().collect::<Vec<_>>();
        if sort_by_freq {
            vec.sort_by(|a, b| match b.1.cmp(&a.1) {
                cmp::Ordering::Equal => a.0.cmp(&b.0),
                ord => ord,
            });
        }
        vec
    }
}
