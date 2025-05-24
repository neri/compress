//! My compression library

#![cfg_attr(not(any(test, feature = "std")), no_std)]

extern crate alloc;

// #[cfg(not(feature = "std"))]
// extern crate libm;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

pub mod entropy;
#[path = "lz/lz.rs"]
pub mod lz;
pub mod num;
pub mod stats;

#[path = "stk1/stk1.rs"]
pub mod stk1;

pub mod deflate;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DecodeError {
    InvalidInput,
    InvalidData,
    OutOfMemory,
    UnsupportedFormat,
    UnexpectedEof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    InvalidInput,
    InvalidData,
    OutOfMemory,
    EntropyError,
}
