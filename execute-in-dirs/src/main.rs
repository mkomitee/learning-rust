use os_pipe::{pipe, PipeReader};
use std::borrow::ToOwned;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;
use std::process::{exit, Command, ExitStatus};
use std::result::Result;
use std::sync::mpsc::{channel, Receiver, Sender};
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

enum ProcessResult {
    IOError(io::Error),
    Code(i32),
    Signal(i32),
}

impl From<Result<ExitStatus, io::Error>> for ProcessResult {
    fn from(result: Result<ExitStatus, io::Error>) -> Self {
        match result {
            Err(err) => ProcessResult::IOError(err),
            Ok(estatus) => {
                if let Some(code) = estatus.code() {
                    ProcessResult::Code(code)
                } else {
                    // We expect here because we know that the command executed (no Err), that it
                    // finished (or there would be no ExitStatus) and that it didn't exit with an
                    // exit code (estatus.code() -> None). While the compiler can't prove it,
                    // there's no other possibility.
                    ProcessResult::Signal(
                        estatus
                            .signal()
                            .expect("process exited with no exit code or signal!"),
                    )
                }
            }
        }
    }
}

#[derive(Clone)]
enum StdIOTarget {
    Stdout,
    Stderr,
}

fn stream_output(
    target: StdIOTarget,
    reader: PipeReader,
    prefix: OsString,
    tx: Sender<(StdIOTarget, OsString, Vec<u8>)>,
) {
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
                tx.send((target.clone(), prefix.clone(), buf.clone()))
                    .unwrap();
                buf.clear();
            }
        }
    }
}

fn print_error(tx: Sender<(StdIOTarget, OsString, Vec<u8>)>, prefix: OsString, message: String) {
    tx.send((StdIOTarget::Stderr, prefix, message.as_bytes().to_vec()))
        .unwrap();
}

fn handle_io(rx: Receiver<(StdIOTarget, OsString, Vec<u8>)>) {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let stderr = io::stderr();
    let mut stderr = stderr.lock();
    loop {
        match rx.recv() {
            Ok((StdIOTarget::Stdout, prefix, message)) => {
                for token in &[prefix.as_bytes(), b": ", &message] {
                    let _ = stdout.write(token);
                }
            }
            Ok((StdIOTarget::Stderr, prefix, message)) => {
                for token in &[prefix.as_bytes(), b": ", &message] {
                    let _ = stderr.write(token);
                }
            }
            // No live senders, we're done!
            Err(_) => break,
        }
    }
}

fn main() {
    // Thank you structopt.
    let opt = Opt::from_args();

    // We need to split argv0 from the rest for Command ...
    let exec = opt.arg[0].to_owned();
    let args: Vec<OsString> = opt.arg.iter().skip(1).map(ToOwned::to_owned).collect();

    // Start up our IO handling thread.
    let (tx_io, rx_io) = channel::<(StdIOTarget, OsString, Vec<u8>)>();
    let io_thread = thread::spawn(move || {
        handle_io(rx_io);
    });

    // We limit concurrency with a semaphore ...
    let semaphore = Arc::new(Semaphore::new(opt.concurrency));

    // Track our threads so we can ensure they complete ...
    let mut cwd_threads = Vec::new();

    // TODO: Don't print out resulting signal / exit code / error until end.
    // TODO: Exit with appropriate exit code
    for cwd in opt.directory {
        let exec = exec.clone();
        let args = args.clone();
        let semaphore = semaphore.clone();
        let tx_io = tx_io.clone();
        cwd_threads.push(thread::spawn(move || {
            // let mut e_code = 0;

            // Acquire our guard to limit concurrency
            let _guard = semaphore.access();

            // Setup our pipes for the command
            let (o_reader, o_writer) = match pipe() {
                Ok((o_reader, o_writer)) => (o_reader, o_writer),
                // Couldn't create our pipes. I suspect a ulimit issue, but there's nothing we can
                // do but note the failure and return.
                Err(err) => {
                    print_error(tx_io, cwd, format!("{:}\n", err));
                    // e_code = 1;
                    return;
                }
            };
            let (e_reader, e_writer) = match pipe() {
                Ok((e_reader, e_writer)) => (e_reader, e_writer),
                // Couldn't create our pipes. I suspect a ulimit issue, but there's nothing we can
                // do but note the failure and return.
                Err(err) => {
                    print_error(tx_io, cwd, format!("{:}\n", err));
                    // e_code = 1;
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
                    print_error(tx_io, cwd, format!("{:}\n", err));
                    // e_code = 1;
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
                let tx = tx_io.clone();
                io_threads.push(thread::spawn(move || {
                    stream_output(target, reader, cwd, tx);
                }));
            }

            // Wait for the child to finish ...
            let result: ProcessResult = child.wait().into();

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

            // Handle the results.
            match result {
                ProcessResult::Code(0) => {}
                ProcessResult::Code(code) => {
                    print_error(tx_io, cwd, format!("exited {:}\n", code));
                    // e_code = 1;
                }
                ProcessResult::Signal(signal) => {
                    print_error(tx_io, cwd, format!("signaled {:}\n", signal));
                    // e_code = 1;
                }
                ProcessResult::IOError(err) => {
                    print_error(tx_io, cwd, format!("{:}\n", err));
                    // e_code = 1;
                }
            };
        }));
    }

    // Close our io thread sender so it can finish up ...
    drop(tx_io);

    // Join our cwd threads.
    for t in cwd_threads {
        // We unwrap because frankly, I don't know what to do if one of our threads panics, so we
        // may as well panic too.
        t.join().unwrap();
    }

    io_thread.join().unwrap();

    let e_code = 0;
    exit(e_code);
}
