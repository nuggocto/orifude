use std::error::Error;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PaperBudget {
    folds: FoldCount,
    strokes: StrokeCount,
    actions: ActionCount,
}

impl PaperBudget {
    const fn from_spec(spec: PaperSpec) -> Result<Self, PaperError> {
        let folds = match FoldCount::new(spec.fold_budget) {
            Ok(folds) => folds,
            Err(error) => return Err(error),
        };
        let strokes = match StrokeCount::new(spec.stroke_budget) {
            Ok(strokes) => strokes,
            Err(error) => return Err(error),
        };
        let actions = match ActionCount::new(spec.action_budget) {
            Ok(actions) => actions,
            Err(error) => return Err(error),
        };
        Ok(Self {
            folds,
            strokes,
            actions,
        })
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct Snapshot {
    cells: Vec<PhysicalCell>,
    ink: InkPattern,
    fold_count: FoldCount,
    stroke_count: StrokeCount,
    action_count: ActionCount,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HistoryEntry {
    before: Snapshot,
    action: PaperAction,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PaperStateKey {
    dimensions: Dimensions,
    cells: Box<[PhysicalCell]>,
    ink: InkPattern,
    fold_count: FoldCount,
    stroke_count: StrokeCount,
}

impl PaperStateKey {
    /// Returns a stable non-cryptographic hash of the canonical state.
    ///
    /// The value is suitable for deterministic solver bookkeeping. Equality
    /// must still resolve hash collisions.
    #[must_use]
    pub fn stable_hash(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        hash_byte(&mut hash, self.dimensions.width.get());
        hash_byte(&mut hash, self.dimensions.height.get());
        for cell in &self.cells {
            hash_byte(&mut hash, cell.coordinate.row.get());
            hash_byte(&mut hash, cell.coordinate.column.get());
            hash_byte(&mut hash, cell.layer.get());
            hash_byte(&mut hash, face_code(cell.face));
            hash_byte(&mut hash, orientation_code(cell.orientation));
        }
        for word in self.ink.words {
            for byte in word.to_le_bytes() {
                hash_byte(&mut hash, byte);
            }
        }
        hash_byte(&mut hash, self.fold_count.get());
        hash_byte(&mut hash, self.stroke_count.get());
        hash
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Paper {
    dimensions: Dimensions,
    budget: PaperBudget,
    cells: Vec<PhysicalCell>,
    ink: InkPattern,
    fold_count: FoldCount,
    stroke_count: StrokeCount,
    action_count: ActionCount,
    history: Vec<HistoryEntry>,
}

impl Paper {
    /// Constructs a validated rectangular paper in row-major identity order.
    ///
    /// # Errors
    ///
    /// Returns a typed validation error for malformed dimensions or budgets.
    ///
    /// # Panics
    ///
    /// Panics only if construction violates the canonical cell identity or
    /// stack invariants.
    pub fn new(spec: PaperSpec) -> Result<Self, PaperError> {
        let dimensions = Dimensions::new(spec.width, spec.height)?;
        let budget = PaperBudget::from_spec(spec)?;
        let cell_count = dimensions.cell_count();
        let mut cells = Vec::with_capacity(cell_count);

        for index in 0..cell_count {
            let cell_id = CellId::from_index(index);
            let coordinate = dimensions
                .original_coordinate(cell_id)
                .expect("a constructed row-major identity must belong to the paper");
            cells.push(PhysicalCell {
                coordinate,
                layer: Layer::bottom(),
                face: Face::Front,
                orientation: Orientation::North,
            });
        }

        let paper = Self {
            dimensions,
            budget,
            cells,
            ink: InkPattern::empty(dimensions),
            fold_count: FoldCount(0),
            stroke_count: StrokeCount(0),
            action_count: ActionCount(0),
            history: Vec::with_capacity(usize::from(budget.actions.get())),
        };
        paper.assert_invariants();
        Ok(paper)
    }

    #[must_use]
    pub const fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    #[must_use]
    pub const fn fold_count(&self) -> FoldCount {
        self.fold_count
    }

    #[must_use]
    pub const fn stroke_count(&self) -> StrokeCount {
        self.stroke_count
    }

    #[must_use]
    pub const fn action_count(&self) -> ActionCount {
        self.action_count
    }

    #[must_use]
    pub fn history_len(&self) -> ActionCount {
        ActionCount::from_history_len(self.history.len())
    }

    #[must_use]
    pub const fn ink(&self) -> InkPattern {
        self.ink
    }

    #[must_use]
    pub fn state_key(&self) -> PaperStateKey {
        PaperStateKey {
            dimensions: self.dimensions,
            cells: self.cells.clone().into_boxed_slice(),
            ink: self.ink,
            fold_count: self.fold_count,
            stroke_count: self.stroke_count,
        }
    }

    pub fn actions(&self) -> impl Iterator<Item = PaperAction> + '_ {
        self.history.iter().map(|entry| entry.action)
    }

    pub fn cell_ids(&self) -> impl Iterator<Item = CellId> + '_ {
        (0..self.cells.len()).map(CellId::from_index)
    }

    #[must_use]
    pub fn physical_cell(&self, cell_id: CellId) -> Option<PhysicalCell> {
        self.cells.get(cell_id.index()).copied()
    }

    /// Derives one bottom-to-top stack view into caller-owned scratch storage.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::CoordinateOutsidePaper`] when the coordinate does
    /// not belong to this paper.
    ///
    /// # Panics
    ///
    /// Panics if the canonical state contains a duplicate or out-of-range
    /// layer. Such a state is a programmer error and cannot be constructed
    /// through the public API.
    pub fn stack_at(
        &self,
        coordinate: Coordinate,
        stack: &mut StackView,
    ) -> Result<(), PaperError> {
        self.dimensions.validate_coordinate(coordinate)?;
        stack.clear();

        let count = self
            .cells
            .iter()
            .filter(|cell| cell.coordinate == coordinate)
            .count();
        let count = u8::try_from(count).expect("a stack count must fit in u8");
        let mut occupied_layers = [false; MAX_PHYSICAL_CELLS];

        for (index, cell) in self.cells.iter().enumerate() {
            if cell.coordinate != coordinate {
                continue;
            }

            let layer = usize::from(cell.layer.get());
            assert!(layer < usize::from(count));
            assert!(!occupied_layers[layer]);
            occupied_layers[layer] = true;
            stack.cell_ids[layer] = CellId::from_index(index);
        }

        stack.length = count;
        Ok(())
    }

    /// Applies a validated paper action.
    ///
    /// # Errors
    ///
    /// Returns a typed operational error without changing the paper when the
    /// action is illegal or exceeds a budget.
    ///
    /// # Panics
    ///
    /// Panics if the canonical state violates cell conservation, identity, or
    /// total layer order. Public actions preserve those invariants.
    pub fn apply(&mut self, action: PaperAction) -> Result<(), PaperError> {
        match action {
            PaperAction::Fold(fold) => self.fold(fold),
            PaperAction::Dot(coordinate) => self.stamp_dot(coordinate),
            PaperAction::Line(line) => self.stamp_line(line),
        }
    }

    /// Applies one vertical or horizontal fold on a cell boundary.
    ///
    /// # Errors
    ///
    /// Returns a typed operational error without mutation for an invalid
    /// crease, an empty moving side, or an exhausted budget.
    ///
    /// # Panics
    ///
    /// Panics if the canonical state violates cell conservation, identity, or
    /// total layer order. Public actions preserve those invariants.
    pub fn fold(&mut self, fold: Fold) -> Result<(), PaperError> {
        self.assert_invariants();
        let axis = fold.direction.axis();
        let extent = match axis {
            FoldAxis::Vertical => self.dimensions.width.get(),
            FoldAxis::Horizontal => self.dimensions.height.get(),
        };
        validate_crease(axis, fold.crease, extent)?;
        self.validate_fold_budget()?;

        let mut stationary_counts = [0_u8; MAX_PHYSICAL_CELLS];
        let mut moving_counts = [0_u8; MAX_PHYSICAL_CELLS];
        let mut moving_cell_count = 0_usize;

        for cell in &self.cells {
            let moving = is_on_moving_side(cell.coordinate, fold);
            let destination = if moving {
                moving_cell_count = moving_cell_count
                    .checked_add(1)
                    .expect("the bounded cell count must not overflow");
                reflected_coordinate(cell.coordinate, fold, self.dimensions)?
            } else {
                cell.coordinate
            };
            self.dimensions.validate_coordinate(destination)?;
            let destination_index = self.dimensions.coordinate_index(destination);
            let counts = if moving {
                &mut moving_counts
            } else {
                &mut stationary_counts
            };
            counts[destination_index] = counts[destination_index]
                .checked_add(1)
                .expect("a bounded stack count must not overflow");
        }

        if moving_cell_count == 0 {
            return Err(PaperError::EmptyMovingSide {
                direction: fold.direction,
            });
        }

        self.remember(PaperAction::Fold(fold));
        for cell in &mut self.cells {
            if !is_on_moving_side(cell.coordinate, fold) {
                continue;
            }

            let destination = reflected_coordinate(cell.coordinate, fold, self.dimensions)
                .expect("a fold validated before mutation must stay inside the paper");
            let destination_index = self.dimensions.coordinate_index(destination);
            let reversed_layer = moving_counts[destination_index]
                .checked_sub(1)
                .and_then(|top| top.checked_sub(cell.layer.get()))
                .expect("a moved cell layer must belong to its source stack");
            let new_layer = stationary_counts[destination_index]
                .checked_add(reversed_layer)
                .expect("a combined stack must fit in the physical-cell limit");

            cell.coordinate = destination;
            cell.layer = Layer::from_index(new_layer);
            cell.face = cell.face.flipped();
            cell.orientation = cell.orientation.folded_across(axis);
        }
        self.fold_count = self.fold_count.increment();
        self.action_count = self.action_count.increment();
        self.assert_invariants();
        Ok(())
    }

    /// Applies a dot through every physical cell at one visible position.
    ///
    /// # Errors
    ///
    /// Returns a typed operational error without mutation when the coordinate
    /// is invalid or empty, or when a budget is exhausted.
    ///
    /// # Panics
    ///
    /// Panics if the canonical state violates cell conservation, identity, or
    /// total layer order. Public actions preserve those invariants.
    pub fn stamp_dot(&mut self, coordinate: Coordinate) -> Result<(), PaperError> {
        self.assert_invariants();
        self.dimensions.validate_coordinate(coordinate)?;
        self.validate_stroke_budget()?;

        let mut stack = StackView::new();
        self.stack_at(coordinate, &mut stack)?;
        if stack.is_empty() {
            return Err(PaperError::EmptyBrushPosition { coordinate });
        }

        self.remember(PaperAction::Dot(coordinate));
        for &cell_id in stack.cell_ids() {
            self.ink.insert(cell_id);
        }
        self.stroke_count = self.stroke_count.increment();
        self.action_count = self.action_count.increment();
        self.assert_invariants();
        Ok(())
    }

    /// Applies one inclusive horizontal or vertical line through occupied stacks.
    ///
    /// # Errors
    ///
    /// Returns a typed operational error without mutation when the line is
    /// diagonal, shorter than two cells, outside the paper, crosses an empty
    /// position, or exceeds a budget.
    pub fn stamp_line(&mut self, line: LineStroke) -> Result<(), PaperError> {
        self.assert_invariants();
        self.dimensions.validate_coordinate(line.start)?;
        self.dimensions.validate_coordinate(line.end)?;
        self.validate_stroke_budget()?;
        let (_, length) = line.axis_and_length()?;

        let mut ink = self.ink;
        let mut stack = StackView::new();
        for offset in 0..length {
            let coordinate = line.coordinate_at(offset);
            self.stack_at(coordinate, &mut stack)?;
            if stack.is_empty() {
                return Err(PaperError::EmptyBrushPosition { coordinate });
            }
            for &cell_id in stack.cell_ids() {
                ink.insert(cell_id);
            }
        }

        self.remember(PaperAction::Line(line));
        self.ink = ink;
        self.stroke_count = self.stroke_count.increment();
        self.action_count = self.action_count.increment();
        self.assert_invariants();
        Ok(())
    }

    /// Compares ink against every original physical cell.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::TargetDimensionsDiffer`] when the target belongs
    /// to another paper size.
    pub fn compare_ink(&self, target: InkPattern) -> Result<InkComparison, PaperError> {
        if self.dimensions != target.dimensions {
            return Err(PaperError::TargetDimensionsDiffer {
                paper: self.dimensions,
                target: target.dimensions,
            });
        }

        Ok(InkComparison {
            missing: target.difference(self.ink),
            extra: self.ink.difference(target),
        })
    }

    /// Restores the complete canonical state before the most recent action.
    ///
    /// # Errors
    ///
    /// Returns [`PaperError::NothingToUndo`] without mutation when no earlier
    /// action exists.
    ///
    /// # Panics
    ///
    /// Panics if the canonical state or a stored snapshot violates cell
    /// conservation, identity, or total layer order.
    pub fn undo(&mut self) -> Result<(), PaperError> {
        self.assert_invariants();
        let Some(entry) = self.history.pop() else {
            return Err(PaperError::NothingToUndo);
        };
        self.cells = entry.before.cells;
        self.ink = entry.before.ink;
        self.fold_count = entry.before.fold_count;
        self.stroke_count = entry.before.stroke_count;
        self.action_count = entry.before.action_count;
        self.assert_invariants();
        Ok(())
    }

    /// Restores the fresh uninked paper and clears successful action history.
    ///
    /// # Panics
    ///
    /// Panics only if the paper's already validated dimensions or budgets have
    /// become invalid, which is a programmer-error invariant.
    pub fn reset(&mut self) {
        let spec = PaperSpec::new(
            self.dimensions.width.get(),
            self.dimensions.height.get(),
            self.budget.folds.get(),
            self.budget.strokes.get(),
            self.budget.actions.get(),
        );
        *self = Self::new(spec).expect("canonical paper settings must remain valid");
    }

    fn validate_fold_budget(&self) -> Result<(), PaperError> {
        if self.fold_count >= self.budget.folds {
            return Err(PaperError::FoldBudgetExhausted {
                limit: self.budget.folds,
            });
        }
        self.validate_action_budget()
    }

    fn validate_stroke_budget(&self) -> Result<(), PaperError> {
        if self.stroke_count >= self.budget.strokes {
            return Err(PaperError::StrokeBudgetExhausted {
                limit: self.budget.strokes,
            });
        }
        self.validate_action_budget()
    }

    fn validate_action_budget(&self) -> Result<(), PaperError> {
        if self.action_count >= self.budget.actions {
            return Err(PaperError::ActionBudgetExhausted {
                limit: self.budget.actions,
            });
        }
        Ok(())
    }

    fn remember(&mut self, action: PaperAction) {
        assert!(self.history.len() < usize::from(self.budget.actions.get()));
        self.history.push(HistoryEntry {
            before: Snapshot {
                cells: self.cells.clone(),
                ink: self.ink,
                fold_count: self.fold_count,
                stroke_count: self.stroke_count,
                action_count: self.action_count,
            },
            action,
        });
    }

    fn assert_invariants(&self) {
        assert_eq!(self.cells.len(), self.dimensions.cell_count());
        assert!(self.cells.len() <= MAX_PHYSICAL_CELLS);
        assert_eq!(self.history.len(), self.action_count.get() as usize);
        assert!(self.fold_count <= self.budget.folds);
        assert!(self.stroke_count <= self.budget.strokes);
        assert!(self.action_count <= self.budget.actions);
        assert_eq!(
            self.fold_count.get() + self.stroke_count.get(),
            self.action_count.get()
        );
        assert_eq!(self.ink.dimensions, self.dimensions);

        let mut identities = [false; MAX_PHYSICAL_CELLS];
        let mut stack_counts = [0_u8; MAX_PHYSICAL_CELLS];
        for index in 0..self.cells.len() {
            let cell_id = CellId::from_index(index);
            assert!(!identities[cell_id.index()]);
            identities[cell_id.index()] = true;
            self.dimensions
                .validate_coordinate(self.cells[index].coordinate)
                .expect("a canonical cell coordinate must remain inside the paper");
            let coordinate_index = self
                .dimensions
                .coordinate_index(self.cells[index].coordinate);
            stack_counts[coordinate_index] = stack_counts[coordinate_index]
                .checked_add(1)
                .expect("a canonical stack count must fit in u8");
        }
        assert!(identities[..self.cells.len()].iter().all(|seen| *seen));

        let mut occupied_layers = [[0_u64; INK_WORDS]; MAX_PHYSICAL_CELLS];
        for cell in &self.cells {
            let coordinate_index = self.dimensions.coordinate_index(cell.coordinate);
            let layer = usize::from(cell.layer.get());
            assert!(layer < usize::from(stack_counts[coordinate_index]));
            let word = layer / u64::BITS as usize;
            let mask = 1_u64 << (layer % u64::BITS as usize);
            assert_eq!(occupied_layers[coordinate_index][word] & mask, 0);
            occupied_layers[coordinate_index][word] |= mask;
        }
    }
}

fn bit_location(cell_id: CellId) -> (usize, u64) {
    let index = cell_id.index();
    let word = index / u64::BITS as usize;
    let bit = index % u64::BITS as usize;
    (word, 1_u64 << bit)
}

fn validate_crease(axis: FoldAxis, crease: u8, extent: u8) -> Result<(), PaperError> {
    if crease == 0 || crease >= extent {
        return Err(PaperError::CreaseOutsidePaper {
            axis,
            crease,
            extent,
        });
    }
    Ok(())
}

fn is_on_moving_side(coordinate: Coordinate, fold: Fold) -> bool {
    match fold.direction {
        FoldDirection::Left => coordinate.column.get() >= fold.crease,
        FoldDirection::Right => coordinate.column.get() < fold.crease,
        FoldDirection::Up => coordinate.row.get() >= fold.crease,
        FoldDirection::Down => coordinate.row.get() < fold.crease,
    }
}

fn reflected_coordinate(
    coordinate: Coordinate,
    fold: Fold,
    dimensions: Dimensions,
) -> Result<Coordinate, PaperError> {
    match fold.direction.axis() {
        FoldAxis::Vertical => {
            let column = reflect_index(
                coordinate.column.get(),
                fold.crease,
                dimensions.width.get(),
                fold.direction,
            )?;
            dimensions.coordinate(coordinate.row.get(), column)
        }
        FoldAxis::Horizontal => {
            let row = reflect_index(
                coordinate.row.get(),
                fold.crease,
                dimensions.height.get(),
                fold.direction,
            )?;
            dimensions.coordinate(row, coordinate.column.get())
        }
    }
}

fn reflect_index(
    index: u8,
    crease: u8,
    extent: u8,
    direction: FoldDirection,
) -> Result<u8, PaperError> {
    let reflected = i16::from(crease) * 2 - 1 - i16::from(index);
    if reflected < 0 || reflected >= i16::from(extent) {
        return Err(PaperError::FoldLeavesPaper {
            direction,
            crease,
            index,
            extent,
        });
    }
    u8::try_from(reflected).map_err(|_| PaperError::FoldLeavesPaper {
        direction,
        crease,
        index,
        extent,
    })
}

fn hash_byte(hash: &mut u64, byte: u8) {
    *hash ^= u64::from(byte);
    *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
}

const fn face_code(face: Face) -> u8 {
    match face {
        Face::Front => 0,
        Face::Back => 1,
    }
}

const fn orientation_code(orientation: Orientation) -> u8 {
    match orientation {
        Orientation::North => 0,
        Orientation::East => 1,
        Orientation::South => 2,
        Orientation::West => 3,
    }
}

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
