use std::cell::Cell;
use std::collections::{HashSet, VecDeque};

use orifude::domain::paper::{
    BrushRule, CellId, Fold, FoldDirection, LineStroke, PaperAction, PaperStateKey, StrokeAxis,
};
use orifude::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};
use orifude::domain::score::Score;
use orifude::solver::{
    Cancellation, CancellationFlag, ExhaustionReason, InvalidSolverInput, MAX_SOLVER_DEPTH,
    MAX_SOLVER_MEMORY_BYTES, MAX_SOLVER_VISITED_STATES, NeverCancel, SolveOutcome, Solver,
    SolverLimits,
};

fn identity(name: &str) -> PuzzleIdentity {
    PuzzleIdentity::new("solver-tests", name).expect("the test identity should be valid")
}

fn cell(value: u8) -> CellId {
    CellId::new(value).expect("the test cell should be globally valid")
}

fn puzzle(
    name: &str,
    target: Vec<CellId>,
    folds: Vec<Fold>,
    fold_budget: u8,
    strokes: u8,
) -> Puzzle {
    Puzzle::new(
        PuzzleSpec::new(identity(name), 4, 4)
            .with_target_cells(target)
            .with_allowed_folds(folds)
            .with_allowed_brushes(vec![BrushRule::Dot])
            .with_budgets(fold_budget, strokes),
    )
    .expect("the test puzzle should be valid")
}

fn solved(outcome: SolveOutcome) -> orifude::solver::Solution {
    match outcome {
        SolveOutcome::Solved(solution) => solution,
        other => panic!("expected a solved outcome, found {other:?}"),
    }
}

#[test]
fn solver_returns_a_deterministic_shortest_verified_replay() {
    let fold = Fold::new(FoldDirection::Right, 1);
    let puzzle = puzzle("shortest", vec![cell(0), cell(1)], vec![fold], 1, 2);

    let first = solved(Solver::solve(
        &puzzle,
        SolverLimits::default(),
        &NeverCancel,
    ));
    let second = solved(Solver::solve(
        &puzzle,
        SolverLimits::default(),
        &NeverCancel,
    ));

    assert_eq!(first.score().folds().get(), 0);
    assert_eq!(first.score().strokes().get(), 2);
    assert_eq!(first.replay().actions(), second.replay().actions());
    let replayed = first
        .replay()
        .execute(&puzzle)
        .expect("the returned replay should execute against its source puzzle");
    assert!(replayed.result().is_success());
    assert_eq!(replayed.result().score(), first.score());
}

#[test]
fn solver_uses_fold_count_before_stroke_count_even_for_a_longer_path() {
    let fold = Fold::new(FoldDirection::Right, 1);
    let puzzle = puzzle(
        "score-order",
        vec![cell(0), cell(1), cell(4), cell(5)],
        vec![fold],
        1,
        4,
    );

    let solution = solved(Solver::solve(
        &puzzle,
        SolverLimits::default(),
        &NeverCancel,
    ));

    assert_eq!(solution.score().folds().get(), 0);
    assert_eq!(solution.score().strokes().get(), 4);
    assert_eq!(solution.replay().actions().len(), 4);
}

#[test]
fn solver_catalog_covers_horizontal_and_vertical_line_starts() {
    let cases = [
        (
            StrokeAxis::Horizontal,
            vec![cell(0), cell(1)],
            (0, 0),
            (0, 1),
        ),
        (StrokeAxis::Vertical, vec![cell(0), cell(4)], (0, 0), (1, 0)),
    ];

    for (axis, target, start, end) in cases {
        let puzzle = Puzzle::new(
            PuzzleSpec::new(identity("line-catalog"), 4, 4)
                .with_target_cells(target)
                .with_allowed_brushes(vec![BrushRule::Line { axis, length: 2 }])
                .with_budgets(0, 1),
        )
        .expect("the line puzzle should be valid");
        let dimensions = puzzle.dimensions();
        let expected = PaperAction::Line(LineStroke::new(
            dimensions
                .coordinate(start.0, start.1)
                .expect("the line start should be valid"),
            dimensions
                .coordinate(end.0, end.1)
                .expect("the line end should be valid"),
        ));

        let solution = solved(Solver::solve(
            &puzzle,
            SolverLimits::default(),
            &NeverCancel,
        ));

        assert_eq!(solution.replay().actions(), [expected]);
        assert_eq!(solution.score().folds().get(), 0);
        assert_eq!(solution.score().strokes().get(), 1);
    }
}

