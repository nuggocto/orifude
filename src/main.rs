use std::env;
use std::error::Error;
use std::fmt;
use std::io::{self, Write};
use std::process::ExitCode;

use orifude::{CommandOutcome, ExitStatus, execute_author, play, run};

const MAX_ERROR_CAUSES: usize = 8;
const MAX_ERROR_REPORT_BYTES: usize = 16 * 1024;
const TRUNCATED_ERROR_SUFFIX: &str = " [truncated]";

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
        Ok(
            command @ (CommandOutcome::Verify(_)
            | CommandOutcome::Solve(_)
            | CommandOutcome::PackInstall(_)
            | CommandOutcome::PackList
            | CommandOutcome::PackRemove(_)),
        ) => {
            let stdout = io::stdout();
            let stderr = io::stderr();
            match execute_author(command, &mut stdout.lock(), &mut stderr.lock()) {
                Ok(status) => status.into(),
                Err(error) => report_failure(&error),
            }
        }
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
    let mut report = SafeErrorReport::new();
    let mut complete = fmt::write(&mut report, format_args!("error: {error}")).is_ok();

    let mut cause = error.source();
    for _ in 0..MAX_ERROR_CAUSES {
        if !complete {
            break;
        }
        let Some(current) = cause else {
            break;
        };

        complete = fmt::write(&mut report, format_args!(": {current}")).is_ok();
        cause = current.source();
    }

    if complete && cause.is_some() {
        let _written = fmt::write(&mut report, format_args!(": additional causes omitted"));
    }

    stream.write_all(report.finish().as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()
}

struct SafeErrorReport {
    text: String,
    truncated: bool,
}

impl SafeErrorReport {
    fn new() -> Self {
        Self {
            text: String::with_capacity(MAX_ERROR_REPORT_BYTES),
            truncated: false,
        }
    }

    fn finish(mut self) -> String {
        if self.truncated {
            self.text.push_str(TRUNCATED_ERROR_SUFFIX);
        }
        self.text
    }
}

impl fmt::Write for SafeErrorReport {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let payload_limit = MAX_ERROR_REPORT_BYTES - TRUNCATED_ERROR_SUFFIX.len();
        for character in text.chars() {
            let safe = if character.is_control() {
                '?'
            } else {
                character
            };
            if self.text.len().saturating_add(safe.len_utf8()) > payload_limit {
                self.truncated = true;
                return Err(fmt::Error);
            }
            self.text.push(safe);
        }
        Ok(())
    }
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

    struct HostileError;

    impl fmt::Display for HostileError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("hostile\u{1b}[31m\n")?;
            for _ in 0..MAX_ERROR_REPORT_BYTES {
                formatter.write_str("long")?;
            }
            Ok(())
        }
    }

    impl fmt::Debug for HostileError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("HostileError")
        }
    }

    impl Error for HostileError {}

    #[test]
    fn error_report_bounds_and_sanitizes_external_causes() {
        let mut report = Vec::new();

        write_error_report(&mut report, &HostileError).expect("the report should be writable");

        assert!(report.len() <= MAX_ERROR_REPORT_BYTES + 1);
        assert!(report.ends_with(b" [truncated]\n"));
        assert!(
            report[..report.len() - 1]
                .iter()
                .all(|byte| !byte.is_ascii_control())
        );
    }
}
