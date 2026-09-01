use std::collections::BTreeMap;
use std::hint::black_box;
use std::mem::{self, size_of};
use std::process::ExitCode;
use std::time::Instant;

use orifude::domain::paper::{
    ActionCount, BrushRule, CellId, Dimensions, Face, Fold, FoldAxis, FoldCount, FoldDirection,
    InkPattern, MAX_ACTIONS, MAX_FOLD_ACTIONS, MAX_PHYSICAL_CELLS, MAX_STROKE_ACTIONS, Orientation,
    Paper, PaperAction, PaperSpec, PaperStateKey, PhysicalCell, StackView, StrokeCount,
};
use orifude::domain::puzzle::{MAX_ALLOWED_FOLDS, MAX_BRUSH_RULES, MAX_ID_BYTES};
use orifude::domain::replay::Replay;

const SAMPLE_COUNT: usize = 25;
const CLONE_ITERATIONS: usize = 2_000;
const LOOKUP_ITERATIONS: usize = 20_000;
const FOLD_ITERATIONS: usize = 200;
const MAX_SPEC: PaperSpec =
    PaperSpec::new(12, 12, MAX_FOLD_ACTIONS, MAX_STROKE_ACTIONS, MAX_ACTIONS);
const LEFT: Fold = Fold::new(FoldDirection::Left, 6);
const UP: Fold = Fold::new(FoldDirection::Up, 6);

