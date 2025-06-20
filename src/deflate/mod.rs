//! Deflate compression algorithm
//!
//! See also: <https://www.ietf.org/rfc/rfc1951.txt>

use crate::{
    num::{
        VarLenInteger,
        bits::{BitSize, BitStreamReader},
    },
    *,
};

#[cfg(test)]
mod tests;

pub mod adler32;

mod deflate;
mod inflate;
pub use deflate::*;
pub use inflate::*;

macro_rules! var_uint32 {
    ($class_name:ident, $base_table:ident, $min_value:expr, $max_value:expr) => {
        #[derive(Debug, PartialEq)]
        pub struct $class_name {
            pub trailing: Option<VarLenInteger>,
            pub leading: u8,
        }

        impl $class_name {
            pub const MIN: u32 = $min_value;

            pub const MAX: u32 = $max_value;

            #[inline]
            pub fn new(value: u32) -> Option<Self> {
                for (index, &item) in $base_table.iter().enumerate().rev() {
                    let (size, min_value) = item;
                    if value < min_value {
                        continue;
                    }
                    let leading = index as u8;
                    let value = value.checked_sub(min_value)?;
                    let max_value = (1u32 << size.map(|v| v as u32).unwrap_or_default()) - 1;
                    if value > max_value {
                        return None;
                    }
                    let trailing = size.map(|size| unsafe {
                        // Safety: The value is checked to be within the valid range
                        VarLenInteger::from_raw_parts(size, value)
                    });
                    return Some(Self { leading, trailing });
                }
                None
            }

            #[inline]
            pub fn value(&self) -> u32 {
                $base_table[self.leading as usize].1
                    + self.trailing.map(|v| v.value()).unwrap_or_default()
            }

            pub fn decode(leading: u8, reader: &mut BitStreamReader) -> Option<Self> {
                let (ext_bit, _min_value) = *($base_table.get(leading as usize)?);
                if let Some(ext_bit) = ext_bit {
                    let trailing = reader.read_bits(ext_bit).map(|value| unsafe {
                        // Safety: The value is guaranteed to be a valid bit size
                        VarLenInteger::from_raw_parts(ext_bit, value)
                    })?;
                    Some(Self {
                        leading,
                        trailing: Some(trailing),
                    })
                } else {
                    Some(Self {
                        leading,
                        trailing: None,
                    })
                }
            }

            #[inline]
            pub fn decode_value(leading: u8, reader: &mut BitStreamReader) -> Option<u32> {
                let (ext_bit, min_value) = *($base_table.get(leading as usize)?);
                if let Some(ext_bit) = ext_bit {
                    let trailing = reader.read_bits(ext_bit)?;
                    Some(min_value + trailing)
                } else {
                    Some(min_value)
                }
            }

            #[inline]
            pub const fn from_raw(leading: u8, trailing: Option<VarLenInteger>) -> Self {
                Self { leading, trailing }
            }

            #[inline]
            pub const fn leading(&self) -> u8 {
                self.leading
            }

            #[inline]
            pub const fn trailing(&self) -> Option<VarLenInteger> {
                self.trailing
            }

            #[inline]
            pub fn trailing_bits_for(leading: u8) -> Option<BitSize> {
                let (size, _) = $base_table.get(leading as usize)?;
                *size
            }
        }
    };
}

//      Extra           Extra               Extra
// Code Bits Dist  Code Bits   Dist     Code Bits Distance
// ---- ---- ----  ---- ----  ------    ---- ---- --------
//  0   0    1     10   4     33-48    20    9   1025-1536
//  1   0    2     11   4     49-64    21    9   1537-2048
//  2   0    3     12   5     65-96    22   10   2049-3072
//  3   0    4     13   5     97-128   23   10   3073-4096
//  4   1   5,6    14   6    129-192   24   11   4097-6144
//  5   1   7,8    15   6    193-256   25   11   6145-8192
//  6   2   9-12   16   7    257-384   26   12  8193-12288
//  7   2  13-16   17   7    385-512   27   12 12289-16384
//  8   3  17-24   18   8    513-768   28   13 16385-24576
//  9   3  25-32   19   8   769-1024   29   13 24577-32768
var_uint32!(DistanceType, VARIABLE_DISTANCE_BASE_TABLE, 1, 32768);
static VARIABLE_DISTANCE_BASE_TABLE: [(Option<BitSize>, u32); 30] = [
    (None, 1),
    (None, 2),
    (None, 3),
    (None, 4),
    (Some(BitSize::Bit1), 5),
    (Some(BitSize::Bit1), 7),
    (Some(BitSize::Bit2), 9),
    (Some(BitSize::Bit2), 13),
    (Some(BitSize::Bit3), 17),
    (Some(BitSize::Bit3), 25),
    (Some(BitSize::Bit4), 33),
    (Some(BitSize::Bit4), 49),
    (Some(BitSize::Bit5), 65),
    (Some(BitSize::Bit5), 97),
    (Some(BitSize::Bit6), 129),
    (Some(BitSize::Bit6), 193),
    (Some(BitSize::Bit7), 257),
    (Some(BitSize::Bit7), 385),
    (Some(BitSize::Bit8), 513),
    (Some(BitSize::Bit8), 769),
    (Some(BitSize::Bit9), 1025),
    (Some(BitSize::Bit9), 1537),
    (Some(BitSize::Bit10), 2049),
    (Some(BitSize::Bit10), 3073),
    (Some(BitSize::Bit11), 4097),
    (Some(BitSize::Bit11), 6145),
    (Some(BitSize::Bit12), 8193),
    (Some(BitSize::Bit12), 12289),
    (Some(BitSize::Bit13), 16385),
    (Some(BitSize::Bit13), 24577),
];

