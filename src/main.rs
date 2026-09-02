use std::env;
use std::error::Error;
use std::io::{self, Write};
use std::process::ExitCode;

use orifude::{CommandOutcome, ExitStatus, play, run};

const MAX_ERROR_CAUSES: usize = 8;

fn main() -> ExitCode {
    let command = {
        let stdout = io::stdout();
        let stderr = io::stderr();
        let mut stdout = stdout.lock();
        let mut stderr = stderr.lock();
        run(env::args_os().skip(1), &mut stdout, &mut stderr)
    };
    match command {
        Ok(CommandOutcome::Play) => match play() {
            Ok(()) => ExitStatus::Success.into(),
            Err(error) => report_failure(&error),
        },
        Ok(CommandOutcome::Exit(status)) => status.into(),
        Err(error) => report_failure(&error),
    }
}

fn report_failure(error: &(dyn Error + 'static)) -> ExitCode {
    let stderr = io::stderr();
    let mut stderr = stderr.lock();
    let _ignored = write_error_report(&mut stderr, error);
    ExitStatus::Failure.into()
}

fn write_error_report(stream: &mut impl Write, error: &(dyn Error + 'static)) -> io::Result<()> {
    write!(stream, "error: {error}")?;

    let mut cause = error.source();
    for _ in 0..MAX_ERROR_CAUSES {
        let Some(current) = cause else {
            return writeln!(stream);
        };

        write!(stream, ": {current}")?;
        cause = current.source();
    }

    if cause.is_some() {
        write!(stream, ": additional causes omitted")?;
    }

    writeln!(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FullDisk;

    impl Write for FullDisk {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("disk is full"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn error_report_includes_the_operational_cause() {
        let error = run(
            [std::ffi::OsString::from("--help")].into_iter(),
            &mut FullDisk,
            &mut io::sink(),
        )
        .expect_err("a full disk should fail");
        let mut report = Vec::new();

        write_error_report(&mut report, &error).expect("the report should be writable");

        assert_eq!(
            String::from_utf8(report).expect("the report should be UTF-8"),
            "error: could not write to standard output: disk is full\n"
        );
    }
}
