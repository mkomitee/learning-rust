#![feature(int_error_matching)]
mod errors;
mod naive;
mod sieve;

use std::env;
use std::io::{self, BufWriter, Write};

// By having main return a result, we can have it exit non-zero and print an error when we
// experience an error by using the ? operator.
fn main() -> Result<(), errors::Error> {
    let args: Vec<String> = env::args().collect();
    let arg1 = args.get(1).ok_or(errors::Error::ArgumentMissing)?;
    let arg2 = args.get(2).ok_or(errors::Error::ArgumentMissing)?;
    let max: u64 = arg2.parse()?;

    let iter = match &arg1[..] {
        "naive" => Ok(naive::new(max)),
        "sieve" => Ok(sieve::new(max)),
        _ => Err(errors::Error::InvalidAlgorithm),
    }?;

    // By locking stdout ourselves & using writeln! instead of println!, we avoid having to
    // re-acquire the lock with each write. Then by using a BufWriter instead of stdout directly,
    // we batch many writes together into a single write syscall.
    let stdout = io::stdout();
    let stdout = stdout.lock();
    let mut stdout = BufWriter::new(stdout);

    for i in iter {
        writeln!(stdout, "{}", i)?;
    }
    Ok(())
}
