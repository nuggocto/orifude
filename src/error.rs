use std::error::Error;
use std::fmt;
use std::io;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

impl fmt::Display for OutputStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdout => formatter.write_str("standard output"),
            Self::Stderr => formatter.write_str("standard error"),
        }
    }
}

#[derive(Debug)]
pub struct OutputError {
    stream: OutputStream,
    source: io::Error,
}

impl OutputError {
    pub(crate) const fn new(stream: OutputStream, source: io::Error) -> Self {
        Self { stream, source }
    }

    #[must_use]
    pub const fn stream(&self) -> OutputStream {
        self.stream
    }

    #[must_use]
    pub const fn source_error(&self) -> &io::Error {
        &self.source
    }
}

impl fmt::Display for OutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "could not write to {}", self.stream)
    }
}

impl Error for OutputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}
