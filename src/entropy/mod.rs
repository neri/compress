//! Entropy coder

use crate::num::math;

#[path = "prefix/prefix.rs"]
pub mod prefix;

pub mod fse;

pub fn entropy_of_blocks(blocks: &[&[u8]]) -> f64 {
    let mut freq_table = [0; 256];
    for bytes in blocks {
        for &byte in bytes.iter() {
            freq_table[byte as usize] += 1;
        }
    }
    entropy_of(&freq_table)
}

pub fn entropy_of_bytes(bytes: &[u8]) -> f64 {
    let mut freq_table = [0; 256];
    for &byte in bytes {
        freq_table[byte as usize] += 1;
    }
    entropy_of(&freq_table)
}

pub fn entropy_of(freq_table: &[usize]) -> f64 {
    let total_size = freq_table.iter().sum::<usize>() as f64;
    let mut entropy = 0.0;
    for &count in freq_table.iter() {
        let p = count as f64 / total_size;
        if p > 0.0 {
            entropy -= p * math::log2(p);
        }
    }
    entropy
}
