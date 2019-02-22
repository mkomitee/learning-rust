mod errors;
mod naive;
mod options;
mod sieve;

use std::io::{self, BufWriter, Write};
use structopt::StructOpt;

// By having main return a result, we can have it exit non-zero and print an error when we
// experience an error by using the ? operator.
fn main() -> Result<(), errors::Error> {
    let opt = options::Opt::from_args();

    let primes = match opt.algorithm {
        options::Algorithm::Naive => naive::primes(opt.max),
        options::Algorithm::Sieve => sieve::primes(opt.max),
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
