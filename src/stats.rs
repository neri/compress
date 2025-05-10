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

pub struct ByteStats {
    freqs: [usize; 256],
    sorted_freqs: [usize; 256],
    sorted_symbols: [u8; 256],
    cumul_freqs: [usize; 257],
    total_count: usize,
    symbol_count: u8,
    max_symbol: u8,
    max_freq: usize,
    min_freq: usize,
}

impl ByteStats {
    #[inline]
    fn empty() -> Self {
        Self {
            freqs: [0; 256],
            sorted_freqs: [0; 256],
            sorted_symbols: [0; 256],
            cumul_freqs: [0; 257],
            symbol_count: 0,
            max_symbol: 0,
            total_count: 0,
            max_freq: 0,
            min_freq: 0,
        }
    }

    pub fn new(input: &[u8]) -> Option<Box<Self>> {
        if input.len() == 0 {
            // needs at least one
            return None;
        }

        let mut result = Box::new(Self::empty());

        for &byte in input {
            result.freqs[byte as usize] += 1;
        }

        result._sort();

        Some(result)
    }

    pub fn with_custom_table(table: &[usize]) -> Box<Self> {
        let mut result = Box::new(Self::empty());

        for (p, q) in result.freqs.iter_mut().zip(table.iter()) {
            *p = *q;
        }

        result._sort();

        result
    }

    fn _sort(&mut self) {
        let mut pairs = Vec::new();
        let mut max_symbol = 0;
        for (index, freq) in self.freqs.iter().enumerate() {
            if *freq > 0 {
                max_symbol = max_symbol.max(index as u8);
                pairs.push((index as u8, *freq));
            }
        }
        pairs.sort_by(|a, b| match b.1.cmp(&a.1) {
            cmp::Ordering::Equal => a.0.cmp(&b.0),
            ord => ord,
        });
        for (index, pair) in pairs.iter().enumerate() {
            self.sorted_freqs[index] = pair.1;
            self.sorted_symbols[index] = pair.0;
        }
        self.max_symbol = max_symbol;
        self.symbol_count = pairs.len() as u8;
        self.total_count = pairs.iter().fold(0, |a, v| a + v.1);
        self.min_freq = pairs.iter().fold(usize::MAX, |a, v| a.min(v.1));
        self.max_freq = pairs.iter().fold(0, |a, v| a.max(v.1));
    }

    #[inline]
    pub fn freq(&self, value: u8) -> usize {
        self.freqs[value as usize]
    }

    #[inline]
    pub fn freqs(&self) -> &[usize; 256] {
        &self.freqs
    }

    #[inline]
    pub fn cumul(&self, value: u8) -> usize {
        self.cumul_freqs[value as usize]
    }

    #[inline]
    pub fn cumuls(&self) -> &[usize; 257] {
        &self.cumul_freqs
    }

    #[inline]
    pub fn total_count(&self) -> usize {
        self.total_count
    }

    #[inline]
    pub fn symbol_count(&self) -> u8 {
        self.symbol_count
    }

    #[inline]
    pub fn max_symbol(&self) -> u8 {
        self.max_symbol
    }

    #[inline]
    pub fn max_freq(&self) -> usize {
        self.max_freq
    }

    #[inline]
    pub fn min_freq(&self) -> usize {
        self.min_freq
    }

    #[inline]
    pub fn sorted<'a>(
        &'a self,
    ) -> impl DoubleEndedIterator + ExactSizeIterator<Item = (u8, usize)> + 'a {
        self.sorted_symbols
            .iter()
            .zip(self.sorted_freqs.iter())
            .map(|(a, b)| (*a, *b))
            .take(self.symbol_count as usize)
    }

    pub fn update_cumul_freqs(&mut self) -> usize {
        self.cumul_freqs[0] = 0;
        for i in 0..256 {
            self.cumul_freqs[i + 1] = self.cumul_freqs[i] + self.freqs[i];
        }
        self.cumul_freqs[256]
    }

    pub fn normalize(&mut self, target_total: usize) -> Option<()> {
        assert!(target_total == target_total.next_power_of_two());

        let cur_total = self.update_cumul_freqs();
        for i in 1..=256 {
            self.cumul_freqs[i] =
                (target_total as u64 * self.cumul_freqs[i] as u64 / cur_total as u64) as usize;
        }

        let mut acc = 0;
        let mut freqs2 = self
            .cumul_freqs
            .iter()
            .skip(1)
            .map(|v| {
                let r = *v - acc;
                acc = *v;
                r
            })
            .collect::<Vec<_>>();

        for i in 0..256 {
            if self.freqs[i] > 0 && freqs2[i] == 0 {
                let mut best_freq = usize::MAX;
                let mut best_steal = None;
                for j in 0..256 {
                    let freq = freqs2[j];
                    if freq > 1 && freq < best_freq {
                        best_freq = freq;
                        best_steal = Some(j);
                    }
                }
                let best_steal = best_steal?;

                freqs2[i] = 1;
                freqs2[best_steal] -= 1;
            }
        }

        for (p, q) in self.freqs.iter_mut().zip(freqs2.iter()) {
            *p = *q;
        }
        self.update_cumul_freqs();

        Some(())
    }
}
