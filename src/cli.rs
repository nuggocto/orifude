use std::ffi::OsString;
use std::io::Write;
use std::process::ExitCode;

use crate::{OutputError, OutputStream};

const HELP: &str = concat!(
    "Orifude is a quiet, offline puzzle game for the terminal.\n\n",
    "Usage: orifude [OPTIONS]\n\n",
    "Options:\n",
    "  -h, --help     Print help\n",
    "  -V, --version  Print version\n",
);
const USAGE_ERROR: &str = concat!(
    "error: unsupported command-line arguments\n\n",
    "Usage: orifude [OPTIONS]\n",
    "For more information, try '--help'.\n",
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ExitStatus {
    Success = 0,
    Failure = 1,
    Usage = 2,
}

impl ExitStatus {
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        Self::from(status.code())
    }
}

enum Command {
    Play,
    Help,
    Version,
    Invalid,
}

fn parse(mut arguments: impl Iterator<Item = OsString>) -> Command {
    let Some(argument) = arguments.next() else {
        return Command::Play;
    };

    if arguments.next().is_some() {
        return Command::Invalid;
    }

    if argument == "-h" || argument == "--help" {
        Command::Help
    } else if argument == "-V" || argument == "--version" {
        Command::Version
    } else {
        Command::Invalid
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// The command-line parser either hands control to the TUI or finishes with a
/// process status after writing a bounded non-interactive response.
pub enum CommandOutcome {
    /// Open the interactive terminal application after releasing stream locks.
    Play,
    /// Finish without opening the terminal application.
    Exit(ExitStatus),
}

/// Runs the command-line boundary with caller-provided streams.
///
/// Arguments are inspected without being collected or reflected in output.
/// This keeps command parsing bounded and terminal-safe.
///
/// # Errors
///
/// Returns [`OutputError`] if writing the selected response fails. An empty
/// argument list returns [`CommandOutcome::Play`] without touching either
/// stream, allowing the binary to release stream locks before opening its TUI.
pub fn run(
    arguments: impl Iterator<Item = OsString>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<CommandOutcome, OutputError> {
    match parse(arguments) {
        Command::Play => Ok(CommandOutcome::Play),
        Command::Help => write(stdout, OutputStream::Stdout, HELP, ExitStatus::Success),
        Command::Version => {
            writeln!(stdout, "orifude {}", env!("CARGO_PKG_VERSION"))
                .map_err(|source| OutputError::new(OutputStream::Stdout, source))?;
            flush(stdout, OutputStream::Stdout)?;
            Ok(CommandOutcome::Exit(ExitStatus::Success))
        }
        Command::Invalid => write(stderr, OutputStream::Stderr, USAGE_ERROR, ExitStatus::Usage),
    }
}

fn write(
    stream: &mut impl Write,
    output_stream: OutputStream,
    content: &str,
    status: ExitStatus,
) -> Result<CommandOutcome, OutputError> {
    stream
        .write_all(content.as_bytes())
        .map_err(|source| OutputError::new(output_stream, source))?;
    flush(stream, output_stream)?;
    Ok(CommandOutcome::Exit(status))
}

fn flush(stream: &mut impl Write, output_stream: OutputStream) -> Result<(), OutputError> {
    stream
        .flush()
        .map_err(|source| OutputError::new(output_stream, source))
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::io::{self, Write};

    use super::*;

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct FailingFlushWriter;

    impl Write for FailingFlushWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
    }

    #[test]
    fn output_failures_are_returned_without_panicking() {
        let error = run(
            [OsString::from("--help")].into_iter(),
            &mut FailingWriter,
            &mut io::sink(),
        )
        .expect_err("a broken output stream should fail");

        assert_eq!(error.stream(), OutputStream::Stdout);
        let source = error
            .source()
            .and_then(|cause| cause.downcast_ref::<io::Error>())
            .expect("the source should be an I/O error");
        assert_eq!(source.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn flush_failures_are_returned_without_panicking() {
        let error = run(
            [OsString::from("--help")].into_iter(),
            &mut FailingFlushWriter,
            &mut io::sink(),
        )
        .expect_err("a failed flush should fail");

        assert_eq!(error.stream(), OutputStream::Stdout);
        let source = error
            .source()
            .and_then(|cause| cause.downcast_ref::<io::Error>())
            .expect("the source should be an I/O error");
        assert_eq!(source.kind(), io::ErrorKind::BrokenPipe);
    }
}
