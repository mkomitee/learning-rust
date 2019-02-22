use std::{fmt, io};

pub enum Error {
    InvalidAlgorithm,
    MaxOverflow(u64),
    IO(String),
}

// Required to have structop parse our Algorithm & display an error if the provided option
// isn't valid.
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

// Required to display errors automatically when returned in a Result from main. We could
// derive it, but this allows us to format it how we want.
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Error::InvalidAlgorithm => String::from("choices are sieve & naive"),
            Error::IO(s) => s.to_owned(),
            Error::MaxOverflow(n) => {
                format!("<max> must be less than {} on this platform", n - 1)
            },
        };
        write!(f, "{}", reason)
    }
}

// Converts an io error to our Error.
impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::IO(format!("{}", err))
    }
}
