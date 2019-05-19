use std::borrow::Borrow;
use std::ffi::OsString;
use std::io;
use std::io::BufReader;
use std::io::Read;
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
    /// Directories in wich to execute command
    #[structopt(parse(from_os_str), raw(required = "true"))]
    directory: Vec<PathBuf>,
    /// Command to execute
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
                    // finished (or we'd have no ExitStatus) and that it didn't exit with an exit
                    // code (estatus.code() -> None). There's no other possibility.
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
                .stderr(Stdio::piped())
                .spawn();
            let cwd = cwd.to_string_lossy();
            let cwd = cwd.trim_end_matches('/');
            match child {
                Err(err) => {
                    eprintln!("{:}: {:}", cwd, err);
                    ecode = 1;
                    continue;
                }

                Ok(mut child) => {
                    // TODO: stream output to stdout/stderr, but prefixed with the cwd.
                    let _stdout = BufReader::new(child.stdout.expect("stdout missing?"));
                    let _stderr = BufReader::new(child.stderr.expect("stderr missing?"));
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
            };
        }
    }
    exit(ecode);
}
