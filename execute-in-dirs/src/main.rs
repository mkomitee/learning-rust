use os_pipe::{pipe, PipeReader};
use std::borrow::ToOwned;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;
use std::process::{exit, Command, ExitStatus};
use std::result::Result;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std_semaphore::Semaphore;
use structopt;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "execute-in-dirs",
    about = "Execute the same command in multiple directories."
)]
struct Opt {
    /// Directories in which to execute command
    // OsString because filenames are not guaranteed to be utf8.
    #[structopt(parse(from_os_str), raw(required = "true"))]
    directory: Vec<OsString>,
    /// Command to execute
    // OsString because files & arguments are not guaranteed to be utf8.
    #[structopt(parse(from_os_str), raw(last = "true", required = "true"))]
    arg: Vec<OsString>,
    #[structopt(short = "c", long = "max-concurrency", default_value = "8")]
    concurrency: isize,
}

enum ProcessExitResult {
    IOError(io::Error),
    Code(i32),
    Signal(i32),
}

struct ProcessResult {
    cwd: OsString,
    exit: ProcessExitResult,
}

impl From<Result<ExitStatus, io::Error>> for ProcessExitResult {
    fn from(result: Result<ExitStatus, io::Error>) -> Self {
        match result {
            Err(err) => ProcessExitResult::IOError(err),
            Ok(estatus) => {
                if let Some(code) = estatus.code() {
                    ProcessExitResult::Code(code)
                } else {
                    // We expect here because we know that the command executed (no Err), that it
                    // finished (or there would be no ExitStatus) and that it didn't exit with an
                    // exit code (estatus.code() -> None). While the compiler can't prove it,
                    // there's no other possibility.
                    ProcessExitResult::Signal(
                        estatus
                            .signal()
                            .expect("process exited with no exit code or signal!"),
                    )
                }
            }
        }
    }
}

// This abomination exists solely because I can't figure out how to write a function generic over
// stdout & stderr which doesn't at the same time lose the ability to lock them for multiple
// writes.
#[derive(Clone)]
enum StdIOTarget {
    Stdout,
    Stderr,
}

// Note, by handing everything off to our io threads, we're avoiding having to lock/unlock
// stdout/stderr over and over, but at the cost of a whole lot of extra cloning. That's probably? a
// bad trade-off.
fn stream_output(target: &StdIOTarget, reader: PipeReader, prefix: &OsString) {
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        let result = reader.read_until(b'\n', &mut buf);
        match result {
            // If we got 0 bytes or an error, we're done. Return.
            Err(_) | Ok(0) => {
                return;
            }
            //  Otherwise ...
            Ok(_) => {
                match target {
                    StdIOTarget::Stdout => write_with_prefix(stdout.lock(), prefix, &buf),
                    StdIOTarget::Stderr => write_with_prefix(stderr.lock(), prefix, &buf),
                }
                buf.clear();
            }
        }
    }
}

fn write_with_prefix<T: Write>(mut handle: T, prefix: &OsString, message: &[u8]) {
    for token in &[prefix.as_bytes(), b": ", message] {
        let _ = handle.write(token);
    }
}

