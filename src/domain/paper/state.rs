//! Canonical paper state, atomic actions, and complete undo snapshots.

use crate::domain::paper::{
    ActionCount, CellId, Coordinate, Dimensions, Face, Fold, FoldAxis, FoldCount, FoldDirection,
    INK_WORDS, InkComparison, InkPattern, Layer, LineStroke, MAX_PHYSICAL_CELLS, Orientation,
    PaperAction, PaperError, PaperSpec, PhysicalCell, StackView, StrokeCount,
};

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
                moving_cell_count += 1;
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
            counts[destination_index] += 1;
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
    /// Panics if the retained dimensions, budgets, or physical-cell collection
    /// violate the paper's validated settings.
    pub fn reset(&mut self) {
        for (index, cell) in self.cells.iter_mut().enumerate() {
            let cell_id = CellId::from_index(index);
            cell.coordinate = self
                .dimensions
                .original_coordinate(cell_id)
                .expect("a retained cell identity must belong to the paper");
            cell.layer = Layer::bottom();
            cell.face = Face::Front;
            cell.orientation = Orientation::North;
        }
        self.ink = InkPattern::empty(self.dimensions);
        self.fold_count = FoldCount(0);
        self.stroke_count = StrokeCount(0);
        self.action_count = ActionCount(0);
        self.history.clear();
        self.assert_invariants();
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

        let mut stack_counts = [0_u8; MAX_PHYSICAL_CELLS];
        for cell in &self.cells {
            self.dimensions
                .validate_coordinate(cell.coordinate)
                .expect("a canonical cell coordinate must remain inside the paper");
            let coordinate_index = self.dimensions.coordinate_index(cell.coordinate);
            stack_counts[coordinate_index] += 1;
        }

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
