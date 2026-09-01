//! Bounded deterministic puzzle search.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet, TryReserveError};
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use crate::domain::attempt::Attempt;
use crate::domain::paper::{
    BrushRule, LineStroke, MAX_FOLD_ACTIONS, MAX_STROKE_ACTIONS, PaperAction, PaperStateKey,
    PhysicalCell, StrokeAxis,
};
use crate::domain::puzzle::Puzzle;
use crate::domain::replay::Replay;
use crate::domain::score::Score;

pub const MAX_SOLVER_VISITED_STATES: usize = 250_000;
pub const MAX_SOLVER_MEMORY_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_SOLVER_DEPTH: u8 = MAX_FOLD_ACTIONS + MAX_STROKE_ACTIONS;
pub const MAX_CANDIDATE_ACTIONS: usize = 1_772;

const RETAINED_STATE_MARGIN_BYTES: usize = 512;
const SEARCH_BASE_MARGIN_BYTES: usize = 64 * 1024;
const NO_NODE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolverLimits {
    visited_states: usize,
    memory_bytes: usize,
    depth: u8,
}

impl SolverLimits {
    #[must_use]
    pub const fn new(max_visited_states: usize, max_memory_bytes: usize, max_depth: u8) -> Self {
        Self {
            visited_states: max_visited_states,
            memory_bytes: max_memory_bytes,
            depth: max_depth,
        }
    }

    #[must_use]
    pub const fn max_visited_states(self) -> usize {
        self.visited_states
    }

    #[must_use]
    pub const fn max_memory_bytes(self) -> usize {
        self.memory_bytes
    }

    #[must_use]
    pub const fn max_depth(self) -> u8 {
        self.depth
    }

    /// Validates all independent solver resource limits.
    ///
    /// # Errors
    ///
    /// Returns the first zero or engine-wide maximum violation.
    pub const fn validate(self) -> Result<(), InvalidSolverInput> {
        validate_limits(self)
    }
}

