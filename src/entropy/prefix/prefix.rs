//! Canonical Prefix Coder
//!
//! See also: <https://en.wikipedia.org/wiki/Canonical_Huffman_code>

mod decode;
mod encode;
pub use decode::*;
pub use encode::*;

pub mod simple;

/// Repeat the previous value `3 + readbits(2)` times
pub const REP3P2: u8 = 16;
/// Repeat 0 `3 + readbits(3)` times
pub const REP3Z3: u8 = 17;
/// Repeat 0 `11 + readbits(7)` times
pub const REP11Z7: u8 = 18;

/// In deflate, Huffman tables are sorted in a specific order to keep their size small.
#[derive(Debug, Clone, Copy, Default)]
pub enum PermutationFlavor {
    /// 16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    #[default]
    Deflate,
    /// 17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    WebP,
}

impl PermutationFlavor {
    const ORDER_DEFLATE: &[u8; 19] = &[
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];

    const ORDER_WEBP: &[u8; 19] = &[
        17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    ];

    pub fn permutation_order(&self) -> &'static [u8; 19] {
        match self {
            Self::Deflate => Self::ORDER_DEFLATE,
            Self::WebP => Self::ORDER_WEBP,
        }
    }
}
