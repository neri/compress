//! cache offsets of matching patterns

use crate::*;
use alloc::vec;
use core::num::NonZero;

macro_rules! def_key {
    ($magic_number:expr, $trait_name:ident, $class_name:ident, $storage_class:ident, $mask:expr) => {
        pub trait $trait_name
        where
            Self::ElementType: Copy,
            Self::KeyType: Copy + Ord,
        {
            type ElementType;
            type KeyType;

            fn null() -> Self;

            fn new(values: [Self::ElementType; $magic_number]) -> Self;

            fn key_value(&self) -> Self::KeyType;

            fn guaranteed_min_len() -> usize;

            fn advance(&mut self, new_value: Self::ElementType);
        }

        #[repr(transparent)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $class_name($storage_class);

        impl $trait_name for $class_name {
            type ElementType = u8;
            type KeyType = $storage_class;

            #[inline]
            fn null() -> Self {
                Self(Default::default())
            }

            #[inline]
            fn new(values: [Self::ElementType; $magic_number]) -> Self {
                Self(
                    values
                        .iter()
                        .rev()
                        .fold(0, |a, &b| (a << 8) | (b as $storage_class)),
                )
            }

            #[inline]
            fn key_value(&self) -> Self::KeyType {
                self.0 & $mask
            }

            #[inline]
            fn guaranteed_min_len() -> usize {
                $magic_number
            }

            #[inline]
            fn advance(&mut self, new_value: Self::ElementType) {
                self.0 =
                    (self.0 >> 8) | ((new_value as $storage_class) << (8 * ($magic_number - 1)));
            }
        }
    };
}

macro_rules! def_cache {
    ($magic_number:expr, $class_name:ident, $key_name:ident, $key_class:ident, $bind_class:ident) => {
        pub struct $class_name<'a, KEY>
        where
            KEY: $key_name,
        {
            source: &'a [KEY::ElementType],
            key: KEY,
            cache: BTreeMap<KEY::KeyType, OffsetList>,
            cursor: usize,
            limit: usize,
            max_distance: usize,
            purge_count: usize,
            purge_limit: usize,
        }

        impl<'a, KEY: $key_name> $class_name<'a, KEY> {
            #[inline]
            pub fn new(
                source: &'a [KEY::ElementType],
                max_distance: usize,
                purge_limit: usize,
            ) -> Self {
                if source.len() < ($magic_number + 1) {
                    Self {
                        source,
                        key: KEY::null(),
                        cache: BTreeMap::new(),
                        cursor: 0,
                        limit: 0,
                        max_distance,
                        purge_count: 0,
                        purge_limit: 0,
                    }
                } else {
                    Self {
                        source,
                        key: KEY::new(source[..$magic_number].try_into().unwrap()),
                        cache: BTreeMap::new(),
                        cursor: 0,
                        limit: source.len() - ($magic_number - 1),
                        max_distance,
                        purge_count: 0,
                        purge_limit: if purge_limit > 0 {
                            purge_limit
                        } else {
                            max_distance * 2
                        },
                    }
                }
            }
        }

        impl<KEY: $key_name> OffsetCache for $class_name<'_, KEY> {
            fn advance(&mut self, step: usize) {
                let limit = self.limit;
                let mut cursor = self.cursor;
                if cursor >= limit {
                    return;
                }
                for _ in 0..step {
                    let value = cursor as u32;
                    self.cache
                        .entry(self.key.key_value())
                        .and_modify(|list| list.push(value))
                        .or_insert_with(|| OffsetList::new(value));

                    cursor += 1;
                    if cursor >= limit {
                        break;
                    }
                    self.key.advance(self.source[cursor + ($magic_number - 1)]);
                }

                self.purge_count += step;
                if self.purge_count >= self.purge_limit {
                    let min_value = self.cursor.saturating_sub(self.max_distance) as u32;
                    self.cache.retain(|_k, v| v.retain(min_value));
                    self.purge_count = cursor % self.max_distance;
                }

                self.cursor = cursor;
            }

            fn matches<'a>(&'a self) -> Option<impl Iterator<Item = NonZero<usize>> + 'a> {
                if self.cursor >= self.limit {
                    return None;
                }
                let min_value = self.cursor.saturating_sub(self.max_distance);
                self.cache
                    .get(&self.key.key_value())
                    .map(|v| v.distances(self.cursor, min_value))
            }

            fn nearest(&self) -> Option<usize> {
                if self.cursor >= self.limit {
                    return None;
                }
                let min_value = self.cursor.saturating_sub(self.max_distance);
                self.cache.get(&self.key.key_value()).and_then(|v| {
                    let nearest = v.nearest().unwrap() as usize;
                    (nearest >= min_value).then(|| self.cursor - nearest)
                })
            }

            fn guaranteed_min_len(&self) -> usize {
                KEY::guaranteed_min_len()
            }
        }

        pub type $bind_class<'a> = $class_name<'a, $key_class>;
    };
}