type Position = (u8, u8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MapCell {
    cell_id: CellId,
    face: Face,
    orientation: Orientation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CoordinateMapPaper {
    dimensions: Dimensions,
    stacks: BTreeMap<Position, Vec<MapCell>>,
    ink: InkPattern,
    fold_count: FoldCount,
    stroke_count: StrokeCount,
    action_count: ActionCount,
}

impl CoordinateMapPaper {
    fn from_dense(paper: &Paper) -> Self {
        let dimensions = paper.dimensions();
        let mut stacks = BTreeMap::new();
        let mut stack = StackView::new();

        for row in 0..dimensions.height().get() {
            for column in 0..dimensions.width().get() {
                let coordinate = dimensions
                    .coordinate(row, column)
                    .expect("an enumerated coordinate must be valid");
                paper
                    .stack_at(coordinate, &mut stack)
                    .expect("an enumerated stack must be readable");
                if stack.is_empty() {
                    continue;
                }

                let cells = stack
                    .cell_ids()
                    .iter()
                    .map(|&cell_id| {
                        let cell = paper
                            .physical_cell(cell_id)
                            .expect("a stack identity must resolve");
                        MapCell {
                            cell_id,
                            face: cell.face(),
                            orientation: cell.orientation(),
                        }
                    })
                    .collect();
                let previous = stacks.insert((row, column), cells);
                assert!(previous.is_none());
            }
        }

        let mapped = Self {
            dimensions,
            stacks,
            ink: paper.ink(),
            fold_count: paper.fold_count(),
            stroke_count: paper.stroke_count(),
            action_count: paper.action_count(),
        };
        mapped.assert_invariants();
        mapped
    }

    fn fold(&mut self, fold: Fold) {
        let mut stationary = BTreeMap::<Position, Vec<MapCell>>::new();
        let mut moving = BTreeMap::<Position, Vec<MapCell>>::new();

        for (position, mut stack) in mem::take(&mut self.stacks) {
            if is_moving(position, fold) {
                let destination = reflect(position, fold);
                stack.reverse();
                for cell in &mut stack {
                    cell.face = flip_face(cell.face);
                    cell.orientation = fold_orientation(cell.orientation, fold.direction().axis());
                }
                let previous = moving.insert(destination, stack);
                assert!(previous.is_none());
            } else {
                let previous = stationary.insert(position, stack);
                assert!(previous.is_none());
            }
        }

        for (position, mut moved_stack) in moving {
            stationary
                .entry(position)
                .or_default()
                .append(&mut moved_stack);
        }
        self.stacks = stationary;
        self.fold_count = FoldCount::new(self.fold_count.get() + 1)
            .expect("the measured fold count must stay within its budget");
        self.action_count = ActionCount::new(self.action_count.get() + 1)
            .expect("the measured action count must stay within its budget");
        self.assert_invariants();
    }

    fn stack(&self, position: Position) -> &[MapCell] {
        self.stacks.get(&position).map_or(&[], Vec::as_slice)
    }

    fn heap_payload_lower_bound(&self) -> usize {
        let entry_payload = self.stacks.len() * (size_of::<Position>() + size_of::<Vec<MapCell>>());
        let stack_payload = self
            .stacks
            .values()
            .map(|stack| stack.capacity() * size_of::<MapCell>())
            .sum::<usize>();
        size_of::<Self>() + entry_payload + stack_payload
    }

    fn assert_matches(&self, paper: &Paper) {
        assert_eq!(self.dimensions, paper.dimensions());
        assert_eq!(self.ink, paper.ink());
        assert_eq!(self.fold_count, paper.fold_count());
        assert_eq!(self.stroke_count, paper.stroke_count());
        assert_eq!(self.action_count, paper.action_count());
        let mut dense_stack = StackView::new();

        for row in 0..self.dimensions.height().get() {
            for column in 0..self.dimensions.width().get() {
                let coordinate = self
                    .dimensions
                    .coordinate(row, column)
                    .expect("an enumerated coordinate must be valid");
                paper
                    .stack_at(coordinate, &mut dense_stack)
                    .expect("an enumerated stack must be readable");
                let mapped_stack = self.stack((row, column));
                assert_eq!(mapped_stack.len(), dense_stack.len());

                for (&cell_id, mapped_cell) in dense_stack.cell_ids().iter().zip(mapped_stack) {
                    let dense_cell = paper
                        .physical_cell(cell_id)
                        .expect("a dense identity must resolve");
                    assert_eq!(mapped_cell.cell_id, cell_id);
                    assert_eq!(mapped_cell.face, dense_cell.face());
                    assert_eq!(mapped_cell.orientation, dense_cell.orientation());
                }
            }
        }
    }

    fn assert_invariants(&self) {
        let mut identities = [false; MAX_PHYSICAL_CELLS];
        let mut cell_count = 0_usize;
        for (&(row, column), stack) in &self.stacks {
            assert!(row < self.dimensions.height().get());
            assert!(column < self.dimensions.width().get());
            assert!(!stack.is_empty());
            for cell in stack {
                assert!(cell.cell_id.index() < self.dimensions.cell_count());
                assert!(!identities[cell.cell_id.index()]);
                identities[cell.cell_id.index()] = true;
                cell_count += 1;
            }
        }
        assert_eq!(cell_count, self.dimensions.cell_count());
        assert!(identities[..cell_count].iter().all(|seen| *seen));
    }
}

#[derive(Clone, Copy)]
struct Nanoseconds {
    median: u128,
    p95: u128,
    max: u128,
}

fn main() -> ExitCode {
    if cfg!(debug_assertions) {
        eprintln!("error: paper measurements must run with --release");
        return ExitCode::FAILURE;
    }

    let dense = Paper::new(MAX_SPEC).expect("the measurement paper must be valid");
    let mapped = CoordinateMapPaper::from_dense(&dense);
    verify_fold_equivalence(&dense, &mapped);

    let positions = all_positions(dense.dimensions());
    let (dense_clone, map_clone) = measure_pair(
        CLONE_ITERATIONS,
        || {
            let copy = black_box(dense.clone());
            u64::from(copy.action_count().get()) + copy.dimensions().cell_count() as u64
        },
        || {
            let copy = black_box(mapped.clone());
            copy.stacks.len() as u64
        },
    );

    let mut dense_stack = StackView::new();
    let mut dense_position = 0_usize;
    let mut map_position = 0_usize;
    let (dense_lookup, map_lookup) = measure_pair(
        LOOKUP_ITERATIONS,
        || {
            let coordinate = positions[dense_position % positions.len()];
            dense_position += 1;
            dense
                .stack_at(coordinate, &mut dense_stack)
                .expect("a measured coordinate must be valid");
            dense_stack.len() as u64
        },
        || {
            let coordinate = positions[map_position % positions.len()];
            map_position += 1;
            mapped
                .stack((coordinate.row().get(), coordinate.column().get()))
                .len() as u64
        },
    );

    let (dense_fold, map_fold) = measure_pair(
        FOLD_ITERATIONS,
        || {
            let mut state = dense.clone();
            state.fold(LEFT).expect("the measured left fold must work");
            state.fold(UP).expect("the measured up fold must work");
            u64::from(state.action_count().get())
        },
        || {
            let mut state = mapped.clone();
            let first_snapshot = state.clone();
            state.fold(LEFT);
            let second_snapshot = state.clone();
            state.fold(UP);
            black_box(first_snapshot);
            black_box(second_snapshot);
            state.stacks.len() as u64
        },
    );

    print_size_estimates(&dense, &mapped);
    println!("\nTiming method: {SAMPLE_COUNT} alternating sample blocks in one release process.");
    println!("Each value is nanoseconds per operation. This is a directional microbenchmark.");
    println!("operation                         median       p95       max");
    print_distribution("dense snapshot clone", dense_clone);
    print_distribution("coordinate-map snapshot clone", map_clone);
    print_distribution("dense derived stack lookup", dense_lookup);
    print_distribution("coordinate-map stack lookup", map_lookup);
    print_distribution("dense clone + two folds", dense_fold);
    print_distribution("map clone + two folds", map_fold);
    ExitCode::SUCCESS
}

fn print_size_estimates(dense: &Paper, mapped: &CoordinateMapPaper) {
    let cell_count = dense.dimensions().cell_count();
    let dense_cell_payload = cell_count * size_of::<PhysicalCell>();
    let snapshot_lower_bound = size_of::<Vec<PhysicalCell>>()
        + dense_cell_payload
        + size_of::<InkPattern>()
        + size_of::<FoldCount>()
        + size_of::<StrokeCount>()
        + size_of::<ActionCount>();
    let solver_key_lower_bound = size_of::<PaperStateKey>() + dense_cell_payload;
    let action_size = size_of::<PaperAction>();
    let replay_lower_bound = size_of::<Replay>()
        + MAX_ACTIONS as usize * action_size
        + MAX_ID_BYTES * 2
        + MAX_ALLOWED_FOLDS * size_of::<Fold>()
        + MAX_BRUSH_RULES * size_of::<BrushRule>();
    let history_lower_bound = MAX_ACTIONS as usize * (snapshot_lower_bound + action_size);

    println!("Paper representation sizes on this target:");
    println!(
        "  physical cell:                 {:>8} bytes",
        size_of::<PhysicalCell>()
    );
    println!("  dense 144-cell payload:        {dense_cell_payload:>8} bytes");
    println!("  snapshot named payloads:       {snapshot_lower_bound:>8} bytes");
    println!("  solver state key lower bound:  {solver_key_lower_bound:>8} bytes");
    println!("  replay action value:           {action_size:>8} bytes");
    println!("  maximum 64-action replay:      {replay_lower_bound:>8} bytes");
    println!("  64 history entries:            {history_lower_bound:>8} bytes");
    println!(
        "  coordinate-map lower bound:    {:>8} bytes",
        mapped.heap_payload_lower_bound()
    );
    println!(
        "  Paper inline value:             {:>8} bytes",
        size_of::<Paper>()
    );
    println!("Map figures exclude B-tree node and allocator overhead.");
}

fn measure_pair(
    iterations: usize,
    mut dense_operation: impl FnMut() -> u64,
    mut map_operation: impl FnMut() -> u64,
) -> (Nanoseconds, Nanoseconds) {
    assert!(iterations > 0);
    let mut dense_samples = Vec::with_capacity(SAMPLE_COUNT);
    let mut map_samples = Vec::with_capacity(SAMPLE_COUNT);

    for sample in 0..SAMPLE_COUNT {
        if sample.is_multiple_of(2) {
            dense_samples.push(time_block(iterations, &mut dense_operation));
            map_samples.push(time_block(iterations, &mut map_operation));
        } else {
            map_samples.push(time_block(iterations, &mut map_operation));
            dense_samples.push(time_block(iterations, &mut dense_operation));
        }
    }
    (summarize(dense_samples), summarize(map_samples))
}

fn time_block(iterations: usize, operation: &mut impl FnMut() -> u64) -> u128 {
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(operation()));
    }
    black_box(checksum);
    started.elapsed().as_nanos() / iterations as u128
}

