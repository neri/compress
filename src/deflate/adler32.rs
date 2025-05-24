//! Adler-32 checksum implementation

/// Adler-32 checksum implementation
///
/// References:
///
/// * <https://www.ietf.org/rfc/rfc1950.txt>
/// * <https://en.wikipedia.org/wiki/Adler-32>
///
pub fn checksum(data: &[u8]) -> u32 {
    let mut s1 = 1u32;
    let mut s2 = 0u32;

    for &byte in data {
        s1 = (s1 + byte as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }

    (s2 << 16) | s1
}
