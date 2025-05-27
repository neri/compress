//! Math functions using the `libm` crate for compatibility with `no_std` environments.
//!
//! They may be replaced by another implementation in the future.

#[inline(always)]
pub fn log2(x: f64) -> f64 {
    return libm::log2(x);
}

#[inline(always)]
pub fn ceil(x: f64) -> f64 {
    return libm::ceil(x);
}
