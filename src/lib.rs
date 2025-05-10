//! Entropy encoding library

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

pub mod bits;
pub mod fse;
pub mod num;
pub mod prefix;
pub mod stats;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DecodeError {
    InvalidInput,
    InvalidData,
    OutOfMemory,
}
