//! Puzzle results and solution ordering.

use crate::domain::paper::{FoldCount, InkComparison, StrokeCount};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Par {
    folds: FoldCount,
    strokes: StrokeCount,
}

impl Par {
    #[must_use]
    pub const fn new(folds: FoldCount, strokes: StrokeCount) -> Self {
        Self { folds, strokes }
    }

    #[must_use]
    pub const fn folds(self) -> FoldCount {
        self.folds
    }

    #[must_use]
    pub const fn strokes(self) -> StrokeCount {
        self.strokes
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Score {
    folds: FoldCount,
    strokes: StrokeCount,
}

impl Score {
    #[must_use]
    pub const fn new(folds: FoldCount, strokes: StrokeCount) -> Self {
        Self { folds, strokes }
    }

    #[must_use]
    pub const fn folds(self) -> FoldCount {
        self.folds
    }

    #[must_use]
    pub const fn strokes(self) -> StrokeCount {
        self.strokes
    }

    #[must_use]
    pub const fn meets(self, par: Par) -> bool {
        self.folds.get() <= par.folds.get() && self.strokes.get() <= par.strokes.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttemptResult {
    comparison: InkComparison,
    score: Score,
    undo_count: u64,
    hints_used: bool,
    meets_par: Option<bool>,
}

impl AttemptResult {
    #[must_use]
    pub const fn new(
        comparison: InkComparison,
        score: Score,
        undo_count: u64,
        hints_used: bool,
        par: Option<Par>,
    ) -> Self {
        let meets_par = match par {
            Some(par) => Some(comparison.is_exact() && score.meets(par)),
            None => None,
        };
        Self {
            comparison,
            score,
            undo_count,
            hints_used,
            meets_par,
        }
    }

    #[must_use]
    pub const fn comparison(self) -> InkComparison {
        self.comparison
    }

    #[must_use]
    pub const fn is_success(self) -> bool {
        self.comparison.is_exact()
    }

    #[must_use]
    pub const fn score(self) -> Score {
        self.score
    }

    #[must_use]
    pub const fn undo_count(self) -> u64 {
        self.undo_count
    }

    #[must_use]
    pub const fn hints_used(self) -> bool {
        self.hints_used
    }

    #[must_use]
    pub const fn meets_par(self) -> Option<bool> {
        self.meets_par
    }
}
