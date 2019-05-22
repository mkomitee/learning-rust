use failure::{err_msg, Error};
use primes::options::{Algorithm, Opt};
use std::io::{self, BufWriter, Write};
use structopt::StructOpt;

// By having main return a result, we can have it exit non-zero and print an error when we
// experience an error by using the ? operator.
fn main() -> Result<(), Error> {
    let opt = Opt::from_args();

    let primes = match opt.algorithm {
        Algorithm::Naive => primes::naive::primes(opt.max),
        Algorithm::Sieve => {
            // Sieve allocates a vector sized at opt.max + 1. This limits us to addressable memory
            // on the system based on the size of usize.
            if opt.max > (std::usize::MAX - 1) as u64 {
                return Err(err_msg(format!(
                    "<max> must be less than {} on this platform",
                    (std::usize::MAX - 1)
                )));
            }
            primes::sieve::primes(opt.max)
        }
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