def_key!(3, MatchingKey3, Matching3BKey, u32, 0x00ff_ffff);
def_key!(4, MatchingKey4, Matching4BKey, u32, 0xffff_ffff);
def_key!(5, MatchingKey5, Matching5BKey, u64, 0x0000_00ff_ffff_ffff);
def_key!(6, MatchingKey6, Matching6BKey, u64, 0x0000_ffff_ffff_ffff);
def_key!(7, MatchingKey7, Matching7BKey, u64, 0x00ff_ffff_ffff_ffff);
def_key!(8, MatchingKey8, Matching8BKey, u64, 0xffff_ffff_ffff_ffff);

def_cache!(3, Matching3Cache, MatchingKey3, Matching3BKey, OffsetCache3);
def_cache!(4, Matching4Cache, MatchingKey4, Matching4BKey, OffsetCache4);
def_cache!(5, Matching5Cache, MatchingKey5, Matching5BKey, OffsetCache5);
def_cache!(6, Matching6Cache, MatchingKey6, Matching6BKey, OffsetCache6);
def_cache!(7, Matching7Cache, MatchingKey7, Matching7BKey, OffsetCache7);
def_cache!(8, Matching8Cache, MatchingKey8, Matching8BKey, OffsetCache8);

pub trait OffsetCache {
    fn advance(&mut self, step: usize);

    fn matches<'a>(&'a self) -> Option<impl Iterator<Item = NonZero<usize>> + 'a>;

    fn nearest(&self) -> Option<usize>;

    // Guaranteed minimum match length
    fn guaranteed_min_len(&self) -> usize;
}

pub type Offset3WordsCache<'a> = Matching3Cache<'a, Matching3WKey>;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Matching3WKey(LruVec3<u32>);

impl MatchingKey3 for Matching3WKey {
    type ElementType = u32;
    type KeyType = u32;

    #[inline]
    fn null() -> Self {
        Self(Default::default())
    }

    #[inline]
    fn new(values: [Self::ElementType; 3]) -> Self {
        Self(LruVec3::new(values[0], values[1], values[2]))
    }

    #[inline]
    fn key_value(&self) -> Self::KeyType {
        self.0.0[0] ^ self.0.0[1].rotate_left(7) ^ self.0.0[2].rotate_right(17)
    }

    #[inline]
    fn guaranteed_min_len() -> usize {
        0
    }

    #[inline]
    fn advance(&mut self, new_value: Self::ElementType) {
        self.0.push(new_value);
    }
}

/// Least Recently Used vector of 3 elements
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct LruVec3<T>([T; 3]);

impl<T: Copy> LruVec3<T> {
    #[inline]
    pub const fn new(val0: T, val1: T, val2: T) -> Self {
        Self([val0, val1, val2])
    }

    pub fn push(&mut self, val: T) {
        self.0[0] = self.0[1];
        self.0[1] = self.0[2];
        self.0[2] = val;
    }
}

pub struct OffsetList {
    inner: Vec<u32>,
}

impl OffsetList {
    #[inline]
    pub fn new(value: u32) -> Self {
        Self { inner: vec![value] }
    }

    #[inline]
    pub fn push(&mut self, value: u32) {
        self.inner.push(value);
    }

    #[inline]
    pub fn nearest(&self) -> Option<u32> {
        self.inner.last().copied()
    }

    pub fn retain(&mut self, min_value: u32) -> bool {
        match self.nearest() {
            None => return false,
            Some(v) if v < min_value => return false,
            _ => {}
        }
        for (index, item) in self.inner.iter().enumerate().rev() {
            if *item < min_value {
                self.inner.drain(..=index);
                self.inner.shrink_to_fit();
                break;
            }
        }
        true
    }

    #[inline]
    pub fn distances<'a>(
        &'a self,
        current: usize,
        min_value: usize,
    ) -> impl Iterator<Item = NonZero<usize>> + 'a {
        Distances {
            iter: self.inner.iter(),
            current,
            min_value,
        }
    }
}

struct Distances<'a> {
    iter: core::slice::Iter<'a, u32>,
    current: usize,
    min_value: usize,
}

impl Iterator for Distances<'_> {
    type Item = NonZero<usize>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(&value) = self.iter.next_back() {
            let value = value as usize;
            if value >= self.min_value {
                return NonZero::new(self.current - value);
            }
        }
        None
    }
}
