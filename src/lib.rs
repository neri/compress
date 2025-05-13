//! My compression library

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

pub mod entropy;
#[path = "lz/lz.rs"]
pub mod lz;
pub mod num;
pub mod slice_window;
pub mod stats;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DecodeError {
    InvalidInput,
    InvalidData,
    OutOfMemory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EncodeError {
    InvalidInput,
    InvalidData,
    OutOfMemory,
    EntropyError,
}
