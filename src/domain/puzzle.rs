//! Validated puzzle contracts.

use std::error::Error;
use std::fmt;

use crate::domain::attempt::Attempt;
use crate::domain::paper::{
    BrushRule, CellId, Dimensions, Fold, FoldCount, InkPattern, MAX_BOARD_HEIGHT, MAX_BOARD_WIDTH,
    MAX_PHYSICAL_CELLS, PaperError, PaperSpec, StrokeAxis, StrokeCount,
};
use crate::domain::score::Par;

pub const CURRENT_PUZZLE_FORMAT_VERSION: u16 = 1;
pub const MAX_ID_BYTES: usize = 64;
pub const MAX_ALLOWED_FOLDS: usize =
    (MAX_BOARD_WIDTH as usize - 1) * 2 + (MAX_BOARD_HEIGHT as usize - 1) * 2;
pub const MAX_BRUSH_RULES: usize =
    1 + (MAX_BOARD_WIDTH as usize - 1) + (MAX_BOARD_HEIGHT as usize - 1);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IdentityPart {
    Pack,
    Puzzle,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IdentityErrorReason {
    Empty,
    TooLong,
    InvalidCharacter,
    InvalidHyphen,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PuzzleIdentity {
    pack_id: Box<str>,
    puzzle_id: Box<str>,
}

/// Exact bounded gameplay definition used to bind saved replays.
///
/// Display metadata is deliberately absent. Two puzzles have the same revision
/// only when every field that can change legal actions or result evaluation is
/// equal.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PuzzleRevision {
    dimensions: Dimensions,
    target: InkPattern,
    allowed_folds: Box<[Fold]>,
    allowed_brushes: Box<[BrushRule]>,
    fold_budget: FoldCount,
    stroke_budget: StrokeCount,
    par: Option<Par>,
}

impl PuzzleRevision {
    pub(crate) fn matches(&self, puzzle: &Puzzle) -> bool {
        self.dimensions == puzzle.dimensions
            && self.target == puzzle.target
            && self.allowed_folds == puzzle.allowed_folds
            && self.allowed_brushes == puzzle.allowed_brushes
            && self.fold_budget == puzzle.fold_budget
            && self.stroke_budget == puzzle.stroke_budget
            && self.par == puzzle.par
    }
}

impl PuzzleIdentity {
    /// Creates a portable pack-scoped puzzle identity.
    ///
    /// # Errors
    ///
    /// Returns a typed error when either component violates the recorded ASCII
    /// identifier grammar or the 64-byte bound.
    pub fn new(pack_id: &str, puzzle_id: &str) -> Result<Self, PuzzleError> {
        validate_id(pack_id, IdentityPart::Pack)?;
        validate_id(puzzle_id, IdentityPart::Puzzle)?;
        Ok(Self {
            pack_id: pack_id.into(),
            puzzle_id: puzzle_id.into(),
        })
    }

    #[must_use]
    pub fn pack_id(&self) -> &str {
        &self.pack_id
    }

    #[must_use]
    pub fn puzzle_id(&self) -> &str {
        &self.puzzle_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PuzzleSpec {
    format_version: u16,
    identity: PuzzleIdentity,
    width: u8,
    height: u8,
    target_cells: Vec<CellId>,
    allowed_folds: Vec<Fold>,
    allowed_brushes: Vec<BrushRule>,
    fold_budget: u8,
    stroke_budget: u8,
    par: Option<Par>,
}

impl PuzzleSpec {
    #[must_use]
    pub const fn new(identity: PuzzleIdentity, width: u8, height: u8) -> Self {
        Self {
            format_version: CURRENT_PUZZLE_FORMAT_VERSION,
            identity,
            width,
            height,
            target_cells: Vec::new(),
            allowed_folds: Vec::new(),
            allowed_brushes: Vec::new(),
            fold_budget: 0,
            stroke_budget: 0,
            par: None,
        }
    }

    #[must_use]
    pub const fn with_format_version(mut self, format_version: u16) -> Self {
        self.format_version = format_version;
        self
    }

    #[must_use]
    pub fn with_target_cells(mut self, target_cells: Vec<CellId>) -> Self {
        self.target_cells = target_cells;
        self
    }

    #[must_use]
    pub fn with_allowed_folds(mut self, allowed_folds: Vec<Fold>) -> Self {
        self.allowed_folds = allowed_folds;
        self
    }

    #[must_use]
    pub fn with_allowed_brushes(mut self, allowed_brushes: Vec<BrushRule>) -> Self {
        self.allowed_brushes = allowed_brushes;
        self
    }

    #[must_use]
    pub const fn with_budgets(mut self, fold_budget: u8, stroke_budget: u8) -> Self {
        self.fold_budget = fold_budget;
        self.stroke_budget = stroke_budget;
        self
    }

    #[must_use]
    pub const fn with_par(mut self, par: Par) -> Self {
        self.par = Some(par);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Puzzle {
    format_version: u16,
    identity: PuzzleIdentity,
    dimensions: Dimensions,
    target: InkPattern,
    allowed_folds: Box<[Fold]>,
    allowed_brushes: Box<[BrushRule]>,
    fold_budget: FoldCount,
    stroke_budget: StrokeCount,
    par: Option<Par>,
}

impl Puzzle {
    /// Validates and constructs one playable puzzle.
    ///
    /// # Errors
    ///
    /// Returns a typed validation error before playable state exists.
    pub fn new(mut spec: PuzzleSpec) -> Result<Self, PuzzleError> {
        if spec.format_version != CURRENT_PUZZLE_FORMAT_VERSION {
            return Err(PuzzleError::UnsupportedFormatVersion {
                found: spec.format_version,
                supported: CURRENT_PUZZLE_FORMAT_VERSION,
            });
        }

        let dimensions = Dimensions::new(spec.width, spec.height)?;
        let fold_budget = FoldCount::new(spec.fold_budget)?;
        let stroke_budget = StrokeCount::new(spec.stroke_budget)?;
        validate_target_cells(dimensions, &spec.target_cells)?;
        let target = InkPattern::from_cell_ids(dimensions, &spec.target_cells)?;

        validate_allowed_folds(dimensions, &mut spec.allowed_folds)?;
        validate_allowed_brushes(dimensions, &mut spec.allowed_brushes)?;
        validate_budget_rules(
            fold_budget,
            stroke_budget,
            &spec.allowed_folds,
            &spec.allowed_brushes,
        )?;
        if !target.is_empty() && stroke_budget.get() == 0 {
            return Err(PuzzleError::TargetNeedsStroke);
        }
        if let Some(par) = spec.par
            && (par.folds() > fold_budget || par.strokes() > stroke_budget)
        {
            return Err(PuzzleError::ParExceedsBudget {
                par,
                fold_budget,
                stroke_budget,
            });
        }

        Ok(Self {
            format_version: spec.format_version,
            identity: spec.identity,
            dimensions,
            target,
            allowed_folds: spec.allowed_folds.into_boxed_slice(),
            allowed_brushes: spec.allowed_brushes.into_boxed_slice(),
            fold_budget,
            stroke_budget,
            par: spec.par,
        })
    }

    #[must_use]
    pub const fn format_version(&self) -> u16 {
        self.format_version
    }

    #[must_use]
    pub const fn identity(&self) -> &PuzzleIdentity {
        &self.identity
    }

    #[must_use]
    pub const fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    #[must_use]
    pub const fn target(&self) -> InkPattern {
        self.target
    }

    #[must_use]
    pub fn allowed_folds(&self) -> &[Fold] {
        &self.allowed_folds
    }

    #[must_use]
    pub fn allowed_brushes(&self) -> &[BrushRule] {
        &self.allowed_brushes
    }

    #[must_use]
    pub const fn fold_budget(&self) -> FoldCount {
        self.fold_budget
    }

    #[must_use]
    pub const fn stroke_budget(&self) -> StrokeCount {
        self.stroke_budget
    }

    #[must_use]
    pub const fn par(&self) -> Option<Par> {
        self.par
    }

    /// Returns the complete bounded gameplay revision for replay validation.
    #[must_use]
    pub fn revision(&self) -> PuzzleRevision {
        PuzzleRevision {
            dimensions: self.dimensions,
            target: self.target,
            allowed_folds: self.allowed_folds.clone(),
            allowed_brushes: self.allowed_brushes.clone(),
            fold_budget: self.fold_budget,
            stroke_budget: self.stroke_budget,
            par: self.par,
        }
    }

    /// Starts an attempt that owns a bounded copy of this validated puzzle.
    ///
    /// Owning the puzzle keeps application state and replay results free of
    /// self-referential lifetimes. The copied rule collections remain within
    /// the explicit puzzle bounds.
    #[must_use]
    pub fn start(&self) -> Attempt {
        Attempt::new(self.clone())
    }

    pub(crate) fn paper_spec(&self) -> PaperSpec {
        PaperSpec::new(
            self.dimensions.width().get(),
            self.dimensions.height().get(),
            self.fold_budget.get(),
            self.stroke_budget.get(),
            crate::domain::paper::MAX_ACTIONS,
        )
    }
}

fn validate_id(value: &str, part: IdentityPart) -> Result<(), PuzzleError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Err(PuzzleError::InvalidIdentity {
            part,
            reason: IdentityErrorReason::Empty,
        });
    }
    if bytes.len() > MAX_ID_BYTES {
        return Err(PuzzleError::InvalidIdentity {
            part,
            reason: IdentityErrorReason::TooLong,
        });
    }
    let is_segment_character = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    if !is_segment_character(bytes[0]) || !is_segment_character(bytes[bytes.len() - 1]) {
        let invalid = if bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
            IdentityErrorReason::InvalidHyphen
        } else {
            IdentityErrorReason::InvalidCharacter
        };
        return Err(PuzzleError::InvalidIdentity {
            part,
            reason: invalid,
        });
    }
    let mut previous_hyphen = false;
    for &byte in bytes {
        if is_segment_character(byte) {
            previous_hyphen = false;
        } else if byte == b'-' && !previous_hyphen {
            previous_hyphen = true;
        } else {
            return Err(PuzzleError::InvalidIdentity {
                part,
                reason: if byte == b'-' {
                    IdentityErrorReason::InvalidHyphen
                } else {
                    IdentityErrorReason::InvalidCharacter
                },
            });
        }
    }
    Ok(())
}

fn validate_target_cells(
    dimensions: Dimensions,
    target_cells: &[CellId],
) -> Result<(), PuzzleError> {
    if target_cells.len() > MAX_PHYSICAL_CELLS {
        return Err(PaperError::TooManyTargetCells {
            count: target_cells.len(),
        }
        .into());
    }
    let mut seen = [false; MAX_PHYSICAL_CELLS];
    for &cell_id in target_cells {
        if cell_id.index() >= dimensions.cell_count() {
            return Err(PaperError::CellOutsidePaper {
                cell_id,
                cell_count: dimensions.cell_count(),
            }
            .into());
        }
        if seen[cell_id.index()] {
            return Err(PuzzleError::DuplicateTargetCell { cell_id });
        }
        seen[cell_id.index()] = true;
    }
    Ok(())
}

fn validate_allowed_folds(
    dimensions: Dimensions,
    allowed_folds: &mut [Fold],
) -> Result<(), PuzzleError> {
    if allowed_folds.len() > MAX_ALLOWED_FOLDS {
        return Err(PuzzleError::TooManyAllowedFolds {
            count: allowed_folds.len(),
            limit: MAX_ALLOWED_FOLDS,
        });
    }
    allowed_folds.sort_unstable();
    if let Some(pair) = allowed_folds.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(PuzzleError::DuplicateAllowedFold { fold: pair[0] });
    }
    for &fold in allowed_folds.iter() {
        let extent = match fold.direction().axis() {
            crate::domain::paper::FoldAxis::Vertical => dimensions.width().get(),
            crate::domain::paper::FoldAxis::Horizontal => dimensions.height().get(),
        };
        if fold.crease() == 0 || fold.crease() >= extent {
            return Err(PuzzleError::AllowedCreaseOutsidePaper { fold, extent });
        }
    }
    Ok(())
}

fn validate_allowed_brushes(
    dimensions: Dimensions,
    allowed_brushes: &mut [BrushRule],
) -> Result<(), PuzzleError> {
    if allowed_brushes.len() > MAX_BRUSH_RULES {
        return Err(PuzzleError::TooManyBrushRules {
            count: allowed_brushes.len(),
            limit: MAX_BRUSH_RULES,
        });
    }
    allowed_brushes.sort_unstable();
    if let Some(pair) = allowed_brushes.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(PuzzleError::DuplicateBrushRule { rule: pair[0] });
    }
    for &rule in allowed_brushes.iter() {
        let BrushRule::Line { axis, length } = rule else {
            continue;
        };
        let extent = match axis {
            StrokeAxis::Horizontal => dimensions.width().get(),
            StrokeAxis::Vertical => dimensions.height().get(),
        };
        if length < 2 || length > extent {
            return Err(PuzzleError::LineLengthOutOfRange {
                axis,
                length,
                extent,
            });
        }
    }
    Ok(())
}

