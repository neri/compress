//! Compression test program.

use compress::stk1::{Configuration, Stk1};
use std::{
    env,
    fs::File,
    io::{Read, Write},
    path::Path,
    process,
};

fn usage() {
    let mut args = env::args_os();
    let arg = args.next().unwrap();
    let path = Path::new(&arg);
    let lpc = path.file_name().unwrap();
    eprintln!("{} [OPTIONS] INFILE OUTFILE", lpc.to_str().unwrap());
    process::exit(1);
}

fn main() {
    let mut args = env::args();
    let _ = args.next().unwrap();

    let mut in_file = None;
    let mut config = Configuration::DEFAULT;
    let mut dry = false;

    while let Some(arg) = args.next() {
        if arg.starts_with("-") {
            match arg.as_str() {
                "--" => {
                    in_file = args.next();
                    break;
                }
                "-dry" => dry = true,
                "-tiny" => config = Configuration::TINY,
                _ => return usage(),
            }
        } else {
            in_file = Some(arg);
            break;
        }
    }
    let in_file = match in_file {
        Some(v) => v,
        None => return usage(),
    };
    let out_file = args.next();
    if !dry && out_file.is_none() {
        return usage();
    }

    let mut is = File::open(in_file).unwrap();
    let mut src = Vec::new();
    let _ = is.read_to_end(&mut src).unwrap();

    let start = std::time::Instant::now();
    let dst = Stk1::encode_with_test(&src, config).unwrap();
    let elapsed = start.elapsed();

    println!(
        "{} bytes <= {} bytes ({:.2}% {:.2}s)",
        dst.len(),
        src.len(),
        dst.len() as f64 / src.len() as f64 * 100.0,
        elapsed.as_secs_f64()
    );

    if let Some(out_file) = out_file {
        let mut os = File::create(out_file).unwrap();
        let _ = os.write_all(&dst).unwrap();
    }
}
