use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

use orifude::{ExitStatus, run};

fn main() -> ExitCode {
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();

    match run(env::args_os().skip(1), &mut stdout, &mut stderr) {
        Ok(status) => status.into(),
        Err(error) => {
            let _ignored = writeln!(stderr, "error: {error}");
            ExitStatus::Failure.into()
        }
    }
}
