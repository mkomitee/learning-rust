use os_pipe::{pipe, PipeReader};
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;
use std::process::{exit, Command, ExitStatus};
use std::result::Result;
use std::thread;
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

enum StdIOTarget {
    Stdout,
    Stderr,
}

fn stream_output(target: &StdIOTarget, reader: PipeReader, prefix: &[u8]) {
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
                // I can't figure out how to get this to be more generic over both stdout & stderr,
                // so instead we have code duplication.
                match target {
                    StdIOTarget::Stdout => {
                        // We're printing a full line, so we need to acquire a lock for the
                        // duration ...
                        let stdout = io::stdout();
                        let mut stdout = stdout.lock();
                        for token in &[prefix, b": ", &buf] {
                            if stdout.write(token).is_err() {
                                return;
                            }
                        }
                    }
                    StdIOTarget::Stderr => {
                        // We're printing a full line, so we need to acquire a lock for the
                        // duration ...
                        let stderr = io::stderr();
                        let mut stderr = stderr.lock();
                        for token in &[prefix, b": ", &buf] {
                            if stderr.write(token).is_err() {
                                return;
                            }
                        }
                    }
                }
                buf.clear();
            }
        }
    }
}

fn print_error(prefix: &[u8], message: String) {
    let stderr = io::stderr();
    let mut stderr = stderr.lock();
    for token in &[prefix, b": ", &message.into_bytes()] {
        if stderr.write(token).is_err() {
            // Well, we can't print to stderr, so there's not much else we can do here short of
            // crashing the program --- maybe we should do that instead of ignoring this?
            return;
        }
    }
}

fn main() {
    let opt = Opt::from_args();
    let mut e_code = 0;

    // We expect here because we know structopt will ensure that opt.arg has at least one element,
    // so we won't have a None.
    let (exec, args) = opt.arg.split_first().expect("structopt lied?");

    // TODO: run the commands concurrently with a max concurrency using threads.
    for cwd in opt.directory {
        // Setup our pipes for the command ...
        let (o_reader, o_writer) = match pipe() {
            Ok((o_reader, o_writer)) => (o_reader, o_writer),
            Err(err) => {
                print_error(cwd.as_bytes(), format!("{:}\n", err));
                e_code = 1;
                continue;
            }
        };
        let (e_reader, e_writer) = match pipe() {
            Ok((e_reader, e_writer)) => (e_reader, e_writer),
            Err(err) => {
                print_error(cwd.as_bytes(), format!("{:}\n", err));
                e_code = 1;
                continue;
            }
        };

        // Spawn our command ...
        let child = Command::new(exec)
            .args(args)
            .current_dir(&cwd)
            .stdout(o_writer)
            .stderr(e_writer)
            .spawn();

        // We're going to write directly to stdout/stderr instead of using println or eprintln
        // because we want to prefix e very line with the cwd we're using and the cwd's are
        // OsStrings because they're not limited to utf8 characters. As such, we need bytes for
        // writing.
        let cwd = cwd.as_bytes().to_owned();

        let mut child = match child {
            Ok(child) => child,
            Err(err) => {
                print_error(&cwd, format!("{:}\n", err));
                e_code = 1;
                continue;
            }
        };

        // We're spawning threads to process stdout/stderr from our commands. Track them to join.
        let mut io_threads = Vec::new();
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

        // Wait for the child to finish ...
        let result: ProcessResult = child.wait().into();

        for t in io_threads {
            // We unwrap because frankly, I don't know what to do if one of our threads panics, so
            // we may as well panic too.
            //
            t.join().unwrap();
        }

        match result {
            ProcessResult::Code(0) => {}
            ProcessResult::Code(code) => {
                print_error(&cwd, format!("exited {:}\n", code));
                e_code = 1;
            }
            ProcessResult::Signal(signal) => {
                print_error(&cwd, format!("signaled {:}\n", signal));
                e_code = 1;
            }
            ProcessResult::IOError(err) => {
                print_error(&cwd, format!("{:}\n", err));
                e_code = 1;
            }
        };
    }
    exit(e_code);
}
