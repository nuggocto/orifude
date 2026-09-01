//! Bounded deterministic action replays.

use std::error::Error;
use std::fmt;

use crate::domain::attempt::{ActionError, Attempt};
use crate::domain::paper::{MAX_ACTIONS, PaperAction};
use crate::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleRevision};

pub const ENGINE_COMPATIBILITY_VERSION: u16 = 1;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ReplayMetadata {
    puzzle: PuzzleIdentity,
    puzzle_revision: PuzzleRevision,
    puzzle_format_version: u16,
    engine_compatibility_version: u16,
}

impl ReplayMetadata {
    #[must_use]
    pub const fn new(
        puzzle: PuzzleIdentity,
        puzzle_revision: PuzzleRevision,
        puzzle_format_version: u16,
        engine_compatibility_version: u16,
    ) -> Self {
        Self {
            puzzle,
            puzzle_revision,
            puzzle_format_version,
            engine_compatibility_version,
        }
    }

    #[must_use]
    pub fn current(puzzle: &Puzzle) -> Self {
        Self::new(
            puzzle.identity().clone(),
            puzzle.revision(),
            puzzle.format_version(),
            ENGINE_COMPATIBILITY_VERSION,
        )
    }

    #[must_use]
    pub const fn puzzle(&self) -> &PuzzleIdentity {
        &self.puzzle
    }

    #[must_use]
    pub const fn puzzle_revision(&self) -> &PuzzleRevision {
        &self.puzzle_revision
    }

    #[must_use]
    pub const fn puzzle_format_version(&self) -> u16 {
        self.puzzle_format_version
    }

    #[must_use]
    pub const fn engine_compatibility_version(&self) -> u16 {
        self.engine_compatibility_version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Replay {
    metadata: ReplayMetadata,
    actions: Box<[PaperAction]>,
}

impl Replay {
    /// Constructs a replay after enforcing its action-count bound.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayError::TooManyActions`] before retaining an oversized
    /// action list.
    pub fn new(metadata: ReplayMetadata, actions: Vec<PaperAction>) -> Result<Self, ReplayError> {
        if actions.len() > usize::from(MAX_ACTIONS) {
            return Err(ReplayError::TooManyActions {
                count: actions.len(),
                limit: MAX_ACTIONS,
            });
        }
        Ok(Self {
            metadata,
            actions: actions.into_boxed_slice(),
        })
    }

    #[must_use]
    pub fn from_attempt(attempt: &Attempt) -> Self {
        Self {
            metadata: ReplayMetadata::current(attempt.puzzle()),
            actions: attempt.actions().collect(),
        }
    }

    #[must_use]
    pub const fn metadata(&self) -> &ReplayMetadata {
        &self.metadata
    }

    #[must_use]
    pub fn actions(&self) -> &[PaperAction] {
        &self.actions
    }

    /// Validates compatibility, then executes into a fresh isolated attempt.
    ///
    /// # Errors
    ///
    /// Returns a compatibility or indexed action error. A failure cannot
    /// mutate another live attempt because replay work owns its fresh state.
    pub fn execute(&self, puzzle: &Puzzle) -> Result<Attempt, ReplayError> {
        self.validate_for(puzzle)?;
        let mut attempt = puzzle.start();
        for (index, &action) in self.actions.iter().enumerate() {
            attempt
                .apply(action)
                .map_err(|source| ReplayError::Action { index, source })?;
        }
        Ok(attempt)
    }

    fn validate_for(&self, puzzle: &Puzzle) -> Result<(), ReplayError> {
        if self.metadata.engine_compatibility_version != ENGINE_COMPATIBILITY_VERSION {
            return Err(ReplayError::IncompatibleEngine {
                found: self.metadata.engine_compatibility_version,
                supported: ENGINE_COMPATIBILITY_VERSION,
            });
        }
        if self.metadata.puzzle_format_version != puzzle.format_version() {
            return Err(ReplayError::IncompatiblePuzzleFormat {
                found: self.metadata.puzzle_format_version,
                expected: puzzle.format_version(),
            });
        }
        if self.metadata.puzzle != *puzzle.identity() {
            return Err(ReplayError::PuzzleIdentityMismatch);
        }
        if !self.metadata.puzzle_revision.matches(puzzle) {
            return Err(ReplayError::PuzzleRevisionMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReplayError {
    TooManyActions { count: usize, limit: u8 },
    IncompatibleEngine { found: u16, supported: u16 },
    IncompatiblePuzzleFormat { found: u16, expected: u16 },
    PuzzleIdentityMismatch,
    PuzzleRevisionMismatch,
    Action { index: usize, source: ActionError },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyActions { count, limit } => {
                write!(
                    formatter,
                    "replay has {count} actions, above the {limit}-action limit"
                )
            }
            Self::IncompatibleEngine { found, supported } => write!(
                formatter,
                "replay engine compatibility {found} does not match {supported}"
            ),
            Self::IncompatiblePuzzleFormat { found, expected } => write!(
                formatter,
                "replay puzzle format {found} does not match {expected}"
            ),
            Self::PuzzleIdentityMismatch => formatter.write_str("replay belongs to another puzzle"),
            Self::PuzzleRevisionMismatch => {
                formatter.write_str("replay belongs to another revision of this puzzle")
            }
            Self::Action { index, source } => {
                write!(formatter, "replay action {index} failed: {source}")
            }
        }
    }
}

impl Error for ReplayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Action { source, .. } => Some(source),
            _ => None,
        }
    }
}
