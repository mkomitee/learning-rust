use core::num::IntErrorKind;
use std::{fmt, io, num};

pub enum Error {
    ArgumentMissing,
    ArgumentOverflow,
    ArgumentInvalid,
    InvalidAlgorithm,
    IO,
}

fn usage(error: &str) -> String {
    return format!("{}\n\nusage: primes [naive|sieve] MAX", error);
}

// Required to display errors automatically when returned in a Result from main. We could
// derive it, but this allows us to format it how we want.
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Error::ArgumentMissing => usage("not enough arguments"),
            Error::ArgumentInvalid => usage("max must be a positive integer"),
            Error::ArgumentOverflow => usage("max too large"),
            Error::InvalidAlgorithm => usage("invalid algorithm"),
            Error::IO => String::from("io error"),
        };
        write!(f, "{}", reason)
    }
}

// Converts a ParseIntError to our Error type with special handling for overflows. Requires
// nightly.
impl From<num::ParseIntError> for Error {
    fn from(err: num::ParseIntError) -> Error {
        return match err.kind() {
            IntErrorKind::Overflow => Error::ArgumentOverflow,
            _ => Error::ArgumentInvalid,
        };
    }
}

// Converts an io error to our Error.
impl From<io::Error> for Error {
    fn from(_err: io::Error) -> Error {
        Error::IO
    }
}