#[test]
fn solver_distinguishes_unsolved_exhausted_cancelled_and_invalid() {
    let impossible = puzzle("unsolved", vec![cell(0), cell(1)], Vec::new(), 0, 1);
    assert!(matches!(
        Solver::solve(&impossible, SolverLimits::default(), &NeverCancel),
        SolveOutcome::Unsolved(_)
    ));

    let fold = Fold::new(FoldDirection::Right, 1);
    let search = puzzle("stops", vec![cell(0), cell(1)], vec![fold], 1, 2);
    let visited_limit = SolverLimits::new(1, MAX_SOLVER_MEMORY_BYTES, MAX_SOLVER_DEPTH);
    assert!(matches!(
        Solver::solve(&search, visited_limit, &NeverCancel),
        SolveOutcome::Exhausted {
            reason: ExhaustionReason::VisitedStates,
            ..
        }
    ));

    let memory_limit = SolverLimits::new(MAX_SOLVER_VISITED_STATES, 1, MAX_SOLVER_DEPTH);
    match Solver::solve(&search, memory_limit, &NeverCancel) {
        SolveOutcome::Exhausted {
            reason: ExhaustionReason::Memory,
            stats,
        } => {
            assert_eq!(stats.visited_states(), 0);
            assert_eq!(stats.checked_actions(), 0);
            assert_eq!(stats.retained_memory_bytes(), 0);
        }
        other => panic!("expected memory exhaustion before search, found {other:?}"),
    }

    let depth_limit = SolverLimits::new(MAX_SOLVER_VISITED_STATES, MAX_SOLVER_MEMORY_BYTES, 0);
    assert!(matches!(
        Solver::solve(&search, depth_limit, &NeverCancel),
        SolveOutcome::Exhausted {
            reason: ExhaustionReason::Depth,
            ..
        }
    ));

    let cancelled = CancellationFlag::new();
    cancelled.cancel();
    assert!(matches!(
        Solver::solve(&search, SolverLimits::default(), &cancelled),
        SolveOutcome::Cancelled(_)
    ));

    assert_eq!(
        Solver::solve(
            &search,
            SolverLimits::new(0, MAX_SOLVER_MEMORY_BYTES, MAX_SOLVER_DEPTH),
            &NeverCancel,
        ),
        SolveOutcome::Invalid(InvalidSolverInput::VisitedStateLimit {
            found: 0,
            maximum: MAX_SOLVER_VISITED_STATES,
        })
    );
    assert!(matches!(
        Solver::solve(
            &search,
            SolverLimits::new(MAX_SOLVER_VISITED_STATES, 0, MAX_SOLVER_DEPTH),
            &NeverCancel,
        ),
        SolveOutcome::Invalid(InvalidSolverInput::MemoryLimit { found: 0, .. })
    ));
    assert!(matches!(
        Solver::solve(
            &search,
            SolverLimits::new(
                MAX_SOLVER_VISITED_STATES,
                MAX_SOLVER_MEMORY_BYTES,
                MAX_SOLVER_DEPTH + 1,
            ),
            &NeverCancel,
        ),
        SolveOutcome::Invalid(InvalidSolverInput::DepthLimit { .. })
    ));
}

#[derive(Debug)]
struct CountdownCancellation {
    remaining_checks: Cell<usize>,
}

impl Cancellation for CountdownCancellation {
    fn is_cancelled(&self) -> bool {
        let remaining = self.remaining_checks.get();
        if remaining == 0 {
            true
        } else {
            self.remaining_checks.set(remaining - 1);
            false
        }
    }
}

#[test]
fn solver_observes_cancellation_during_expansion() {
    let puzzle = puzzle(
        "cancel-during-search",
        vec![cell(0), cell(1), cell(2), cell(3)],
        Vec::new(),
        0,
        4,
    );
    let cancellation = CountdownCancellation {
        remaining_checks: Cell::new(4),
    };

    let outcome = Solver::solve(&puzzle, SolverLimits::default(), &cancellation);

    assert!(matches!(outcome, SolveOutcome::Cancelled(_)));
}

#[test]
fn solver_matches_an_independent_tiny_exhaustive_search() {
    let fixtures = [
        puzzle("reference-dot", vec![cell(3)], Vec::new(), 0, 1),
        puzzle(
            "reference-fold",
            vec![cell(0), cell(1)],
            vec![Fold::new(FoldDirection::Right, 1)],
            1,
            2,
        ),
        puzzle(
            "reference-unsolved",
            vec![cell(0), cell(1)],
            Vec::new(),
            0,
            1,
        ),
    ];

    for puzzle in fixtures {
        let expected = reference_best_score(&puzzle);
        let actual = match Solver::solve(&puzzle, SolverLimits::default(), &NeverCancel) {
            SolveOutcome::Solved(solution) => Some(solution.score()),
            SolveOutcome::Unsolved(_) => None,
            other => panic!("the tiny solver should finish, found {other:?}"),
        };
        assert_eq!(actual, expected, "puzzle {}", puzzle.identity().puzzle_id());
    }
}

fn reference_best_score(puzzle: &Puzzle) -> Option<Score> {
    let actions = reference_actions(puzzle);
    let maximum_depth = usize::from(puzzle.fold_budget().get() + puzzle.stroke_budget().get());
    let root = puzzle.start();
    let mut visited = HashSet::<PaperStateKey>::from([root.state_key()]);
    let mut frontier = VecDeque::from([(root, 0_usize)]);
    let mut best = None;

    while let Some((attempt, depth)) = frontier.pop_front() {
        if attempt.result().is_success() {
            let score = attempt.result().score();
            best = Some(best.map_or(score, |current: Score| current.min(score)));
        }
        if depth == maximum_depth {
            continue;
        }
        for &action in &actions {
            let mut child = attempt.clone();
            if child.apply(action).is_err() || !visited.insert(child.state_key()) {
                continue;
            }
            frontier.push_back((child, depth + 1));
        }
    }
    best
}

fn reference_actions(puzzle: &Puzzle) -> Vec<PaperAction> {
    let mut actions: Vec<_> = puzzle
        .allowed_folds()
        .iter()
        .copied()
        .map(PaperAction::Fold)
        .collect();
    assert_eq!(puzzle.allowed_brushes(), [BrushRule::Dot]);
    let dimensions = puzzle.dimensions();
    for row in 0..dimensions.height().get() {
        for column in 0..dimensions.width().get() {
            actions.push(PaperAction::Dot(
                dimensions
                    .coordinate(row, column)
                    .expect("an enumerated coordinate should be valid"),
            ));
        }
    }
    actions
}