fn summarize(mut samples: Vec<u128>) -> Nanoseconds {
    assert_eq!(samples.len(), SAMPLE_COUNT);
    samples.sort_unstable();
    Nanoseconds {
        median: samples[SAMPLE_COUNT / 2],
        p95: samples[(SAMPLE_COUNT * 95).div_ceil(100) - 1],
        max: samples[SAMPLE_COUNT - 1],
    }
}

fn print_distribution(name: &str, distribution: Nanoseconds) {
    println!(
        "{name:<32} {:>9} {:>9} {:>9}",
        distribution.median, distribution.p95, distribution.max
    );
}

fn all_positions(dimensions: Dimensions) -> Vec<orifude::domain::paper::Coordinate> {
    let mut positions = Vec::with_capacity(dimensions.cell_count());
    for row in 0..dimensions.height().get() {
        for column in 0..dimensions.width().get() {
            positions.push(
                dimensions
                    .coordinate(row, column)
                    .expect("an enumerated coordinate must be valid"),
            );
        }
    }
    positions
}

fn verify_fold_equivalence(dense: &Paper, mapped: &CoordinateMapPaper) {
    let mut dense = dense.clone();
    let mut mapped = mapped.clone();
    dense
        .fold(LEFT)
        .expect("the comparison left fold must work");
    mapped.fold(LEFT);
    mapped.assert_matches(&dense);
    dense.fold(UP).expect("the comparison up fold must work");
    mapped.fold(UP);
    mapped.assert_matches(&dense);
}

