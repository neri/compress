//! Lempel-Ziv compression utilities
//!
//! See also: <https://en.wikipedia.org/wiki/LZ77_and_LZ78>

pub mod cache;
pub mod lzss;

#[path = "match_finder/match_finder.rs"]
pub mod match_finder;

mod slice_window;
pub use slice_window::*;

#[inline]
#[track_caller]
pub fn matching_len<T>(data: &[T], current: usize, distance: usize) -> usize
where
    T: Sized + Copy + PartialEq,
{
    debug_assert!(
        data.len() >= current && distance != 0 && current >= distance,
        "INVALID MATCHES: LEN {} CURRENT {} DISTANCE {}",
        data.len(),
        current,
        distance
    );
    unsafe {
        // Safety: `data` is guaranteed to be valid, and `current` and `distance` are checked.
        let max_len = data.len() - current;
        let p = data.as_ptr().add(current);
        let q = data.as_ptr().add(current - distance);

        for len in 0..max_len {
            if p.add(len).read_volatile() != q.add(len).read_volatile() {
                return len;
            }
        }
        max_len
    }
}

#[inline]
pub fn find_distance_matches<T: Sized + Copy + PartialEq>(
    input: &[T],
    cursor: usize,
    threshold_min: usize,
    threshold_max: usize,
    guaranteed_min_len: usize,
    dist_iter: impl Iterator<Item = usize>,
) -> Option<Match> {
    let threshold_min_len = threshold_min.saturating_sub(guaranteed_min_len);
    let threshold_max_len = threshold_max.saturating_sub(guaranteed_min_len);
    let cursor = cursor + guaranteed_min_len;
    let mut matches = Match::ZERO;
    for distance in dist_iter {
        let len = matching_len(input, cursor, distance) + guaranteed_min_len;
        if matches.len < len {
            matches = Match::new(len, distance);
            if matches.len >= threshold_max_len {
                break;
            }
        }
    }
    (matches.len >= threshold_min_len as usize).then(|| matches)
}

/// Matching distance and length
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub len: usize,
    pub distance: usize,
}

impl Match {
    pub const ZERO: Self = Self::new(0, 0);

    #[inline]
    pub const fn new(len: usize, distance: usize) -> Self {
        Self { len, distance }
    }

    #[inline]
    pub const fn is_zero(&self) -> bool {
        self.len == 0
    }
}

impl Default for Match {
    #[inline]
    fn default() -> Self {
        Self::ZERO
    }
}

pub struct LzOutputBuffer<'a> {
    buffer: &'a mut [u8],
    position: usize,
}

impl<'a> LzOutputBuffer<'a> {
    #[inline]
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self {
            buffer,
            position: 0,
        }
    }

    #[inline]
    pub fn is_eof(&self) -> bool {
        self.position >= self.buffer.len()
    }

    #[inline]
    pub fn push_literal(&mut self, literal: u8) -> LzOutputBufferResult {
        if self.position < self.buffer.len() {
            self.buffer[self.position] = literal;
            self.position += 1;
            LzOutputBufferResult::Success
        } else {
            LzOutputBufferResult::Failure
        }
    }

    pub fn extend_from_slice(&mut self, data: &[u8]) -> LzOutputBufferResult {
        if self.position + data.len() <= self.buffer.len() {
            self.buffer[self.position..self.position + data.len()].copy_from_slice(data);
            self.position += data.len();
            LzOutputBufferResult::Success
        } else {
            LzOutputBufferResult::Failure
        }
    }

    pub fn copy_lz(&mut self, distance: usize, copy_len: usize) -> LzOutputBufferResult {
        if distance > self.position {
            return LzOutputBufferResult::Failure;
        }
        let copy_len = copy_len.min(self.buffer.len() - self.position);
        unsafe {
            // Safety: distance is guaranteed to be valid, and copy_len is checked against the buffer size.
            let dest = self.buffer.as_mut_ptr().add(self.position);
            if distance == 1 {
                core::slice::from_raw_parts_mut(dest, copy_len).fill(dest.sub(1).read_volatile());
            } else {
                _memcpy(dest, dest.sub(distance), copy_len);
            }
        }
        self.position += copy_len;

        LzOutputBufferResult::Success
    }
}

/// # Safety
///
/// Everything is the caller's responsibility.
#[inline]
unsafe fn _memcpy(dest: *mut u8, src: *const u8, count: usize) {
    unsafe {
        let mut dest = dest;
        let mut src = src;
        for _ in 0..count {
            dest.write(src.read());
            dest = dest.add(1);
            src = src.add(1);
        }
    }
}

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LzOutputBufferResult {
    Success,
    Failure,
}

impl LzOutputBufferResult {
    #[inline]
    pub fn ok_or<E>(self, e: E) -> Result<(), E> {
        match self {
            LzOutputBufferResult::Success => Ok(()),
            LzOutputBufferResult::Failure => Err(e),
        }
    }
}
