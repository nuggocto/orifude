//! Bounded geometry and action values shared by the paper engine.

mod error;
mod state;

pub use error::PaperError;
pub use state::{Paper, PaperStateKey};

use std::fmt;

pub const MIN_BOARD_WIDTH: u8 = 4;
pub const MAX_BOARD_WIDTH: u8 = 12;
pub const MIN_BOARD_HEIGHT: u8 = 4;
pub const MAX_BOARD_HEIGHT: u8 = 12;
pub const MAX_PHYSICAL_CELLS: usize = 144;
pub const MAX_FOLD_ACTIONS: u8 = 12;
pub const MAX_STROKE_ACTIONS: u8 = 8;
pub const MAX_ACTIONS: u8 = 64;

const INK_WORDS: usize = MAX_PHYSICAL_CELLS.div_ceil(u64::BITS as usize);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CellId(u8);

impl CellId {
    /// Creates an identity within the global physical-cell limit.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::CellIdOutOfRange`] for values above the limit.
    pub const fn new(value: u8) -> Result<Self, PaperError> {
        if (value as usize) < MAX_PHYSICAL_CELLS {
            Ok(Self(value))
        } else {
            Err(PaperError::CellIdOutOfRange { value })
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    fn from_index(index: usize) -> Self {
        let value = u8::try_from(index).expect("a physical-cell index must fit in u8");
        Self::new(value).expect("a physical-cell index must stay below the global limit")
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Row(u8);

impl Row {
    /// Creates a row within the global board limit.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::RowOutOfRange`] when `value` cannot name a row on
    /// any supported paper.
    pub const fn new(value: u8) -> Result<Self, PaperError> {
        if value < MAX_BOARD_HEIGHT {
            Ok(Self(value))
        } else {
            Err(PaperError::RowOutOfRange { value })
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Column(u8);

impl Column {
    /// Creates a column within the global board limit.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::ColumnOutOfRange`] when `value` cannot name a
    /// column on any supported paper.
    pub const fn new(value: u8) -> Result<Self, PaperError> {
        if value < MAX_BOARD_WIDTH {
            Ok(Self(value))
        } else {
            Err(PaperError::ColumnOutOfRange { value })
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Width(u8);

impl Width {
    /// Creates a supported paper width.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::WidthOutOfRange`] outside the supported board
    /// bounds.
    pub const fn new(value: u8) -> Result<Self, PaperError> {
        if value >= MIN_BOARD_WIDTH && value <= MAX_BOARD_WIDTH {
            Ok(Self(value))
        } else {
            Err(PaperError::WidthOutOfRange { value })
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Height(u8);

impl Height {
    /// Creates a supported paper height.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::HeightOutOfRange`] outside the supported board
    /// bounds.
    pub const fn new(value: u8) -> Result<Self, PaperError> {
        if value >= MIN_BOARD_HEIGHT && value <= MAX_BOARD_HEIGHT {
            Ok(Self(value))
        } else {
            Err(PaperError::HeightOutOfRange { value })
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Layer(u8);

impl Layer {
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    const fn bottom() -> Self {
        Self(0)
    }

    fn from_index(index: u8) -> Self {
        assert!((index as usize) < MAX_PHYSICAL_CELLS);
        Self(index)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FoldCount(u8);

impl FoldCount {
    /// Creates a fold count within the attempt limit.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::FoldBudgetOutOfRange`] above the global limit.
    pub const fn new(value: u8) -> Result<Self, PaperError> {
        if value <= MAX_FOLD_ACTIONS {
            Ok(Self(value))
        } else {
            Err(PaperError::FoldBudgetOutOfRange { value })
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    fn increment(self) -> Self {
        Self(
            self.0
                .checked_add(1)
                .expect("a validated fold count must not overflow"),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StrokeCount(u8);

impl StrokeCount {
    /// Creates a stroke count within the attempt limit.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::StrokeBudgetOutOfRange`] above the global limit.
    pub const fn new(value: u8) -> Result<Self, PaperError> {
        if value <= MAX_STROKE_ACTIONS {
            Ok(Self(value))
        } else {
            Err(PaperError::StrokeBudgetOutOfRange { value })
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    fn increment(self) -> Self {
        Self(
            self.0
                .checked_add(1)
                .expect("a validated stroke count must not overflow"),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActionCount(u8);

impl ActionCount {
    /// Creates an action count within the replay and history limit.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::ActionBudgetOutOfRange`] above the global limit.
    pub const fn new(value: u8) -> Result<Self, PaperError> {
        if value <= MAX_ACTIONS {
            Ok(Self(value))
        } else {
            Err(PaperError::ActionBudgetOutOfRange { value })
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    fn increment(self) -> Self {
        Self(
            self.0
                .checked_add(1)
                .expect("a validated action count must not overflow"),
        )
    }

    fn from_history_len(length: usize) -> Self {
        let value = u8::try_from(length).expect("bounded history length must fit in u8");
        Self::new(value).expect("history length must stay below the global action limit")
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Coordinate {
    row: Row,
    column: Column,
}

impl Coordinate {
    #[must_use]
    pub const fn new(row: Row, column: Column) -> Self {
        Self { row, column }
    }

    #[must_use]
    pub const fn row(self) -> Row {
        self.row
    }

    #[must_use]
    pub const fn column(self) -> Column {
        self.column
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Dimensions {
    width: Width,
    height: Height,
}

impl Dimensions {
    /// Creates validated dimensions from raw cell counts.
    ///
    /// # Errors
    ///
    /// Returns a dimension-specific error when either count is outside the
    /// supported board bounds.
    pub const fn new(width: u8, height: u8) -> Result<Self, PaperError> {
        let width = match Width::new(width) {
            Ok(width) => width,
            Err(error) => return Err(error),
        };
        let height = match Height::new(height) {
            Ok(height) => height,
            Err(error) => return Err(error),
        };
        Ok(Self { width, height })
    }

    #[must_use]
    pub const fn width(self) -> Width {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> Height {
        self.height
    }

    #[must_use]
    pub const fn cell_count(self) -> usize {
        self.width.get() as usize * self.height.get() as usize
    }

    /// Resolves a coordinate inside these dimensions.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::CoordinateOutsidePaper`] when the coordinate is
    /// not part of this paper.
    pub const fn coordinate(self, row: u8, column: u8) -> Result<Coordinate, PaperError> {
        if row >= self.height.get() || column >= self.width.get() {
            return Err(PaperError::CoordinateOutsidePaper {
                row,
                column,
                width: self.width.get(),
                height: self.height.get(),
            });
        }

        let row = match Row::new(row) {
            Ok(row) => row,
            Err(error) => return Err(error),
        };
        let column = match Column::new(column) {
            Ok(column) => column,
            Err(error) => return Err(error),
        };
        Ok(Coordinate::new(row, column))
    }

    /// Returns the stable row-major identity at a coordinate.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::CoordinateOutsidePaper`] when the coordinate does
    /// not belong to this paper.
    pub fn cell_id(self, coordinate: Coordinate) -> Result<CellId, PaperError> {
        self.validate_coordinate(coordinate)?;
        Ok(CellId::from_index(self.coordinate_index(coordinate)))
    }

    /// Returns the original row-major coordinate for a physical cell.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::CellOutsidePaper`] when the global identity is not
    /// part of this paper.
    ///
    /// # Panics
    ///
    /// Panics only if validated dimensions cannot represent their own
    /// row-major coordinates, which is an internal invariant violation.
    pub fn original_coordinate(self, cell_id: CellId) -> Result<Coordinate, PaperError> {
        if cell_id.index() >= self.cell_count() {
            return Err(PaperError::CellOutsidePaper {
                cell_id,
                cell_count: self.cell_count(),
            });
        }

        let width = usize::from(self.width.get());
        let row = u8::try_from(cell_id.index() / width)
            .expect("a row-major row must fit within the validated height");
        let column = u8::try_from(cell_id.index() % width)
            .expect("a row-major column must fit within the validated width");
        self.coordinate(row, column)
    }

    fn validate_coordinate(self, coordinate: Coordinate) -> Result<(), PaperError> {
        if coordinate.row.get() >= self.height.get() || coordinate.column.get() >= self.width.get()
        {
            return Err(PaperError::CoordinateOutsidePaper {
                row: coordinate.row.get(),
                column: coordinate.column.get(),
                width: self.width.get(),
                height: self.height.get(),
            });
        }
        Ok(())
    }

    fn coordinate_index(self, coordinate: Coordinate) -> usize {
        usize::from(coordinate.row.get()) * usize::from(self.width.get())
            + usize::from(coordinate.column.get())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Face {
    Front,
    Back,
}

impl Face {
    const fn flipped(self) -> Self {
        match self {
            Self::Front => Self::Back,
            Self::Back => Self::Front,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Orientation {
    North,
    East,
    South,
    West,
}

impl Orientation {
    const fn folded_across(self, axis: FoldAxis) -> Self {
        match (axis, self) {
            (FoldAxis::Vertical, Self::North | Self::South)
            | (FoldAxis::Horizontal, Self::East | Self::West) => self,
            (FoldAxis::Vertical, Self::East) => Self::West,
            (FoldAxis::Vertical, Self::West) => Self::East,
            (FoldAxis::Horizontal, Self::North) => Self::South,
            (FoldAxis::Horizontal, Self::South) => Self::North,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PhysicalCell {
    coordinate: Coordinate,
    layer: Layer,
    face: Face,
    orientation: Orientation,
}

impl PhysicalCell {
    #[must_use]
    pub const fn coordinate(self) -> Coordinate {
        self.coordinate
    }

    #[must_use]
    pub const fn layer(self) -> Layer {
        self.layer
    }

    #[must_use]
    pub const fn face(self) -> Face {
        self.face
    }

    #[must_use]
    pub const fn orientation(self) -> Orientation {
        self.orientation
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FoldAxis {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FoldDirection {
    Left,
    Right,
    Up,
    Down,
}

impl FoldDirection {
    #[must_use]
    pub const fn axis(self) -> FoldAxis {
        match self {
            Self::Left | Self::Right => FoldAxis::Vertical,
            Self::Up | Self::Down => FoldAxis::Horizontal,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Fold {
    direction: FoldDirection,
    crease: u8,
}

impl Fold {
    #[must_use]
    pub const fn new(direction: FoldDirection, crease: u8) -> Self {
        Self { direction, crease }
    }

    #[must_use]
    pub const fn direction(self) -> FoldDirection {
        self.direction
    }

    #[must_use]
    pub const fn crease(self) -> u8 {
        self.crease
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StrokeAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BrushRule {
    Dot,
    Line { axis: StrokeAxis, length: u8 },
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LineStroke {
    start: Coordinate,
    end: Coordinate,
}

impl LineStroke {
    /// Creates a line with endpoints in canonical row-major order.
    #[must_use]
    pub fn new(start: Coordinate, end: Coordinate) -> Self {
        if start <= end {
            Self { start, end }
        } else {
            Self {
                start: end,
                end: start,
            }
        }
    }

    #[must_use]
    pub const fn start(self) -> Coordinate {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> Coordinate {
        self.end
    }

    /// Returns the canonical axis and inclusive length of this line.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::LineIsNotAxisAligned`] for a diagonal line and
    /// [`PaperError::LineIsTooShort`] when both endpoints are the same cell.
    pub fn axis_and_length(self) -> Result<(StrokeAxis, u8), PaperError> {
        if self.start.row == self.end.row && self.start.column == self.end.column {
            return Err(PaperError::LineIsTooShort);
        }
        if self.start.row == self.end.row {
            let length = self.start.column.get().abs_diff(self.end.column.get()) + 1;
            return Ok((StrokeAxis::Horizontal, length));
        }
        if self.start.column == self.end.column {
            let length = self.start.row.get().abs_diff(self.end.row.get()) + 1;
            return Ok((StrokeAxis::Vertical, length));
        }
        Err(PaperError::LineIsNotAxisAligned {
            start: self.start,
            end: self.end,
        })
    }

    fn coordinate_at(self, offset: u8) -> Coordinate {
        let row_start = self.start.row.get().min(self.end.row.get());
        let column_start = self.start.column.get().min(self.end.column.get());
        if self.start.row == self.end.row {
            Coordinate::new(
                self.start.row,
                Column::new(
                    column_start
                        .checked_add(offset)
                        .expect("a validated horizontal line must stay in range"),
                )
                .expect("a validated horizontal line column must be globally valid"),
            )
        } else {
            Coordinate::new(
                Row::new(
                    row_start
                        .checked_add(offset)
                        .expect("a validated vertical line must stay in range"),
                )
                .expect("a validated vertical line row must be globally valid"),
                self.start.column,
            )
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PaperAction {
    Fold(Fold),
    Dot(Coordinate),
    Line(LineStroke),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaperSpec {
    width: u8,
    height: u8,
    fold_budget: u8,
    stroke_budget: u8,
    action_budget: u8,
}

impl PaperSpec {
    #[must_use]
    pub const fn new(
        width: u8,
        height: u8,
        fold_budget: u8,
        stroke_budget: u8,
        action_budget: u8,
    ) -> Self {
        Self {
            width,
            height,
            fold_budget,
            stroke_budget,
            action_budget,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InkPattern {
    dimensions: Dimensions,
    words: [u64; INK_WORDS],
}

impl InkPattern {
    #[must_use]
    pub const fn empty(dimensions: Dimensions) -> Self {
        Self {
            dimensions,
            words: [0; INK_WORDS],
        }
    }

    /// Builds an ink pattern from stable physical-cell identities.
    ///
    /// # Errors
    ///
    /// Returns an error when the input exceeds the physical-cell bound or an
    /// identity does not belong to these dimensions.
    pub fn from_cell_ids(dimensions: Dimensions, cell_ids: &[CellId]) -> Result<Self, PaperError> {
        if cell_ids.len() > MAX_PHYSICAL_CELLS {
            return Err(PaperError::TooManyTargetCells {
                count: cell_ids.len(),
            });
        }

        let mut pattern = Self::empty(dimensions);
        for &cell_id in cell_ids {
            if cell_id.index() >= dimensions.cell_count() {
                return Err(PaperError::CellOutsidePaper {
                    cell_id,
                    cell_count: dimensions.cell_count(),
                });
            }
            pattern.insert(cell_id);
        }
        Ok(pattern)
    }

    #[must_use]
    pub fn contains(self, cell_id: CellId) -> bool {
        if cell_id.index() >= self.dimensions.cell_count() {
            return false;
        }
        let (word, mask) = bit_location(cell_id);
        self.words[word] & mask != 0
    }

    #[must_use]
    pub fn len(self) -> usize {
        self.words
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        let mut index = 0;
        while index < INK_WORDS {
            if self.words[index] != 0 {
                return false;
            }
            index += 1;
        }
        true
    }

    pub fn cell_ids(self) -> impl Iterator<Item = CellId> {
        (0..self.dimensions.cell_count())
            .map(CellId::from_index)
            .filter(move |cell_id| self.contains(*cell_id))
    }

    #[must_use]
    pub const fn dimensions(self) -> Dimensions {
        self.dimensions
    }

    fn insert(&mut self, cell_id: CellId) {
        assert!(cell_id.index() < self.dimensions.cell_count());
        let (word, mask) = bit_location(cell_id);
        self.words[word] |= mask;
    }

    fn difference(self, other: Self) -> Self {
        assert_eq!(self.dimensions, other.dimensions);
        let mut words = [0; INK_WORDS];
        for (index, word) in words.iter_mut().enumerate() {
            *word = self.words[index] & !other.words[index];
        }
        Self {
            dimensions: self.dimensions,
            words,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkComparison {
    missing: InkPattern,
    extra: InkPattern,
}

impl InkComparison {
    #[must_use]
    pub const fn missing(self) -> InkPattern {
        self.missing
    }

    #[must_use]
    pub const fn extra(self) -> InkPattern {
        self.extra
    }

    #[must_use]
    pub const fn is_exact(self) -> bool {
        self.missing.is_empty() && self.extra.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StackView {
    cell_ids: [CellId; MAX_PHYSICAL_CELLS],
    length: u8,
}

impl StackView {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cell_ids: [CellId(0); MAX_PHYSICAL_CELLS],
            length: 0,
        }
    }

    #[must_use]
    pub fn cell_ids(&self) -> &[CellId] {
        &self.cell_ids[..usize::from(self.length)]
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.length as usize
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }

    fn clear(&mut self) {
        self.length = 0;
    }
}

impl Default for StackView {
    fn default() -> Self {
        Self::new()
    }
}

fn bit_location(cell_id: CellId) -> (usize, u64) {
    let index = cell_id.index();
    let word = index / u64::BITS as usize;
    let bit = index % u64::BITS as usize;
    (word, 1_u64 << bit)
}

impl fmt::Display for FoldAxis {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vertical => formatter.write_str("vertical"),
            Self::Horizontal => formatter.write_str("horizontal"),
        }
    }
}

impl fmt::Display for FoldDirection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Left => formatter.write_str("left"),
            Self::Right => formatter.write_str("right"),
            Self::Up => formatter.write_str("up"),
            Self::Down => formatter.write_str("down"),
        }
    }
}

impl fmt::Display for StrokeAxis {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Horizontal => formatter.write_str("horizontal"),
            Self::Vertical => formatter.write_str("vertical"),
        }
    }
}

impl fmt::Display for BrushRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dot => formatter.write_str("dot"),
            Self::Line { axis, length } => write!(formatter, "{length}-cell {axis} line"),
        }
    }
}
