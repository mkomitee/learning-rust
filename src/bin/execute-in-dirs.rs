use os_pipe::{pipe, PipeReader};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;
use std::process::{exit, Command, ExitStatus};
use std::result::Result;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;
use std_semaphore::Semaphore;
use structopt;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "execute-in-dirs",
    about = "    Execute the same command in multiple directories.",
    author = "\n"
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
    Panic,
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
enum IOHandle {
    Output,
    Error,
}

fn stream_output(target: &IOHandle, reader: PipeReader, prefix: &OsStr) {
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
            Ok(_) => {
                match target {
                    IOHandle::Output => write_with_prefix(stdout.lock(), prefix, &buf),
                    IOHandle::Error => write_with_prefix(stderr.lock(), prefix, &buf),
                }
                buf.clear();
            }
        }
    }
}

fn trim_end(s: &[u8], v: u8) -> &[u8] {
    let end_idx = s.len() - s.iter().rev().take_while(|&&x| x == v).count();
    &s[..end_idx]
}

fn write_with_prefix<T: Write>(mut handle: T, prefix: &OsStr, message: &[u8]) {
    // for token in &[trim_end(prefix.as_bytes(), 47u8), b": ", message] {
    for token in &[trim_end(prefix.as_bytes(), '/' as u8), b": ", message] {
        // If we can't write (e.g. the reader has closed it's end of the pipe) ignore the error and
        // continue to run.
        let _ = handle.write(token);
    }
}

fn execute_command(
    cwd: OsString,
    exec: OsString,
    args: Vec<OsString>,
    semaphore: Arc<Semaphore>,
    tx: Sender<ProcessResult>,
) {
    // Acquire our guard to limit concurrency
    let _guard = semaphore.access();

    // Setup our pipes for the command
    let (o_reader, o_writer) = match pipe() {
        Ok((o_reader, o_writer)) => (o_reader, o_writer),
        // Couldn't create our pipes. I suspect a ulimit issue, but there's nothing we can do but
        // note the failure and return.
        Err(err) => {
            tx.send(ProcessResult {
                exit: ProcessExitResult::IOError(err),
                cwd,
            })
            // We expect because we know the receiver has not been dropped, and that's the only
            // thing that could cause an error.
            .expect("result rx expectedly dropped");
            return;
        }
    };
    let (e_reader, e_writer) = match pipe() {
        Ok((e_reader, e_writer)) => (e_reader, e_writer),
        // Couldn't create our pipes. I suspect a ulimit issue, but there's nothing we can do but
        // note the failure and return.
        Err(err) => {
            tx.send(ProcessResult {
                exit: ProcessExitResult::IOError(err),
                cwd,
            })
            // We expect because we know the receiver has not been dropped, and that's the only
            // thing that could cause an error.
            .expect("result rx unexpectedly dropped");
            return;
        }
    };

    // Spawn our command
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
            // We expect because we know the receiver has not been dropped, and that's the only
            // thing that could cause an error.
            .expect("result rx unexpectedly dropped");
            return;
        }
    };

    // We're spawning threads to process stdout/stderr from our commands. Track them to join.
    let mut io_threads = Vec::new();

    // This feels silly, but apparently we can't write a simple generic function which can take
    // both io::stdout & io::stderr without losing the ability to lock them without generic
    // associated types because io::Std{out,err}.lock()'s are borrows.
    for (target, reader) in vec![(IOHandle::Output, o_reader), (IOHandle::Error, e_reader)] {
        // Clone since we're moving into a thread closure
        let cwd = cwd.clone();
        io_threads.push(thread::spawn(move || {
            stream_output(&target, reader, &cwd);
        }));
    }

    // Wait for the child to finish
    let result: ProcessExitResult = child.wait().into();

    // Drop the child since it owns the write side of our pipes, and it needs to be dropped to
    // close them so our io threads can get an EOF. This is what the docs say to do so I'm
    // including it to be complete, but in practice, I've still never seen the EOF happen.
    drop(child);

    // Join our io threads so that we block until all of our commands output has been handled.
    for thread in io_threads {
        thread.join().expect("io thread paniced");
    }

    tx.send(ProcessResult { exit: result, cwd })
        // We expect because we know the receiver has not been dropped, and that's the only thing
        // that could cause an error.
        .expect("result rx unexpectedly dropped");
}

fn process_results(results: Vec<ProcessResult>) -> i32 {
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
            ProcessExitResult::Panic => {
                write_with_prefix(stderr.lock(), &result.cwd, b"paniced");
            }
        };
    }
    e_code
}

fn main() {
    // Thank you structopt.
    let mut opt = Opt::from_args();

    // We need to split argv0 from the rest for Command
    let exec = opt.arg.remove(0);
    let args = opt.arg;

    // Processing our results at the end
    let mut results = Vec::new();
    let (tx, rx) = channel();

    // We limit concurrency with a semaphore
    let semaphore = Arc::new(Semaphore::new(opt.concurrency));

    // Track our threads so we can ensure they complete
    let mut cmd_threads = HashMap::new();

    // Launch the command threads
    for cwd in opt.directory {
        let exec = exec.clone();
        let args = args.clone();
        let semaphore = semaphore.clone();
        let tx = tx.clone();
        cmd_threads.insert(
            cwd.clone(),
            thread::spawn(move || {
                execute_command(cwd, exec, args, semaphore, tx);
            }),
        );
    }

    // Join our cmd threads.
    for (cwd, thread) in cmd_threads {
        if let Ok(()) = thread.join() {
            results.push(
                rx.recv()
                    // We expect because we know that we'll have at least one result per cmd thread
                    // we were able to join.
                    .expect("all tx threads dropped with buffered message"),
            );
        } else {
            results.push(ProcessResult {
                exit: ProcessExitResult::Panic,
                cwd,
            })
        }
    }

    exit(process_results(results));
}