fn main() {
    // Thank you structopt.
    let opt = Opt::from_args();

    // We need to split argv0 from the rest for Command ...
    let exec = opt.arg[0].to_owned();
    let args: Vec<OsString> = opt.arg.iter().skip(1).map(ToOwned::to_owned).collect();

    // Processing our results at the end ...
    let mut results = Vec::new();
    let (tx, rx) = channel::<ProcessResult>();

    // We limit concurrency with a semaphore ...
    let semaphore = Arc::new(Semaphore::new(opt.concurrency));

    // Track our threads so we can ensure they complete ...
    let mut cmd_threads = Vec::new();

    for cwd in opt.directory {
        let exec = exec.clone();
        let args = args.clone();
        let semaphore = semaphore.clone();
        let tx = tx.clone();
        cmd_threads.push(thread::spawn(move || {
            // Acquire our guard to limit concurrency
            let _guard = semaphore.access();

            // Setup our pipes for the command
            let (o_reader, o_writer) = match pipe() {
                Ok((o_reader, o_writer)) => (o_reader, o_writer),
                // Couldn't create our pipes. I suspect a ulimit issue, but there's nothing we can
                // do but note the failure and return.
                Err(err) => {
                    tx.send(ProcessResult {
                        exit: ProcessExitResult::IOError(err),
                        cwd,
                    })
                    .unwrap();
                    return;
                }
            };
            let (e_reader, e_writer) = match pipe() {
                Ok((e_reader, e_writer)) => (e_reader, e_writer),
                // Couldn't create our pipes. I suspect a ulimit issue, but there's nothing we can
                // do but note the failure and return.
                Err(err) => {
                    tx.send(ProcessResult {
                        exit: ProcessExitResult::IOError(err),
                        cwd,
                    })
                    .unwrap();
                    return;
                }
            };

            // Spawn our command ...
            let child = Command::new(exec)
                .args(args)
                .current_dir(&cwd)
                .stdout(o_writer)
                .stderr(e_writer)
                .spawn();

            let mut child = match child {
                Ok(child) => child,
                // The child couldn't spawn, nothing left to do but note the failure and return.
                Err(err) => {
                    tx.send(ProcessResult {
                        exit: ProcessExitResult::IOError(err),
                        cwd,
                    })
                    .unwrap();
                    return;
                }
            };

            // We're spawning threads to process stdout/stderr from our commands. Track them to
            // join.
            let mut io_threads = Vec::new();

            // This feels stupid, but I can't figure out how to pass the actual stdout / stderr to
            // the same function -- using a Box<dyn Write> works but we lose the ability to lock
            // it.
            for (target, reader) in vec![
                (StdIOTarget::Stdout, o_reader),
                (StdIOTarget::Stderr, e_reader),
            ] {
                // Clone since we're moving into a thread closure ...
                let cwd = cwd.clone();
                io_threads.push(thread::spawn(move || {
                    stream_output(&target, reader, &cwd);
                }));
            }
            // {
            //     let cwd = cwd.clone();
            //     io_threads.push(thread::spawn(move || {
            //         stream_output(io::stdout(), o_reader, cwd);
            //     }));
            // }
            // {
            //     let cwd = cwd.clone();
            //     io_threads.push(thread::spawn(move || {
            //         stream_output(io::stderr(), e_reader, cwd);
            //     }));
            // }

            // Wait for the child to finish ...
            let result: ProcessExitResult = child.wait().into();

            // Drop the child since it owns the write side of our pipes, and it needs to be dropped
            // to close them so our io threads can get an EOF. This is what the docs say to do so
            // I'm including it to be complete, but in practice, I've still never seen the EOF
            // happen.
            drop(child);

            // Join our io threads.
            for t in io_threads {
                // We unwrap because frankly, I don't know what to do if one of our threads panics,
                // so we may as well panic too.
                t.join().unwrap();
            }

            tx.send(ProcessResult { exit: result, cwd }).unwrap();
        }));
    }

    // Join our cwd threads.
    for t in cmd_threads {
        // We unwrap because frankly, I don't know what to do if one of our threads panics, so we
        // may as well panic too.
        t.join().unwrap();
        results.push(rx.recv().unwrap());
    }

    let mut e_code = 0;
    let stderr = io::stderr();

    for result in results {
        // Handle the results.
        match result.exit {
            ProcessExitResult::Code(0) => {}
            ProcessExitResult::Code(code) => {
                write_with_prefix(
                    stderr.lock(),
                    &result.cwd,
                    format!("exited {:}\n", code).as_bytes(),
                );
                e_code = 1;
            }
            ProcessExitResult::Signal(signal) => {
                write_with_prefix(
                    stderr.lock(),
                    &result.cwd,
                    format!("signaled {:}\n", signal).as_bytes(),
                );
                e_code = 1;
            }
            ProcessExitResult::IOError(err) => {
                write_with_prefix(stderr.lock(), &result.cwd, format!("{:}\n", err).as_bytes());
                e_code = 1;
            }
        };
    }

    exit(e_code);
}