//      Extra               Extra               Extra
// Code Bits Length(s) Code Bits Lengths   Code Bits Length(s)
// ---- ---- ------     ---- ---- -------   ---- ---- -------
//  257   0     3       267   1   15,16     277   4   67-82
//  258   0     4       268   1   17,18     278   4   83-98
//  259   0     5       269   2   19-22     279   4   99-114
//  260   0     6       270   2   23-26     280   4  115-130
//  261   0     7       271   2   27-30     281   5  131-162
//  262   0     8       272   2   31-34     282   5  163-194
//  263   0     9       273   3   35-42     283   5  195-226
//  264   0    10       274   3   43-50     284   5  227-257
//  265   1  11,12      275   3   51-58     285   0    258
//  266   1  13,14      276   3   59-66
var_uint32!(LenType, VARIABLE_LENGTH_BASE_TABLE, 3, 258);
static VARIABLE_LENGTH_BASE_TABLE: [(Option<BitSize>, u32); 29] = [
    (None, 3),
    (None, 4),
    (None, 5),
    (None, 6),
    (None, 7),
    (None, 8),
    (None, 9),
    (None, 10),
    (Some(BitSize::Bit1), 11),
    (Some(BitSize::Bit1), 13),
    (Some(BitSize::Bit1), 15),
    (Some(BitSize::Bit1), 17),
    (Some(BitSize::Bit2), 19),
    (Some(BitSize::Bit2), 23),
    (Some(BitSize::Bit2), 27),
    (Some(BitSize::Bit2), 31),
    (Some(BitSize::Bit3), 35),
    (Some(BitSize::Bit3), 43),
    (Some(BitSize::Bit3), 51),
    (Some(BitSize::Bit3), 59),
    (Some(BitSize::Bit4), 67),
    (Some(BitSize::Bit4), 83),
    (Some(BitSize::Bit4), 99),
    (Some(BitSize::Bit4), 115),
    (Some(BitSize::Bit5), 131),
    (Some(BitSize::Bit5), 163),
    (Some(BitSize::Bit5), 195),
    (Some(BitSize::Bit5), 227),
    (None, 258),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum WindowSize {
    Size256 = 0,
    Size512,
    Size1024,
    Size2048,
    Size4096,
    Size8192,
    Size16384,
    #[default]
    Size32768,
}

impl WindowSize {
    #[inline]
    pub const fn preferred(size: usize) -> Self {
        match size {
            ..=256 => Self::Size256,
            ..=512 => Self::Size512,
            ..=1024 => Self::Size1024,
            ..=2048 => Self::Size2048,
            ..=4096 => Self::Size4096,
            ..=8192 => Self::Size8192,
            ..=16384 => Self::Size16384,
            _ => Self::Size32768,
        }
    }

    #[inline]
    pub const fn value(&self) -> usize {
        256 << *self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum CompressionLevel {
    /// Compress as fast as possible
    Fastest = 0,
    /// Currently, same as `Default`
    Fast = 1,
    /// Default compression level, balances speed and compression ratio
    #[default]
    Default = 6,
    /// Compress as much as possible
    Best = 9,
}

impl CompressionLevel {
    #[inline]
    pub const fn is_fast_method(&self) -> bool {
        matches!(self, Self::Fastest | Self::Fast)
    }

    #[inline]
    pub const fn is_best_method(&self) -> bool {
        matches!(self, Self::Best | Self::Default)
    }

    #[inline]
    pub const fn zlib_flevel(&self) -> u8 {
        match self {
            CompressionLevel::Fastest => 0,
            CompressionLevel::Fast => 1,
            CompressionLevel::Default => 2,
            CompressionLevel::Best => 3,
        }
    }
}