impl Default for SolverLimits {
    fn default() -> Self {
        Self::new(
            MAX_SOLVER_VISITED_STATES,
            MAX_SOLVER_MEMORY_BYTES,
            MAX_SOLVER_DEPTH,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidSolverInput {
    VisitedStateLimit { found: usize, maximum: usize },
    MemoryLimit { found: usize, maximum: usize },
    DepthLimit { found: u8, maximum: u8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExhaustionReason {
    VisitedStates,
    Memory,
    Depth,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SolveStats {
    visited_states: usize,
    expanded_states: usize,
    checked_actions: usize,
    retained_memory_bytes: usize,
    maximum_frontier: usize,
    deepest_state: u8,
}

impl SolveStats {
    #[must_use]
    pub const fn visited_states(self) -> usize {
        self.visited_states
    }

    #[must_use]
    pub const fn expanded_states(self) -> usize {
        self.expanded_states
    }

    #[must_use]
    pub const fn checked_actions(self) -> usize {
        self.checked_actions
    }

    /// Returns the conservative retained-allocation charge used by the solver.
    #[must_use]
    pub const fn retained_memory_bytes(self) -> usize {
        self.retained_memory_bytes
    }

    #[must_use]
    pub const fn maximum_frontier(self) -> usize {
        self.maximum_frontier
    }

    #[must_use]
    pub const fn deepest_state(self) -> u8 {
        self.deepest_state
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Solution {
    replay: Replay,
    score: Score,
    stats: SolveStats,
}

impl Solution {
    #[must_use]
    pub const fn replay(&self) -> &Replay {
        &self.replay
    }

    #[must_use]
    pub const fn score(&self) -> Score {
        self.score
    }

    #[must_use]
    pub const fn stats(&self) -> SolveStats {
        self.stats
    }

    pub(crate) fn with_replay(self, replay: Replay) -> Self {
        Self { replay, ..self }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SolveOutcome {
    Solved(Solution),
    Unsolved(SolveStats),
    Exhausted {
        reason: ExhaustionReason,
        stats: SolveStats,
    },
    Cancelled(SolveStats),
    Invalid(InvalidSolverInput),
}

pub trait Cancellation {
    fn is_cancelled(&self) -> bool;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NeverCancel;

impl Cancellation for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[derive(Debug, Default)]
pub struct CancellationFlag {
    cancelled: AtomicBool,
}

impl CancellationFlag {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, AtomicOrdering::Release);
    }
}

impl Cancellation for CancellationFlag {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(AtomicOrdering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Solver;

impl Solver {
    /// Returns the conservative retained-memory charge for one search state.
    ///
    /// The charge includes the canonical key payload, parent node, frontier
    /// entry, and margin for collection control bytes and allocator overhead.
    #[must_use]
    pub fn conservative_state_memory_bytes(puzzle: &Puzzle) -> usize {
        state_memory_charge(puzzle)
    }

    #[must_use]
    pub fn solve<C: Cancellation + ?Sized>(
        puzzle: &Puzzle,
        limits: SolverLimits,
        cancellation: &C,
    ) -> SolveOutcome {
        if let Err(error) = limits.validate() {
            return SolveOutcome::Invalid(error);
        }
        if cancellation.is_cancelled() {
            return SolveOutcome::Cancelled(SolveStats::default());
        }

        let base_memory = search_base_memory(puzzle, limits, catalog_capacity(puzzle));
        if base_memory > limits.memory_bytes {
            return SolveOutcome::Exhausted {
                reason: ExhaustionReason::Memory,
                stats: SolveStats::default(),
            };
        }
        let catalog = match action_catalog(puzzle) {
            Ok(catalog) => catalog,
            Err(CatalogError::Allocation) => {
                return SolveOutcome::Exhausted {
                    reason: ExhaustionReason::Memory,
                    stats: SolveStats::default(),
                };
            }
        };
        Search::new(puzzle, limits, cancellation, catalog, base_memory).run()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SearchNode {
    parent: u32,
    action: Option<PaperAction>,
    depth: u8,
    score: Score,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct QueueEntry {
    score: Score,
    order: u32,
    node: u32,
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .cmp(&self.score)
            .then_with(|| other.order.cmp(&self.order))
    }
}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct Search<'a, C: Cancellation + ?Sized> {
    puzzle: &'a Puzzle,
    limits: SolverLimits,
    cancellation: &'a C,
    catalog: Box<[PaperAction]>,
    visited: HashSet<PaperStateKey>,
    nodes: Vec<SearchNode>,
    frontier: BinaryHeap<QueueEntry>,
    work_attempt: Attempt,
    stats: SolveStats,
    memory_per_state: usize,
    depth_was_limited: bool,
}

impl<'a, C: Cancellation + ?Sized> Search<'a, C> {
    fn new(
        puzzle: &'a Puzzle,
        limits: SolverLimits,
        cancellation: &'a C,
        catalog: Box<[PaperAction]>,
        base_memory: usize,
    ) -> Self {
        let memory_per_state = state_memory_charge(puzzle);

        Self {
            puzzle,
            limits,
            cancellation,
            catalog,
            visited: HashSet::new(),
            nodes: Vec::new(),
            frontier: BinaryHeap::new(),
            work_attempt: puzzle.start(),
            stats: SolveStats {
                retained_memory_bytes: base_memory,
                ..SolveStats::default()
            },
            memory_per_state,
            depth_was_limited: false,
        }
    }

    fn run(mut self) -> SolveOutcome {
        let root_key = self.work_attempt.state_key();
        let root_score = self.work_attempt.result().score();
        let root = SearchNode {
            parent: NO_NODE,
            action: None,
            depth: 0,
            score: root_score,
        };
        if let Err(reason) = self.retain_state(root_key, root) {
            return self.exhausted(reason);
        }

        while let Some(entry) = self.frontier.pop() {
            if self.cancellation.is_cancelled() {
                return SolveOutcome::Cancelled(self.stats);
            }
            self.restore(entry.node);
            let result = self.work_attempt.result();
            if result.is_success() {
                return SolveOutcome::Solved(self.verified_solution(result.score()));
            }

            let node = self.nodes[entry.node as usize];
            assert_eq!(node.score, entry.score);
            if node.depth >= self.limits.depth || node.depth >= self.maximum_puzzle_depth() {
                if node.depth < self.maximum_puzzle_depth() {
                    self.depth_was_limited = true;
                }
                continue;
            }

            self.stats.expanded_states = self
                .stats
                .expanded_states
                .checked_add(1)
                .expect("the visited-state bound must cap expanded states");
            for catalog_index in 0..self.catalog.len() {
                if self.cancellation.is_cancelled() {
                    return SolveOutcome::Cancelled(self.stats);
                }
                let action = self.catalog[catalog_index];
                let ink_before = self.work_attempt.ink();
                self.stats.checked_actions = self
                    .stats
                    .checked_actions
                    .checked_add(1)
                    .expect("the bounded transition count must fit usize");
                if self.work_attempt.apply(action).is_err() {
                    continue;
                }

                let ink_after = self.work_attempt.ink();
                let brush_changed_ink =
                    matches!(action, PaperAction::Fold(_)) || ink_after != ink_before;
                let ink_stays_within_target = ink_after
                    .cell_ids()
                    .all(|cell_id| self.puzzle.target().contains(cell_id));
                if brush_changed_ink && ink_stays_within_target {
                    let child_depth = node
                        .depth
                        .checked_add(1)
                        .expect("the puzzle action budgets must fit u8");
                    let child_score = Score::new(
                        self.work_attempt.fold_count(),
                        self.work_attempt.stroke_count(),
                    );
                    let child_key = self.work_attempt.state_key();
                    if !self.visited.contains(&child_key) {
                        let child = SearchNode {
                            parent: entry.node,
                            action: Some(action),
                            depth: child_depth,
                            score: child_score,
                        };
                        if let Err(reason) = self.retain_state(child_key, child) {
                            return self.exhausted(reason);
                        }
                    }
                }

                self.work_attempt
                    .undo()
                    .expect("a successful search action must remain exactly undoable");
            }
        }

        if self.depth_was_limited {
            self.exhausted(ExhaustionReason::Depth)
        } else {
            SolveOutcome::Unsolved(self.stats)
        }
    }

    fn retain_state(
        &mut self,
        key: PaperStateKey,
        node: SearchNode,
    ) -> Result<(), ExhaustionReason> {
        if self.nodes.len() >= self.limits.visited_states {
            return Err(ExhaustionReason::VisitedStates);
        }
        let next_memory = self
            .stats
            .retained_memory_bytes
            .checked_add(self.memory_per_state)
            .ok_or(ExhaustionReason::Memory)?;
        if next_memory > self.limits.memory_bytes {
            return Err(ExhaustionReason::Memory);
        }
        reserve_one(&mut self.visited)?;
        reserve_vec_one(&mut self.nodes)?;
        reserve_heap_one(&mut self.frontier)?;

        let node_index = u32::try_from(self.nodes.len())
            .expect("the visited-state limit must fit in a node index");
        assert!(self.visited.insert(key));
        self.nodes.push(node);
        self.frontier.push(QueueEntry {
            score: node.score,
            order: node_index,
            node: node_index,
        });
        self.stats.visited_states = self.nodes.len();
        self.stats.retained_memory_bytes = next_memory;
        self.stats.maximum_frontier = self.stats.maximum_frontier.max(self.frontier.len());
        self.stats.deepest_state = self.stats.deepest_state.max(node.depth);
        Ok(())
    }

    fn restore(&mut self, node_index: u32) {
        let mut path = [None; MAX_SOLVER_DEPTH as usize];
        let mut path_len = 0_usize;
        let mut cursor = node_index;
        while cursor != NO_NODE {
            let node = self.nodes[cursor as usize];
            if let Some(action) = node.action {
                assert!(path_len < path.len());
                path[path_len] = Some(action);
                path_len += 1;
            }
            cursor = node.parent;
        }

        self.work_attempt.reset();
        for index in (0..path_len).rev() {
            self.work_attempt
                .apply(path[index].expect("the recorded path prefix must be complete"))
                .expect("a retained search path must replay through the production engine");
        }
        assert_eq!(usize::from(self.nodes[node_index as usize].depth), path_len);
        assert_eq!(
            self.nodes[node_index as usize].score,
            Score::new(
                self.work_attempt.fold_count(),
                self.work_attempt.stroke_count(),
            )
        );
    }

    fn verified_solution(&self, score: Score) -> Solution {
        let replay = Replay::from_attempt(&self.work_attempt);
        let verified = replay
            .execute(self.puzzle)
            .expect("a solver path must replay against its exact puzzle revision");
        assert!(verified.result().is_success());
        assert_eq!(verified.result().score(), score);
        Solution {
            replay,
            score,
            stats: self.stats,
        }
    }

    fn maximum_puzzle_depth(&self) -> u8 {
        self.puzzle
            .fold_budget()
            .get()
            .checked_add(self.puzzle.stroke_budget().get())
            .expect("validated puzzle action budgets must fit u8")
    }

    fn exhausted(&self, reason: ExhaustionReason) -> SolveOutcome {
        SolveOutcome::Exhausted {
            reason,
            stats: self.stats,
        }
    }
}

const fn validate_limits(limits: SolverLimits) -> Result<(), InvalidSolverInput> {
    if limits.visited_states == 0 || limits.visited_states > MAX_SOLVER_VISITED_STATES {
        return Err(InvalidSolverInput::VisitedStateLimit {
            found: limits.visited_states,
            maximum: MAX_SOLVER_VISITED_STATES,
        });
    }
    if limits.memory_bytes == 0 || limits.memory_bytes > MAX_SOLVER_MEMORY_BYTES {
        return Err(InvalidSolverInput::MemoryLimit {
            found: limits.memory_bytes,
            maximum: MAX_SOLVER_MEMORY_BYTES,
        });
    }
    if limits.depth > MAX_SOLVER_DEPTH {
        return Err(InvalidSolverInput::DepthLimit {
            found: limits.depth,
            maximum: MAX_SOLVER_DEPTH,
        });
    }
    Ok(())
}

fn cell_payload_bytes(puzzle: &Puzzle) -> usize {
    puzzle
        .dimensions()
        .cell_count()
        .checked_mul(size_of::<PhysicalCell>())
        .expect("the bounded physical-cell payload must fit usize")
}

fn state_memory_charge(puzzle: &Puzzle) -> usize {
    cell_payload_bytes(puzzle)
        .checked_add(size_of::<PaperStateKey>())
        .and_then(|bytes| bytes.checked_add(size_of::<SearchNode>()))
        .and_then(|bytes| bytes.checked_add(size_of::<QueueEntry>()))
        .and_then(|bytes| bytes.checked_add(RETAINED_STATE_MARGIN_BYTES))
        .expect("the bounded solver state charge must fit usize")
}

fn search_base_memory(puzzle: &Puzzle, limits: SolverLimits, catalog_len: usize) -> usize {
    let history_bytes = cell_payload_bytes(puzzle)
        .checked_add(256)
        .and_then(|bytes| bytes.checked_mul(usize::from(limits.depth) + 2))
        .expect("the bounded replay scratch charge must fit usize");
    SEARCH_BASE_MARGIN_BYTES
        .checked_add(history_bytes)
        .and_then(|bytes| {
            catalog_len
                .checked_mul(size_of::<PaperAction>())
                .and_then(|catalog_bytes| bytes.checked_add(catalog_bytes))
        })
        .expect("the bounded solver base charge must fit usize")
}

fn reserve_one(set: &mut HashSet<PaperStateKey>) -> Result<(), ExhaustionReason> {
    set.try_reserve(1).map_err(reservation_failed)
}

fn reserve_vec_one(nodes: &mut Vec<SearchNode>) -> Result<(), ExhaustionReason> {
    nodes.try_reserve(1).map_err(reservation_failed)
}

fn reserve_heap_one(frontier: &mut BinaryHeap<QueueEntry>) -> Result<(), ExhaustionReason> {
    frontier.try_reserve(1).map_err(reservation_failed)
}

fn reservation_failed(_error: TryReserveError) -> ExhaustionReason {
    ExhaustionReason::Memory
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CatalogError {
    Allocation,
}

pub(crate) fn action_catalog(puzzle: &Puzzle) -> Result<Box<[PaperAction]>, CatalogError> {
    let dimensions = puzzle.dimensions();
    let capacity = catalog_capacity(puzzle);
    let mut actions = Vec::new();
    actions
        .try_reserve_exact(capacity)
        .map_err(|_| CatalogError::Allocation)?;
    actions.extend(
        puzzle
            .allowed_folds()
            .iter()
            .copied()
            .map(PaperAction::Fold),
    );

    for &rule in puzzle.allowed_brushes() {
        match rule {
            BrushRule::Dot => {
                for row in 0..dimensions.height().get() {
                    for column in 0..dimensions.width().get() {
                        actions.push(PaperAction::Dot(
                            dimensions
                                .coordinate(row, column)
                                .expect("a bounded catalog coordinate must be valid"),
                        ));
                    }
                }
            }
            BrushRule::Line { axis, length } => match axis {
                StrokeAxis::Horizontal => {
                    let start_columns = dimensions.width().get() - length + 1;
                    for row in 0..dimensions.height().get() {
                        for column in 0..start_columns {
                            let start = dimensions
                                .coordinate(row, column)
                                .expect("a horizontal line start must be valid");
                            let end = dimensions
                                .coordinate(row, column + length - 1)
                                .expect("a horizontal line end must be valid");
                            actions.push(PaperAction::Line(LineStroke::new(start, end)));
                        }
                    }
                }
                StrokeAxis::Vertical => {
                    let start_rows = dimensions.height().get() - length + 1;
                    for row in 0..start_rows {
                        for column in 0..dimensions.width().get() {
                            let start = dimensions
                                .coordinate(row, column)
                                .expect("a vertical line start must be valid");
                            let end = dimensions
                                .coordinate(row + length - 1, column)
                                .expect("a vertical line end must be valid");
                            actions.push(PaperAction::Line(LineStroke::new(start, end)));
                        }
                    }
                }
            },
        }
    }

    assert_eq!(actions.len(), capacity);
    assert!(actions.len() <= MAX_CANDIDATE_ACTIONS);
    Ok(actions.into_boxed_slice())
}

fn catalog_capacity(puzzle: &Puzzle) -> usize {
    let dimensions = puzzle.dimensions();
    let mut count = puzzle.allowed_folds().len();
    for &rule in puzzle.allowed_brushes() {
        let additional = match rule {
            BrushRule::Dot => dimensions.cell_count(),
            BrushRule::Line {
                axis: StrokeAxis::Horizontal,
                length,
            } => {
                usize::from(dimensions.height().get())
                    * usize::from(dimensions.width().get() - length + 1)
            }
            BrushRule::Line {
                axis: StrokeAxis::Vertical,
                length,
            } => {
                usize::from(dimensions.width().get())
                    * usize::from(dimensions.height().get() - length + 1)
            }
        };
        count = count
            .checked_add(additional)
            .expect("the bounded action catalog must fit usize");
    }
    assert!(count <= MAX_CANDIDATE_ACTIONS);
    count
}
