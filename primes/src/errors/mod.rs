use std::{fmt, io, string};

pub enum Error {
    InvalidAlgorithm,
    IO(String),
}

impl string::ToString for Error {
    fn to_string(&self) -> String {
        return format!("{:?}", self);
    }
}

// Required to display errors automatically when returned in a Result from main. We could
// derive it, but this allows us to format it how we want.
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Error::InvalidAlgorithm => "choices are sieve & naive",
            Error::IO(s) => s,
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
