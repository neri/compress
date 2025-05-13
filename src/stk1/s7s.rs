use alloc::vec::Vec;

/// One of the representation formats for multibyte integers
///
/// `aaaa_aaa0 bbbb_bbb0 cccc_ccc1 -> a_aaaa_aabb_bbbb_bccc_cccc`
///
/// (C) Kawai Hidemi
///
/// Related Documents: <http://osask.net/w/196.html#j16c9806>
pub struct S7s;

impl S7s {
    pub fn write(output: &mut Vec<u8>, value: usize) {
        let value = value as u64;
        if value < 0x80 {
            output.push((value << 1) as u8 | 1);
        } else if value < 0x40_00 {
            output.push((value >> 6) as u8 & 0xFE);
            output.push((value << 1) as u8 | 1);
        } else if value < 0x20_00_00 {
            output.push((value >> 13) as u8 & 0xFE);
            output.push((value >> 6) as u8 & 0xFE);
            output.push((value << 1) as u8 | 1);
        } else if value < 0x10_00_00_00 {
            output.push((value >> 20) as u8 & 0xFE);
            output.push((value >> 13) as u8 & 0xFE);
            output.push((value >> 6) as u8 & 0xFE);
            output.push((value << 1) as u8 | 1);
        } else if value < 0x8_00_00_00_00 {
            output.push((value >> 27) as u8 & 0xFE);
            output.push((value >> 20) as u8 & 0xFE);
            output.push((value >> 13) as u8 & 0xFE);
            output.push((value >> 6) as u8 & 0xFE);
            output.push((value << 1) as u8 | 1);
        } else if value < 0x4_00_00_00_00_00 {
            output.push((value >> 34) as u8 & 0xFE);
            output.push((value >> 27) as u8 & 0xFE);
            output.push((value >> 20) as u8 & 0xFE);
            output.push((value >> 13) as u8 & 0xFE);
            output.push((value >> 6) as u8 & 0xFE);
            output.push((value << 1) as u8 | 1);
        } else if value < 0x2_00_00_00_00_00_00 {
            output.push((value >> 41) as u8 & 0xFE);
            output.push((value >> 34) as u8 & 0xFE);
            output.push((value >> 27) as u8 & 0xFE);
            output.push((value >> 20) as u8 & 0xFE);
            output.push((value >> 13) as u8 & 0xFE);
            output.push((value >> 6) as u8 & 0xFE);
            output.push((value << 1) as u8 | 1);
        } else if value < 0x1_00_00_00_00_00_00_00 {
            output.push((value >> 48) as u8 & 0xFE);
            output.push((value >> 41) as u8 & 0xFE);
            output.push((value >> 34) as u8 & 0xFE);
            output.push((value >> 27) as u8 & 0xFE);
            output.push((value >> 20) as u8 & 0xFE);
            output.push((value >> 13) as u8 & 0xFE);
            output.push((value >> 6) as u8 & 0xFE);
            output.push((value << 1) as u8 | 1);
        } else if value < 0x80_00_00_00_00_00_00_00 {
            output.push((value >> 55) as u8 & 0xFE);
            output.push((value >> 48) as u8 & 0xFE);
            output.push((value >> 41) as u8 & 0xFE);
            output.push((value >> 34) as u8 & 0xFE);
            output.push((value >> 27) as u8 & 0xFE);
            output.push((value >> 20) as u8 & 0xFE);
            output.push((value >> 13) as u8 & 0xFE);
            output.push((value >> 6) as u8 & 0xFE);
            output.push((value << 1) as u8 | 1);
        } else {
            output.push((value >> 62) as u8 & 0xFE);
            output.push((value >> 55) as u8 & 0xFE);
            output.push((value >> 48) as u8 & 0xFE);
            output.push((value >> 41) as u8 & 0xFE);
            output.push((value >> 34) as u8 & 0xFE);
            output.push((value >> 27) as u8 & 0xFE);
            output.push((value >> 20) as u8 & 0xFE);
            output.push((value >> 13) as u8 & 0xFE);
            output.push((value >> 6) as u8 & 0xFE);
            output.push((value << 1) as u8 | 1);
        }
    }

    pub fn read_with_acc<'a, T>(iter: &mut T, acc: usize) -> Option<usize>
    where
        T: Iterator<Item = &'a u8>,
    {
        let mut acc = acc;
        while (acc & 1) == 0 {
            let next = *iter.next()? as usize;
            acc = (acc << 7) | next;
        }
        Some(acc >> 1)
    }

    #[inline]
    pub fn read<'a, T>(iter: &mut T) -> Option<usize>
    where
        T: Iterator<Item = &'a u8>,
    {
        Self::read_with_acc(iter, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::S7s;

    #[test]
    fn scaled_value() {
        for scale in 1..=32 {
            for source in [
                0, 0x55555555, 0xaaaaaaaa, 1234578, 87654321, 0xEDB88320, 0x04C11DB7, 0xFFFFFFFF,
            ] {
                let mask = 1usize.wrapping_shl(scale).wrapping_sub(1);
                let value = source & mask;

                let mut vec = Vec::new();
                S7s::write(&mut vec, value);

                let mut iter = vec.iter();
                let decoded = S7s::read(&mut iter).unwrap();

                assert_eq!(value, decoded);
            }
        }
    }
}
