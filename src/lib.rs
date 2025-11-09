//! My compression library

// #![cfg_attr(not(any(test, feature = "std")), no_std)]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

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
    InternalInconsistency,
}

/// A Fibonacci word generator for testing purposes.
#[cfg(test)]
pub(crate) fn fib_str(a: u8, b: u8, limit: usize) -> Vec<u8> {
    use core::mem::swap;
    let mut n = 1;
    let mut x = Vec::new();
    let mut y: Vec<u8> = Vec::new();
    let mut c = Vec::new();
    while x.len() < limit {
        match n {
            0 => {}
            1 => x.push(a),
            2 => y.push(b),
            _ => {
                c.clear();
                c.extend_from_slice(&x);
                c.extend_from_slice(&y);
                swap(&mut x, &mut y);
                swap(&mut x, &mut c);
            }
        }
        n += 1;
    }
    x.truncate(limit);
    x
}

#[cfg(test)]
pub(crate) fn random_ab(a: u8, b: u8, limit: usize) -> Vec<u8> {
    use rand::RngCore;
    let mut rng = rand::rng();
    let mut v = Vec::with_capacity(limit);
    for _ in 0..limit {
        v.push(if rng.next_u32() % 2 == 0 { a } else { b })
    }
    v
}

#[cfg(test)]
pub(crate) fn random_alphabet(min: u8, max: u8, limit: usize) -> Vec<u8> {
    use rand::RngCore;
    assert!(min < max, "min must be less than max");
    let min = min as u32;
    let range_max = max as u32 - min;
    let mask = (range_max + 1).next_power_of_two() - 1;
    let mut rng = rand::rng();
    let mut v = Vec::with_capacity(limit);
    while v.len() < limit {
        let rand = rng.next_u32() & mask;
        if rand <= range_max {
            v.push((rand + min) as u8);
        }
    }
    v
}