fn validate_budget_rules(
    fold_budget: FoldCount,
    stroke_budget: StrokeCount,
    allowed_folds: &[Fold],
    allowed_brushes: &[BrushRule],
) -> Result<(), PuzzleError> {
    if fold_budget.get() == 0 && !allowed_folds.is_empty() {
        return Err(PuzzleError::FoldRulesWithoutBudget);
    }
    if fold_budget.get() > 0 && allowed_folds.is_empty() {
        return Err(PuzzleError::FoldBudgetWithoutRules);
    }
    if stroke_budget.get() == 0 && !allowed_brushes.is_empty() {
        return Err(PuzzleError::BrushRulesWithoutBudget);
    }
    if stroke_budget.get() > 0 && allowed_brushes.is_empty() {
        return Err(PuzzleError::StrokeBudgetWithoutRules);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PuzzleError {
    InvalidIdentity {
        part: IdentityPart,
        reason: IdentityErrorReason,
    },
    UnsupportedFormatVersion {
        found: u16,
        supported: u16,
    },
    Paper(PaperError),
    DuplicateTargetCell {
        cell_id: CellId,
    },
    TooManyAllowedFolds {
        count: usize,
        limit: usize,
    },
    DuplicateAllowedFold {
        fold: Fold,
    },
    AllowedCreaseOutsidePaper {
        fold: Fold,
        extent: u8,
    },
    TooManyBrushRules {
        count: usize,
        limit: usize,
    },
    DuplicateBrushRule {
        rule: BrushRule,
    },
    LineLengthOutOfRange {
        axis: StrokeAxis,
        length: u8,
        extent: u8,
    },
    FoldRulesWithoutBudget,
    FoldBudgetWithoutRules,
    BrushRulesWithoutBudget,
    StrokeBudgetWithoutRules,
    TargetNeedsStroke,
    ParExceedsBudget {
        par: Par,
        fold_budget: FoldCount,
        stroke_budget: StrokeCount,
    },
}

impl From<PaperError> for PuzzleError {
    fn from(error: PaperError) -> Self {
        Self::Paper(error)
    }
}

impl fmt::Display for PuzzleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentity { part, reason } => {
                write!(formatter, "the {part} ID is invalid: {reason}")
            }
            Self::UnsupportedFormatVersion { found, supported } => write!(
                formatter,
                "puzzle format version {found} is unsupported; this engine accepts {supported}"
            ),
            Self::Paper(error) => error.fmt(formatter),
            Self::DuplicateTargetCell { cell_id } => {
                write!(formatter, "target repeats physical cell {}", cell_id.get())
            }
            Self::TooManyAllowedFolds { count, limit } => {
                write!(
                    formatter,
                    "puzzle declares {count} folds, above the {limit}-fold limit"
                )
            }
            Self::DuplicateAllowedFold { fold } => write!(
                formatter,
                "puzzle repeats the {} fold at crease {}",
                fold.direction(),
                fold.crease()
            ),
            Self::AllowedCreaseOutsidePaper { fold, extent } => write!(
                formatter,
                "the allowed {} fold at crease {} is outside extent {extent}",
                fold.direction(),
                fold.crease()
            ),
            Self::TooManyBrushRules { count, limit } => write!(
                formatter,
                "puzzle declares {count} brush rules, above the {limit}-rule limit"
            ),
            Self::DuplicateBrushRule { rule } => {
                write!(formatter, "puzzle repeats the {rule} brush rule")
            }
            Self::LineLengthOutOfRange {
                axis,
                length,
                extent,
            } => write!(
                formatter,
                "{axis} line length {length} must be between 2 and {extent}"
            ),
            Self::FoldRulesWithoutBudget => {
                formatter.write_str("puzzle allows folds but has no fold budget")
            }
            Self::FoldBudgetWithoutRules => {
                formatter.write_str("puzzle has a fold budget but allows no folds")
            }
            Self::BrushRulesWithoutBudget => {
                formatter.write_str("puzzle allows brushes but has no stroke budget")
            }
            Self::StrokeBudgetWithoutRules => {
                formatter.write_str("puzzle has a stroke budget but allows no brushes")
            }
            Self::TargetNeedsStroke => {
                formatter.write_str("a nonempty target requires at least one brush stroke")
            }
            Self::ParExceedsBudget {
                par,
                fold_budget,
                stroke_budget,
            } => write!(
                formatter,
                "par {}/{} exceeds puzzle budgets {}/{}",
                par.folds().get(),
                par.strokes().get(),
                fold_budget.get(),
                stroke_budget.get()
            ),
        }
    }
}

impl Error for PuzzleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Paper(error) => Some(error),
            _ => None,
        }
    }
}

impl fmt::Display for IdentityPart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pack => formatter.write_str("pack"),
            Self::Puzzle => formatter.write_str("puzzle"),
        }
    }
}

impl fmt::Display for IdentityErrorReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("it is empty"),
            Self::TooLong => write!(formatter, "it exceeds {MAX_ID_BYTES} bytes"),
            Self::InvalidCharacter => formatter
                .write_str("use lowercase ASCII letters and digits separated by single hyphens"),
            Self::InvalidHyphen => {
                formatter.write_str("hyphens cannot begin, end, or repeat in an ID")
            }
        }
    }
}
