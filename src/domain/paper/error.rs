use std::error::Error;
use std::fmt;

use crate::domain::paper::{
    ActionCount, CellId, Coordinate, Dimensions, FoldAxis, FoldCount, FoldDirection, MAX_ACTIONS,
    MAX_BOARD_HEIGHT, MAX_BOARD_WIDTH, MAX_FOLD_ACTIONS, MAX_PHYSICAL_CELLS, MAX_STROKE_ACTIONS,
    MIN_BOARD_HEIGHT, MIN_BOARD_WIDTH, StrokeCount,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaperError {
    CellIdOutOfRange {
        value: u8,
    },
    RowOutOfRange {
        value: u8,
    },
    ColumnOutOfRange {
        value: u8,
    },
    WidthOutOfRange {
        value: u8,
    },
    HeightOutOfRange {
        value: u8,
    },
    FoldBudgetOutOfRange {
        value: u8,
    },
    StrokeBudgetOutOfRange {
        value: u8,
    },
    ActionBudgetOutOfRange {
        value: u8,
    },
    CoordinateOutsidePaper {
        row: u8,
        column: u8,
        width: u8,
        height: u8,
    },
    CellOutsidePaper {
        cell_id: CellId,
        cell_count: usize,
    },
    TooManyTargetCells {
        count: usize,
    },
    CreaseOutsidePaper {
        axis: FoldAxis,
        crease: u8,
        extent: u8,
    },
    FoldLeavesPaper {
        direction: FoldDirection,
        crease: u8,
        index: u8,
        extent: u8,
    },
    EmptyMovingSide {
        direction: FoldDirection,
    },
    FoldBudgetExhausted {
        limit: FoldCount,
    },
    StrokeBudgetExhausted {
        limit: StrokeCount,
    },
    ActionBudgetExhausted {
        limit: ActionCount,
    },
    EmptyBrushPosition {
        coordinate: Coordinate,
    },
    LineIsTooShort,
    LineIsNotAxisAligned {
        start: Coordinate,
        end: Coordinate,
    },
    TargetDimensionsDiffer {
        paper: Dimensions,
        target: Dimensions,
    },
    NothingToUndo,
}

impl fmt::Display for PaperError {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CellIdOutOfRange { value } => {
                write!(formatter, "physical cell ID {value} exceeds the limit")
            }
            Self::RowOutOfRange { value } => write!(formatter, "row {value} exceeds the limit"),
            Self::ColumnOutOfRange { value } => {
                write!(formatter, "column {value} exceeds the limit")
            }
            Self::WidthOutOfRange { value } => write!(
                formatter,
                "paper width {value} must be between {MIN_BOARD_WIDTH} and {MAX_BOARD_WIDTH}"
            ),
            Self::HeightOutOfRange { value } => write!(
                formatter,
                "paper height {value} must be between {MIN_BOARD_HEIGHT} and {MAX_BOARD_HEIGHT}"
            ),
            Self::FoldBudgetOutOfRange { value } => {
                write!(formatter, "fold budget {value} exceeds {MAX_FOLD_ACTIONS}")
            }
            Self::StrokeBudgetOutOfRange { value } => write!(
                formatter,
                "stroke budget {value} exceeds {MAX_STROKE_ACTIONS}"
            ),
            Self::ActionBudgetOutOfRange { value } => {
                write!(formatter, "action budget {value} exceeds {MAX_ACTIONS}")
            }
            Self::CoordinateOutsidePaper {
                row,
                column,
                width,
                height,
            } => write!(
                formatter,
                "coordinate ({row}, {column}) is outside a {width} by {height} paper"
            ),
            Self::CellOutsidePaper {
                cell_id,
                cell_count,
            } => write!(
                formatter,
                "physical cell ID {} is outside a paper with {cell_count} cells",
                cell_id.get()
            ),
            Self::TooManyTargetCells { count } => write!(
                formatter,
                "target contains {count} entries, above the {MAX_PHYSICAL_CELLS}-cell limit"
            ),
            Self::CreaseOutsidePaper {
                axis,
                crease,
                extent,
            } => write!(
                formatter,
                "{axis} crease {crease} is outside the paper extent {extent}"
            ),
            Self::FoldLeavesPaper {
                direction,
                crease,
                index,
                extent,
            } => write!(
                formatter,
                "the {direction} fold at crease {crease} reflects index {index} outside extent {extent}"
            ),
            Self::EmptyMovingSide { direction } => {
                write!(
                    formatter,
                    "the {direction} fold has no paper on its moving side"
                )
            }
            Self::FoldBudgetExhausted { limit } => {
                write!(formatter, "the fold budget of {} is exhausted", limit.get())
            }
            Self::StrokeBudgetExhausted { limit } => write!(
                formatter,
                "the brush-stroke budget of {} is exhausted",
                limit.get()
            ),
            Self::ActionBudgetExhausted { limit } => {
                write!(
                    formatter,
                    "the action budget of {} is exhausted",
                    limit.get()
                )
            }
            Self::EmptyBrushPosition { coordinate } => write!(
                formatter,
                "cannot stamp empty position ({}, {})",
                coordinate.row.get(),
                coordinate.column.get()
            ),
            Self::LineIsTooShort => {
                formatter.write_str("a line brush must cover at least two positions")
            }
            Self::LineIsNotAxisAligned { start, end } => write!(
                formatter,
                "line endpoints ({}, {}) and ({}, {}) are not horizontally or vertically aligned",
                start.row.get(),
                start.column.get(),
                end.row.get(),
                end.column.get()
            ),
            Self::TargetDimensionsDiffer { paper, target } => write!(
                formatter,
                "target is {} by {}, but the paper is {} by {}",
                target.width.get(),
                target.height.get(),
                paper.width.get(),
                paper.height.get()
            ),
            Self::NothingToUndo => formatter.write_str("there is no paper action to undo"),
        }
    }
}

impl Error for PaperError {}
