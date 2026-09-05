//! The application boundary for Orifude.
//!
//! Command-line outcomes distinguish an interactive launch from stable process
//! codes for successful output, operational failures, and invalid usage.
//!
//! ```
//! use orifude::ExitStatus;
//!
//! assert_eq!(ExitStatus::Success.code(), 0);
//! assert_eq!(ExitStatus::Failure.code(), 1);
//! assert_eq!(ExitStatus::Usage.code(), 2);
//! ```

mod author;
mod cli;
mod content;
pub mod domain;
mod error;
pub mod generator;
pub mod packs;
pub mod solver;
pub mod storage;
mod tui;

pub use author::{AuthorError, execute_author};
pub use cli::{CommandOutcome, ExitStatus, run};
pub use error::{OutputError, OutputStream};
pub use tui::{EventError, TuiError, play};
