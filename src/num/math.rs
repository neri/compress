#[inline]
pub fn log2(x: f64) -> f64 {
    // #[cfg(feature = "std")]
    // return x.log2();

    // #[cfg(not(feature = "std"))]
    return libm::log2(x);
}
