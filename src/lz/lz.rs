//! Lempel-Ziv compression utilities

pub mod cache;
pub mod lzss;
// pub mod sais;

#[inline]
#[track_caller]
pub fn matching_len<T>(data: &[T], current: usize, distance: usize, max_len: usize) -> usize
where
    T: Sized + Copy + PartialEq,
{
    if true {
        assert!(
            data.len() >= current && distance != 0 && current >= distance,
            "INVALID MATCHES: LEN {} CURRENT {} DISTANCE {}",
            data.len(),
            current,
            distance
        );
    }
    unsafe {
        let max_len = (data.len() - current).min(max_len);
        let p = data.as_ptr().add(current);
        let q = data.as_ptr().add(current - distance);

        #[allow(unused_mut)]
        let mut len = 0;
        // if size_of::<T>() == 1 && max_len >= 4 {
        //     let max_len_m3 = max_len - 3;
        //     while len < max_len_m3 {
        //         if (p.add(len) as *const u32).read_volatile()
        //             != (q.add(len) as *const u32).read_volatile()
        //         {
        //             break;
        //         }
        //         len += 4;
        //     }
        // }
        for len in len..max_len {
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
    max_len: usize,
    threshold_min: usize,
    threshold_max: usize,
    guaranteed_min_len: usize,
    dist_iter: impl Iterator<Item = usize>,
) -> Option<Matches> {
    let threshold_min_len = threshold_min.saturating_sub(guaranteed_min_len);
    let threshold_max_len = threshold_max.saturating_sub(guaranteed_min_len);
    let cursor = cursor + guaranteed_min_len;
    let max_len = max_len.saturating_sub(guaranteed_min_len);
    let mut matches = Matches::ZERO;
    for distance in dist_iter {
        let len = matching_len(input, cursor, distance, max_len) + guaranteed_min_len;
        if matches.len < len {
            matches = Matches::new(len, distance);
            if matches.len >= threshold_max_len {
                break;
            }
        }
    }
    (matches.len >= threshold_min_len as usize).then(|| matches)
}

/// Matching distance and length
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Matches {
    pub len: usize,
    pub distance: usize,
}

impl Matches {
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

impl Default for Matches {
    #[inline]
    fn default() -> Self {
        Self::ZERO
    }
}
