//! Compression test program
//!
//! This application is for testing purposes only and is not intended for practical use

use compress::{
    deflate::{CompressionLevel, deflate},
    lz::lzss,
};
use std::{env, path::Path, process};

fn main() {
    let mut args = env::args();
    let _ = args.next().unwrap();

    let time0 = std::time::Instant::now();
    while time0.elapsed().as_secs_f64() < 1.0 {
        stabilize();
    }

    let src_size = 0x10000;
    // let input = fib_str(0x55, 0xaa, src_size);
    let input = random_ab(0x55, 0xaa, src_size);
    // let input = random_alphabet(b'A', b'Z', src_size);

    #[allow(dead_code)]
    fn calc(acc: &mut usize, item: lzss::LZSS) {
        match item {
            lzss::LZSS::Literal(_) => *acc += 1,
            lzss::LZSS::Match(_) => *acc += 3,
        }
    }

    for _ in 0..5 {
        let times = 100;

        let time0 = std::time::Instant::now();
        let mut encode_size_fast = 0;
        for _ in 0..times {
            encode_size_fast = deflate(&input, CompressionLevel::Fastest, None)
                .unwrap()
                .len();
        }
        let elapsed_fast = time0.elapsed();

        // let time0 = std::time::Instant::now();
        // for _ in 0..times {
        //     let _ = compress::lz::match_finder::MatchFinder::new(&input);
        // }
        // let elapsed_mf = time0.elapsed();

        let time0 = std::time::Instant::now();
        let mut encode_size_default = 0;
        for _ in 0..times {
            encode_size_default = deflate(&input, CompressionLevel::Default, None)
                .unwrap()
                .len();
        }
        let elapsed_default = time0.elapsed();

        let time0 = std::time::Instant::now();
        let mut encode_size_best = 0;
        for _ in 0..times {
            encode_size_best = deflate(&input, CompressionLevel::Best, None).unwrap().len();
        }
        let elapsed_best = time0.elapsed();

        println!(
            "times {}: fast: {:.03}kb {:.02}% {:.03}s, default: {:.03}kb {:.02}% {:.03}s best: {:.03}kb {:.02}% {:.03}s",
            times,
            encode_size_fast as f64 / 1024.0,
            encode_size_fast as f64 / src_size as f64 * 100.0,
            elapsed_fast.as_secs_f64(),
            encode_size_default as f64 / 1024.0,
            encode_size_default as f64 / src_size as f64 * 100.0,
            elapsed_default.as_secs_f64(),
            encode_size_best as f64 / 1024.0,
            encode_size_best as f64 / src_size as f64 * 100.0,
            elapsed_best.as_secs_f64(),
        );
    }
}

#[allow(unused)]
fn usage() {
    let mut args = env::args_os();
    let arg = args.next().unwrap();
    let path = Path::new(&arg);
    let lpc = path.file_name().unwrap();
    eprintln!("{} [OPTIONS] INFILE OUTFILE", lpc.to_str().unwrap());
    process::exit(1);
}

/// A Fibonacci word generator for testing purposes.
#[allow(unused)]
fn fib_str(a: u8, b: u8, limit: usize) -> Vec<u8> {
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

#[allow(unused)]
fn random_ab(a: u8, b: u8, limit: usize) -> Vec<u8> {
    use rand::RngCore;
    let mut rng = rand::rng();
    let mut v = Vec::with_capacity(limit);
    for _ in 0..limit {
        v.push(if rng.next_u32() % 2 == 0 { a } else { b })
    }
    v
}

#[allow(unused)]
fn random_alphabet(min: u8, max: u8, limit: usize) -> Vec<u8> {
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

fn stabilize() {
    use rand::RngCore;
    let mut rng = rand::rng();
    let len = 0x1000 + (rng.next_u32() as usize & 0xfffff);
    let mut v = Vec::with_capacity(len);
    rng.fill_bytes(&mut v);
}
