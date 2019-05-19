// Thanks to Globi::<!> for this monstrosity. It allows us to write a function generic over
// stdout/stderr and still be able to lock() them.
#![warn(rust_2018_idioms)]

use std::io::{self, Write};

// A trait family for type constructors that have a lifetime and are Write
trait WriteFamilyLt<'a> {
    type Out: Write;
}

trait LockWrite {
    type Locked: for<'a> WriteFamilyLt<'a>;

    fn lock(&self) -> <Self::Locked as WriteFamilyLt<'_>>::Out;
}

struct StderrLockFamily;
impl<'a> WriteFamilyLt<'a> for StderrLockFamily {
    type Out = io::StderrLock<'a>;
}

impl LockWrite for io::Stderr {
    type Locked = StderrLockFamily;

    fn lock(&self) -> io::StderrLock<'_> {
        self.lock()
    }
}

struct StdoutLockFamily;
impl<'a> WriteFamilyLt<'a> for StdoutLockFamily {
    type Out = io::StdoutLock<'a>;
}

impl LockWrite for io::Stdout {
    type Locked = StdoutLockFamily;

    fn lock(&self) -> io::StdoutLock<'_> {
        self.lock()
    }
}

fn write(fhandle: impl LockWrite, messages: &[&[u8]]) {
    let mut fhandle = fhandle.lock();
    for message in messages {
        fhandle.write(message).unwrap();
    }
    fhandle.write(b"\n").unwrap();
}

fn main() {
    write(io::stdout(), &[b"a", b"b"]);
    write(io::stderr(), &[b"c", b"d"]);
}
