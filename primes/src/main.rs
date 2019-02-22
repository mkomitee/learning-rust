mod errors;
mod naive;
mod sieve;

use std::io::{self, BufWriter, Write};
use std::str::FromStr;
use structopt::StructOpt;

#[derive(Debug)]
enum Algorithm {
    Naive,
    Sieve,
}

impl FromStr for Algorithm {
    type Err = errors::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sieve" => Ok(Algorithm::Sieve),
            "naive" => Ok(Algorithm::Naive),
            _ => Err(errors::Error::InvalidAlgorithm),
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "primes",
    about = "Calculate primes.",
    version = "",
    author = ""
)]
struct Opt {
    /// Valid choices are sieve & naive
    algorithm: Algorithm,

    /// Find all primes less than this
    max: u64,
}

// By having main return a result, we can have it exit non-zero and print an error when we
// experience an error by using the ? operator.
fn main() -> Result<(), errors::Error> {
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