fn is_moving((row, column): Position, fold: Fold) -> bool {
    match fold.direction() {
        FoldDirection::Left => column >= fold.crease(),
        FoldDirection::Right => column < fold.crease(),
        FoldDirection::Up => row >= fold.crease(),
        FoldDirection::Down => row < fold.crease(),
    }
}

fn reflect((row, column): Position, fold: Fold) -> Position {
    match fold.direction().axis() {
        FoldAxis::Vertical => (row, reflect_index(column, fold.crease())),
        FoldAxis::Horizontal => (reflect_index(row, fold.crease()), column),
    }
}

fn reflect_index(index: u8, crease: u8) -> u8 {
    let reflected = usize::from(crease) * 2 - 1 - usize::from(index);
    u8::try_from(reflected).expect("the measured half-fold must remain inside the paper")
}

const fn flip_face(face: Face) -> Face {
    match face {
        Face::Front => Face::Back,
        Face::Back => Face::Front,
    }
}

const fn fold_orientation(orientation: Orientation, axis: FoldAxis) -> Orientation {
    match (axis, orientation) {
        (FoldAxis::Vertical, Orientation::North | Orientation::South)
        | (FoldAxis::Horizontal, Orientation::East | Orientation::West) => orientation,
        (FoldAxis::Vertical, Orientation::East) => Orientation::West,
        (FoldAxis::Vertical, Orientation::West) => Orientation::East,
        (FoldAxis::Horizontal, Orientation::North) => Orientation::South,
        (FoldAxis::Horizontal, Orientation::South) => Orientation::North,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_map_matches_dense_state_for_both_fold_axes() {
        let dense = Paper::new(MAX_SPEC).expect("the paper should be valid");
        let mapped = CoordinateMapPaper::from_dense(&dense);

        verify_fold_equivalence(&dense, &mapped);
    }

    #[test]
    fn coordinate_map_matches_every_fold_direction_and_valid_pair() {
        let directions = [
            FoldDirection::Left,
            FoldDirection::Right,
            FoldDirection::Up,
            FoldDirection::Down,
        ];

        for first_direction in directions {
            for second_direction in directions {
                let mut dense = Paper::new(MAX_SPEC).expect("the paper should be valid");
                let mut mapped = CoordinateMapPaper::from_dense(&dense);
                let first = Fold::new(first_direction, 6);
                dense.fold(first).expect("the first fold should be valid");
                mapped.fold(first);
                mapped.assert_matches(&dense);

                let second = Fold::new(second_direction, 6);
                if dense.fold(second).is_ok() {
                    mapped.fold(second);
                    mapped.assert_matches(&dense);
                }
            }
        }
    }
}
