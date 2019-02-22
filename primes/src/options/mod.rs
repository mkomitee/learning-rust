use crate::errors::Error;
use std::str::FromStr;
use structopt::StructOpt;

#[derive(Debug)]
pub enum Algorithm {
    Naive,
    Sieve,
}

impl FromStr for Algorithm {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sieve" => Ok(Algorithm::Sieve),
            "naive" => Ok(Algorithm::Naive),
            _ => Err(Error::InvalidAlgorithm),
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    author = "\n",
    about = "    Calculate primes with multiple algorithms.",
    raw(setting = "structopt::clap::AppSettings::AllowNegativeNumbers")
)]
pub struct Opt {
    /// Valid choices are sieve & naive
    pub algorithm: Algorithm,

    /// Find all primes less than this
    pub max: u64,
}
