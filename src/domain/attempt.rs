//! Playable puzzle attempts.

use std::error::Error;
use std::fmt;

use crate::domain::paper::{
    ActionCount, BrushRule, CellId, Coordinate, Dimensions, FoldCount, InkPattern, LineStroke,
    Paper, PaperAction, PaperError, PaperStateKey, PhysicalCell, StackView, StrokeCount,
};
use crate::domain::puzzle::Puzzle;
use crate::domain::score::{AttemptResult, Score};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attempt {
    puzzle: Puzzle,
    paper: Paper,
    undo_count: u64,
    hints_used: bool,
}

impl Attempt {
    pub(crate) fn new(puzzle: Puzzle) -> Self {
        let paper = Paper::new(puzzle.paper_spec())
            .expect("a validated puzzle must construct its canonical paper");
        Self {
            puzzle,
            paper,
            undo_count: 0,
            hints_used: false,
        }
    }

    #[must_use]
    pub const fn puzzle(&self) -> &Puzzle {
        &self.puzzle
    }

    #[must_use]
    pub const fn dimensions(&self) -> Dimensions {
        self.paper.dimensions()
    }

    #[must_use]
    pub const fn fold_count(&self) -> FoldCount {
        self.paper.fold_count()
    }

    #[must_use]
    pub const fn stroke_count(&self) -> StrokeCount {
        self.paper.stroke_count()
    }

    #[must_use]
    pub const fn action_count(&self) -> ActionCount {
        self.paper.action_count()
    }

    #[must_use]
    pub const fn ink(&self) -> InkPattern {
        self.paper.ink()
    }

    #[must_use]
    pub const fn undo_count(&self) -> u64 {
        self.undo_count
    }

    #[must_use]
    pub const fn hints_used(&self) -> bool {
        self.hints_used
    }

    pub fn cell_ids(&self) -> impl Iterator<Item = CellId> + '_ {
        self.paper.cell_ids()
    }

    #[must_use]
    pub fn physical_cell(&self, cell_id: CellId) -> Option<PhysicalCell> {
        self.paper.physical_cell(cell_id)
    }

    /// Derives one bottom-to-top stack into caller-owned bounded storage.
    ///
    /// # Errors
    ///
    /// Returns an operational error when the coordinate is outside the paper.
    pub fn stack_at(
        &self,
        coordinate: Coordinate,
        stack: &mut StackView,
    ) -> Result<(), ActionError> {
        self.paper
            .stack_at(coordinate, stack)
            .map_err(ActionError::Paper)
    }

    #[must_use]
    pub fn state_key(&self) -> PaperStateKey {
        self.paper.state_key()
    }

    pub fn actions(&self) -> impl Iterator<Item = PaperAction> + '_ {
        self.paper.actions()
    }

    /// Applies one puzzle-approved action.
    ///
    /// # Errors
    ///
    /// Returns a typed operational error without changing state when the
    /// puzzle disallows the action or paper legality rejects it.
    pub fn apply(&mut self, action: PaperAction) -> Result<(), ActionError> {
        match action {
            PaperAction::Fold(fold) => {
                if self.puzzle.allowed_folds().binary_search(&fold).is_err() {
                    return Err(ActionError::FoldNotAllowed { fold });
                }
            }
            PaperAction::Dot(_) => {
                if self
                    .puzzle
                    .allowed_brushes()
                    .binary_search(&BrushRule::Dot)
                    .is_err()
                {
                    return Err(ActionError::BrushNotAllowed {
                        rule: BrushRule::Dot,
                    });
                }
            }
            PaperAction::Line(line) => {
                let (axis, length) = line.axis_and_length().map_err(ActionError::Paper)?;
                let rule = BrushRule::Line { axis, length };
                if self.puzzle.allowed_brushes().binary_search(&rule).is_err() {
                    return Err(ActionError::BrushNotAllowed { rule });
                }
            }
        }
        self.paper.apply(action).map_err(ActionError::Paper)
    }

    /// Applies one puzzle-approved fold.
    ///
    /// # Errors
    ///
    /// Returns a typed error without mutation when the fold is disallowed or
    /// illegal in the current state.
    pub fn fold(&mut self, fold: crate::domain::paper::Fold) -> Result<(), ActionError> {
        self.apply(PaperAction::Fold(fold))
    }

    /// Applies one puzzle-approved dot.
    ///
    /// # Errors
    ///
    /// Returns a typed error without mutation when the dot is disallowed or
    /// illegal in the current state.
    pub fn stamp_dot(&mut self, coordinate: Coordinate) -> Result<(), ActionError> {
        self.apply(PaperAction::Dot(coordinate))
    }

    /// Applies one puzzle-approved line.
    ///
    /// # Errors
    ///
    /// Returns a typed error without mutation when the line is disallowed or
    /// illegal in the current state.
    pub fn stamp_line(&mut self, line: LineStroke) -> Result<(), ActionError> {
        self.apply(PaperAction::Line(line))
    }

    /// Restores the complete state before the most recent successful action.
    ///
    /// The personal undo counter saturates only after `u64::MAX` successful
    /// undos. Paper state restoration never depends on that accounting field.
    ///
    /// # Errors
    ///
    /// Returns [`ActionError::Paper`] without mutation when there is no action
    /// to undo.
    pub fn undo(&mut self) -> Result<(), ActionError> {
        self.paper.undo().map_err(ActionError::Paper)?;
        self.undo_count = self.undo_count.saturating_add(1);
        Ok(())
    }

    /// Restores the fresh paper and clears the replayable action sequence.
    ///
    /// Personal hint and undo history remain attached to this attempt.
    pub fn reset(&mut self) {
        self.paper.reset();
    }

    pub const fn mark_hint_used(&mut self) {
        self.hints_used = true;
    }

    /// Compares the complete physical-cell ink state with the puzzle target.
    ///
    /// # Panics
    ///
    /// Panics only if a validated puzzle and its own paper disagree on
    /// dimensions, which is a programmer-error invariant.
    #[must_use]
    pub fn result(&self) -> AttemptResult {
        let comparison = self
            .paper
            .compare_ink(self.puzzle.target())
            .expect("a puzzle target must match its constructed paper");
        AttemptResult::new(
            comparison,
            Score::new(self.paper.fold_count(), self.paper.stroke_count()),
            self.undo_count,
            self.hints_used,
            self.puzzle.par(),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionError {
    FoldNotAllowed { fold: crate::domain::paper::Fold },
    BrushNotAllowed { rule: BrushRule },
    Paper(PaperError),
}

impl fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FoldNotAllowed { fold } => write!(
                formatter,
                "the puzzle does not allow the {} fold at crease {}",
                fold.direction(),
                fold.crease()
            ),
            Self::BrushNotAllowed { rule } => {
                write!(formatter, "the puzzle does not allow the {rule} brush")
            }
            Self::Paper(error) => error.fmt(formatter),
        }
    }
}

impl Error for ActionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Paper(error) => Some(error),
            Self::FoldNotAllowed { .. } | Self::BrushNotAllowed { .. } => None,
        }
    }
}
