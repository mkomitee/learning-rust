mod errors;
mod naive;
mod options;
mod sieve;

use crate::{
    errors::Error,
    options::{Algorithm, Opt},
};
use std::io::{self, BufWriter, Write};
use structopt::StructOpt;

// By having main return a result, we can have it exit non-zero and print an error when we
// experience an error by using the ? operator.
fn main() -> Result<(), Error> {
    let opt = Opt::from_args();

    let primes = match opt.algorithm {
        Algorithm::Naive => naive::primes(opt.max),
        Algorithm::Sieve => sieve::primes(opt.max),
    };

    // By locking stdout ourselves & using writeln! instead of println!, we avoid having to
    // re-acquire the lock with each write. Then by using a BufWriter instead of stdout directly,
    // we batch many writes together into a single write syscall.
    let stdout = io::stdout();
    let stdout = stdout.lock();
    let mut stdout = BufWriter::new(stdout);

    for i in primes {
        writeln!(stdout, "{}", i)?;
    }
    Ok(())
}
