use std::ffi::OsString;
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{exit, Command, ExitStatus, Stdio};
use std::result::Result;
use structopt;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "execute-in-dirs",
    about = "Execute the same command in multiple directories."
)]
struct Opt {
    /// Directories in which to execute command
    // PathBuf because filenames are not guaranteed to be utf8.
    #[structopt(parse(from_os_str), raw(required = "true"))]
    directory: Vec<PathBuf>,
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
                    // I expect here because I know that the command executed (no Err), that it
                    // finished (or we'd have no ExitStatus) and that it didn't exit with an exit
                    // code (estatus.code() -> None). While the compiler can't prove it, there's no
                    // other possibility.
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

fn main() {
    let opt = Opt::from_args();
    let mut ecode = 0;
    if let Some((exec, args)) = opt.arg.split_first() {
        // TODO: run the commands concurrently with a max concurrency using threads.
        for cwd in opt.directory {
            let child = Command::new(exec)
                .args(args)
                .current_dir(&cwd)
                .stdout(Stdio::piped())
                // .stderr(Stdio::piped()) // TODO: handle stderr too, once I figure out how ...
                .spawn();

            // For now, I'm using eprintln! to print these, and PathBuf's don't implement Display,
            // so I convert them to String's for the sole purpose of display. At some point I may
            // chose to drop this and write bytes directly to my stderr, but this is probably good
            // enough.
            let cwd = cwd.to_string_lossy();
            let cwd = cwd.trim_end_matches('/');

            if let Err(err) = child {
                eprintln!("{:}: {:}", cwd, err);
                ecode = 1;
                continue;
            }

            // I expect here because I've already verified child.is_ok() with the previous if let,
            // and if it was err, we'd have continued our loop. I chose to do it this way to avoid
            // another level of indentation.
            let mut child = child.unwrap();

            // XXX: I want to stream stdout/stderr to the console on the appropriate file
            // descriptors, and I know I'm going to have to use epoll directly, figure out
            // async/futures, or potentially use threads, but the first thing I need to do is
            // figure out how to get access to the Child's stdout pipe.
            //
            // I wonder if I'll need to use something like https://docs.rs/os_pipe/0.8.1/os_pipe/
            // to create my own pipes and hand them to the Command instead of using Stdio::piped()
            // which causes me to have to access the pipe by partially moving out from the Child.
            //
            // In any event, I can't figure out how to effectively do what I'm trying to do in the
            // next line without partially moving Child when accessing/unwrapping child.stdout,
            // which makes it so I cannot call wait() on the child later-on.
            // The error I get when I compile with the following line is:
            //
            // error[E0382]: borrow of moved value: `child`
            //    --> src/main.rs:90:41
            //     |
            // 110 |             let _stdout = child.stdout.expect("stdout missing?");
            //     |                           ------------ value moved here
            // 111 |
            // 112 |             let result: ProcessResult = child.wait().into();
            //     |                                         ^^^^^ value borrowed here after partial move
            //     |
            //     = note: move occurs because `child.stdout` has type `std::option::Option<std::process::ChildStdout>`, which does not implement the `Copy` trait
            let stdout = child.stdout.expect("stdout missing?");

            let result: ProcessResult = child.wait().into();
            match result {
                ProcessResult::Code(0) => eprintln!("{:}: ok", cwd),
                ProcessResult::Code(code) => {
                    eprintln!("{:}: exited {:}", cwd, code);
                    ecode = 1;
                }
                ProcessResult::Signal(signal) => {
                    eprintln!("{:}: signaled {:}", cwd, signal);
                    ecode = 1;
                }
                ProcessResult::IOError(err) => {
                    eprintln!("{:}: {:}", cwd, err);
                    ecode = 1;
                }
            };
        }
    }
    exit(ecode);
}
