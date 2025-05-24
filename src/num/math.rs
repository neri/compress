#[inline(always)]
pub fn log2(x: f64) -> f64 {
    return libm::log2(x);
}

#[inline(always)]
pub fn ceil(x: f64) -> f64 {
    return libm::ceil(x);
}
