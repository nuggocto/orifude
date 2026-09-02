use serde::{Deserialize, Serialize};

use crate::domain::paper::{
    BrushRule, Fold, FoldCount, FoldDirection, LineStroke, PaperAction, StrokeAxis, StrokeCount,
};
use crate::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};
use crate::domain::replay::{ENGINE_COMPATIBILITY_VERSION, Replay, ReplayMetadata};
use crate::domain::score::Par;

pub const CURRENT_REPLAY_FORMAT_VERSION: u16 = 1;
pub const MAX_REPLAY_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedReplay {
    puzzle: Puzzle,
    replay: Replay,
}

impl DecodedReplay {
    #[must_use]
    pub const fn puzzle(&self) -> &Puzzle {
        &self.puzzle
    }

    #[must_use]
    pub const fn replay(&self) -> &Replay {
        &self.replay
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReplayDocument {
    replay_format_version: u16,
    engine_compatibility_version: u16,
    puzzle: GameplayDocument,
    actions: Vec<ActionDocument>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GameplayDocument {
    puzzle_format_version: u16,
    pack_id: String,
    puzzle_id: String,
    width: u8,
    height: u8,
    target: Vec<[u8; 2]>,
    folds: Vec<FoldDocument>,
    brushes: Vec<BrushDocument>,
    fold_budget: u8,
    stroke_budget: u8,
    par: Option<ParDocument>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FoldDocument {
    direction: DirectionDocument,
    crease: u8,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum DirectionDocument {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum BrushDocument {
    Dot,
    Line { axis: AxisDocument, length: u8 },
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum AxisDocument {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ParDocument {
    folds: u8,
    strokes: u8,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum ActionDocument {
    Fold {
        direction: DirectionDocument,
        crease: u8,
    },
    Dot {
        row: u8,
        column: u8,
    },
    Line {
        start: [u8; 2],
        end: [u8; 2],
    },
}

pub(super) fn encode(puzzle: &Puzzle, replay: &Replay) -> Result<Vec<u8>, ReplayDocumentError> {
    let dimensions = puzzle.dimensions();
    let target = puzzle
        .target()
        .cell_ids()
        .map(|cell_id| {
            dimensions
                .original_coordinate(cell_id)
                .map(|coordinate| [coordinate.row().get(), coordinate.column().get()])
                .map_err(|_| ReplayDocumentError)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let document = ReplayDocument {
        replay_format_version: CURRENT_REPLAY_FORMAT_VERSION,
        engine_compatibility_version: replay.metadata().engine_compatibility_version(),
        puzzle: GameplayDocument {
            puzzle_format_version: puzzle.format_version(),
            pack_id: puzzle.identity().pack_id().to_owned(),
            puzzle_id: puzzle.identity().puzzle_id().to_owned(),
            width: dimensions.width().get(),
            height: dimensions.height().get(),
            target,
            folds: puzzle
                .allowed_folds()
                .iter()
                .copied()
                .map(FoldDocument::from)
                .collect(),
            brushes: puzzle
                .allowed_brushes()
                .iter()
                .copied()
                .map(BrushDocument::from)
                .collect(),
            fold_budget: puzzle.fold_budget().get(),
            stroke_budget: puzzle.stroke_budget().get(),
            par: puzzle.par().map(|par| ParDocument {
                folds: par.folds().get(),
                strokes: par.strokes().get(),
            }),
        },
        actions: replay
            .actions()
            .iter()
            .copied()
            .map(ActionDocument::from)
            .collect(),
    };
    let bytes = toml::to_string(&document)
        .map_err(|_| ReplayDocumentError)?
        .into_bytes();
    if bytes.len() > MAX_REPLAY_BYTES {
        return Err(ReplayDocumentError);
    }
    Ok(bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<DecodedReplay, ReplayDocumentError> {
    if bytes.len() > MAX_REPLAY_BYTES {
        return Err(ReplayDocumentError);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ReplayDocumentError)?;
    let document: ReplayDocument = toml::from_str(text).map_err(|_| ReplayDocumentError)?;
    if document.replay_format_version != CURRENT_REPLAY_FORMAT_VERSION
        || document.engine_compatibility_version != ENGINE_COMPATIBILITY_VERSION
    {
        return Err(ReplayDocumentError);
    }
    let gameplay = document.puzzle;
    let identity = PuzzleIdentity::new(&gameplay.pack_id, &gameplay.puzzle_id)
        .map_err(|_| ReplayDocumentError)?;
    let dimensions = crate::domain::paper::Dimensions::new(gameplay.width, gameplay.height)
        .map_err(|_| ReplayDocumentError)?;
    let target = gameplay
        .target
        .iter()
        .map(|[row, column]| {
            dimensions
                .coordinate(*row, *column)
                .and_then(|coordinate| dimensions.cell_id(coordinate))
                .map_err(|_| ReplayDocumentError)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let folds = gameplay
        .folds
        .into_iter()
        .map(|fold| Fold::new(fold.direction.into(), fold.crease))
        .collect();
    let brushes = gameplay.brushes.into_iter().map(BrushRule::from).collect();
    let mut spec = PuzzleSpec::new(identity, gameplay.width, gameplay.height)
        .with_format_version(gameplay.puzzle_format_version)
        .with_target_cells(target)
        .with_allowed_folds(folds)
        .with_allowed_brushes(brushes)
        .with_budgets(gameplay.fold_budget, gameplay.stroke_budget);
    if let Some(par) = gameplay.par {
        spec = spec.with_par(Par::new(
            FoldCount::new(par.folds).map_err(|_| ReplayDocumentError)?,
            StrokeCount::new(par.strokes).map_err(|_| ReplayDocumentError)?,
        ));
    }
    let puzzle = Puzzle::new(spec).map_err(|_| ReplayDocumentError)?;
    let actions = document
        .actions
        .into_iter()
        .map(|action| action.into_action(dimensions))
        .collect::<Result<Vec<_>, _>>()?;
    let metadata = ReplayMetadata::new(
        puzzle.identity().clone(),
        puzzle.revision(),
        puzzle.format_version(),
        document.engine_compatibility_version,
    );
    let replay = Replay::new(metadata, actions).map_err(|_| ReplayDocumentError)?;
    let attempt = replay.execute(&puzzle).map_err(|_| ReplayDocumentError)?;
    if !attempt.result().is_success() {
        return Err(ReplayDocumentError);
    }
    Ok(DecodedReplay { puzzle, replay })
}

impl From<Fold> for FoldDocument {
    fn from(fold: Fold) -> Self {
        Self {
            direction: fold.direction().into(),
            crease: fold.crease(),
        }
    }
}

impl From<FoldDirection> for DirectionDocument {
    fn from(direction: FoldDirection) -> Self {
        match direction {
            FoldDirection::Left => Self::Left,
            FoldDirection::Right => Self::Right,
            FoldDirection::Up => Self::Up,
            FoldDirection::Down => Self::Down,
        }
    }
}

impl From<DirectionDocument> for FoldDirection {
    fn from(direction: DirectionDocument) -> Self {
        match direction {
            DirectionDocument::Left => Self::Left,
            DirectionDocument::Right => Self::Right,
            DirectionDocument::Up => Self::Up,
            DirectionDocument::Down => Self::Down,
        }
    }
}

impl From<BrushRule> for BrushDocument {
    fn from(brush: BrushRule) -> Self {
        match brush {
            BrushRule::Dot => Self::Dot,
            BrushRule::Line { axis, length } => Self::Line {
                axis: axis.into(),
                length,
            },
        }
    }
}

impl From<BrushDocument> for BrushRule {
    fn from(brush: BrushDocument) -> Self {
        match brush {
            BrushDocument::Dot => Self::Dot,
            BrushDocument::Line { axis, length } => Self::Line {
                axis: axis.into(),
                length,
            },
        }
    }
}

impl From<StrokeAxis> for AxisDocument {
    fn from(axis: StrokeAxis) -> Self {
        match axis {
            StrokeAxis::Horizontal => Self::Horizontal,
            StrokeAxis::Vertical => Self::Vertical,
        }
    }
}

impl From<AxisDocument> for StrokeAxis {
    fn from(axis: AxisDocument) -> Self {
        match axis {
            AxisDocument::Horizontal => Self::Horizontal,
            AxisDocument::Vertical => Self::Vertical,
        }
    }
}

impl From<PaperAction> for ActionDocument {
    fn from(action: PaperAction) -> Self {
        match action {
            PaperAction::Fold(fold) => Self::Fold {
                direction: fold.direction().into(),
                crease: fold.crease(),
            },
            PaperAction::Dot(coordinate) => Self::Dot {
                row: coordinate.row().get(),
                column: coordinate.column().get(),
            },
            PaperAction::Line(line) => Self::Line {
                start: [line.start().row().get(), line.start().column().get()],
                end: [line.end().row().get(), line.end().column().get()],
            },
        }
    }
}

impl ActionDocument {
    fn into_action(
        self,
        dimensions: crate::domain::paper::Dimensions,
    ) -> Result<PaperAction, ReplayDocumentError> {
        match self {
            Self::Fold { direction, crease } => {
                Ok(PaperAction::Fold(Fold::new(direction.into(), crease)))
            }
            Self::Dot { row, column } => dimensions
                .coordinate(row, column)
                .map(PaperAction::Dot)
                .map_err(|_| ReplayDocumentError),
            Self::Line { start, end } => {
                let start = dimensions
                    .coordinate(start[0], start[1])
                    .map_err(|_| ReplayDocumentError)?;
                let end = dimensions
                    .coordinate(end[0], end[1])
                    .map_err(|_| ReplayDocumentError)?;
                Ok(PaperAction::Line(LineStroke::new(start, end)))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ReplayDocumentError;
