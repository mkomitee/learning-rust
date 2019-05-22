#![cfg_attr(feature = "nightly", feature(int_error_matching))]
use std::io::{self, BufWriter, Write};
use std::{env, fmt, num};

#[cfg(feature = "nightly")]
use core::num::IntErrorKind;

#[cfg(feature = "nightly")]
enum Error {
    ArgumentMissing,
    ArgumentEmpty,
    ArgumentOverflow,
    ArgumentInvalid,
    IO,
}

#[cfg(feature = "nightly")]
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Error::ArgumentMissing => "not enough arguments specified",
            Error::ArgumentInvalid => "specified argument is invalid",
            Error::ArgumentEmpty => "specified argument is empty",
            Error::ArgumentOverflow => "specified argument is too large",
            Error::IO => "io error",
        };
        write!(f, "{}", reason)
    }
}

#[cfg(feature = "nightly")]
impl From<num::ParseIntError> for Error {
    fn from(err: num::ParseIntError) -> Error {
        match err.kind() {
            IntErrorKind::Empty => Error::ArgumentEmpty,
            IntErrorKind::Overflow => Error::ArgumentOverflow,
            _ => Error::ArgumentInvalid,
        }
    }
}

#[cfg(not(feature = "nightly"))]
enum Error {
    ArgumentMissing,
    ArgumentInvalid,
    IO,
}

#[cfg(not(feature = "nightly"))]
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Error::ArgumentMissing => "not enough arguments specified",
            Error::ArgumentInvalid => "specified argument is invalid",
            Error::IO => "io error",
        };
        write!(f, "{}", reason)
    }
}

#[cfg(not(feature = "nightly"))]
impl From<num::ParseIntError> for Error {
    fn from(_err: num::ParseIntError) -> Error {
        Error::ArgumentInvalid
    }
}

impl From<io::Error> for Error {
    fn from(_err: io::Error) -> Error {
        Error::IO
    }
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    let arg = args.get(1).ok_or(Error::ArgumentMissing)?;
    let max: u64 = arg.parse()?;

    // By locking stdout ourselves & using writeln! instead of println!, we avoid having to
    // re-acquire the lock with each write. Then by using a BufWriter instead of stdout directly,
    // we batch many writes together into a single write syscall.
    let stdout = io::stdout();
    let stdout = stdout.lock();

    let mut stdout = BufWriter::new(stdout);

    for i in 1..=max {
        match i {
            v if v % 15 == 0 => writeln!(stdout, "FizzBuzz!"),
            v if v % 3 == 0 => writeln!(stdout, "fizz"),
            v if v % 5 == 0 => writeln!(stdout, "buzz"),
            v => writeln!(stdout, "{}", v),
        }?;
    }
    Ok(())
}
