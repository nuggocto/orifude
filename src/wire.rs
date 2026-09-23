//! TOML leaf shapes shared by the puzzle-pack and saved-replay formats.
//!
//! Both formats are versioned contracts. Changing a type here changes both, so
//! a change requires a format version decision for each of them.

use serde::{Deserialize, Serialize};

use crate::domain::paper::{BrushRule, Fold, FoldDirection, StrokeAxis};
use crate::domain::score::Par;

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FoldDocument {
    pub(crate) direction: DirectionDocument,
    pub(crate) crease: u8,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DirectionDocument {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum BrushDocument {
    Dot {},
    Line { axis: AxisDocument, length: u8 },
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AxisDocument {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParDocument {
    pub(crate) folds: u8,
    pub(crate) strokes: u8,
}

impl From<FoldDocument> for Fold {
    fn from(fold: FoldDocument) -> Self {
        Self::new(fold.direction.into(), fold.crease)
    }
}

impl From<Fold> for FoldDocument {
    fn from(fold: Fold) -> Self {
        Self {
            direction: fold.direction().into(),
            crease: fold.crease(),
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

impl From<BrushDocument> for BrushRule {
    fn from(brush: BrushDocument) -> Self {
        match brush {
            BrushDocument::Dot {} => Self::Dot,
            BrushDocument::Line { axis, length } => Self::Line {
                axis: axis.into(),
                length,
            },
        }
    }
}

impl From<BrushRule> for BrushDocument {
    fn from(brush: BrushRule) -> Self {
        match brush {
            BrushRule::Dot => Self::Dot {},
            BrushRule::Line { axis, length } => Self::Line {
                axis: axis.into(),
                length,
            },
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

impl From<StrokeAxis> for AxisDocument {
    fn from(axis: StrokeAxis) -> Self {
        match axis {
            StrokeAxis::Horizontal => Self::Horizontal,
            StrokeAxis::Vertical => Self::Vertical,
        }
    }
}

impl From<Par> for ParDocument {
    fn from(par: Par) -> Self {
        Self {
            folds: par.folds().get(),
            strokes: par.strokes().get(),
        }
    }
}
