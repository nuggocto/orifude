//! The application boundary for Orifude.
//!
//! Command-line outcomes have stable process codes so callers can distinguish
//! successful output, operational failures, and invalid usage.
//!
//! ```
//! use orifude::ExitStatus;
//!
//! assert_eq!(ExitStatus::Success.code(), 0);
//! assert_eq!(ExitStatus::Failure.code(), 1);
//! assert_eq!(ExitStatus::Usage.code(), 2);
//! ```

mod cli;
pub mod domain;
mod error;

pub use cli::{ExitStatus, run};
pub use error::{OutputError, OutputStream};
