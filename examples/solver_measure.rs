use std::collections::HashSet;
use std::hint::black_box;
use std::mem::size_of;
use std::process::ExitCode;
use std::time::Instant;

use orifude::domain::attempt::Attempt;
use orifude::domain::paper::{
    ActionCount, BrushRule, CellId, Fold, FoldCount, FoldDirection, InkPattern, PaperAction,
    PaperStateKey, PhysicalCell, StrokeCount,
};
use orifude::domain::puzzle::{
    MAX_ALLOWED_FOLDS, MAX_BRUSH_RULES, Puzzle, PuzzleIdentity, PuzzleSpec,
};
use orifude::solver::{
    ExhaustionReason, NeverCancel, SolveOutcome, SolveStats, Solver, SolverLimits,
};

const SAMPLE_COUNT: usize = 25;
const SIMPLE_ITERATIONS: usize = 250;
const BROAD_ITERATIONS: usize = 25;
const FRONTIER_ITERATIONS: usize = 250;
const LOOKUP_ITERATIONS: usize = 25;
const LOOKUP_KEY_COUNT: usize = 256;

#[derive(Clone, Copy)]
struct Nanoseconds {
    median: u128,
    p95: u128,
    maximum: u128,
}

impl Nanoseconds {
    fn per_item(self, count: usize) -> Self {
        assert!(count > 0);
        let count = count as u128;
        Self {
            median: self.median / count,
            p95: self.p95 / count,
            maximum: self.maximum / count,
        }
    }
}

fn main() -> ExitCode {
    if cfg!(debug_assertions) {
        eprintln!("error: solver measurements must run with --release");
        return ExitCode::FAILURE;
    }

    let fixtures = representative_puzzles();
    println!("Solver results use {SAMPLE_COUNT} samples in one release process.");
    println!(
        "Times summarize batch-average nanoseconds per solve, not individual latency tails.\n"
    );
    println!(
        "class                  visited  expanded   checked      median         p95         max"
    );
    for (name, puzzle, iterations) in &fixtures {
        let outcome = Solver::solve(puzzle, SolverLimits::default(), &NeverCancel);
        let stats = completed_stats(&outcome);
        let timings = measure(*iterations, || {
            let outcome = Solver::solve(black_box(puzzle), SolverLimits::default(), &NeverCancel);
            u64::try_from(completed_stats(&outcome).checked_actions())
                .expect("the bounded action count must fit u64")
        });
        println!(
            "{name:<22}{:>8}{:>10}{:>10}{:>12}{:>12}{:>12}",
            stats.visited_states(),
            stats.expanded_states(),
            stats.checked_actions(),
            timings.median,
            timings.p95,
            timings.maximum,
        );
    }

    measure_broad_search();

    let (frontier_attempt, frontier_actions) = maximum_depth_attempt();
    let mut replay_attempt = frontier_attempt.puzzle().start();
    let clone_timings = measure(FRONTIER_ITERATIONS, || {
        let cloned = black_box(frontier_attempt.clone());
        u64::from(cloned.action_count().get())
    });
    let replay_timings = measure(FRONTIER_ITERATIONS, || {
        replay_attempt.reset();
        for &action in &frontier_actions {
            replay_attempt
                .apply(action)
                .expect("the measured retained path must replay");
        }
        u64::from(replay_attempt.action_count().get())
    });

    println!("\nFrontier work uses one fixed maximum-depth path.");
    println!("operation                         median         p95         max");
    print_distribution("clone full Attempt", clone_timings);
    print_distribution("reset and replay path", replay_timings);

    let keys = lookup_keys();
    let mut hashed = HashSet::with_capacity(keys.len());
    hashed.extend(keys.iter().cloned());
    let linear = keys.clone();
    let hashed_timings =
        measure(LOOKUP_ITERATIONS, || lookup_hash(&hashed, &keys)).per_item(keys.len());
    let linear_timings =
        measure(LOOKUP_ITERATIONS, || lookup_linear(&linear, &keys)).per_item(keys.len());
    let key_payload = 12_usize * 12 * size_of::<PhysicalCell>();
    let key_lower_bound = size_of::<PaperStateKey>() + key_payload;
    let selected_charge = Solver::conservative_state_memory_bytes(&maximum_puzzle());
    let snapshot_lower_bound = size_of::<Vec<PhysicalCell>>()
        + key_payload
        + size_of::<InkPattern>()
        + size_of::<FoldCount>()
        + size_of::<StrokeCount>()
        + size_of::<ActionCount>();
    let maximum_rule_payload =
        MAX_ALLOWED_FOLDS * size_of::<Fold>() + MAX_BRUSH_RULES * size_of::<BrushRule>();
    let full_attempt_lower_bound = size_of::<Attempt>()
        + key_payload
        + maximum_rule_payload
        + frontier_actions.len() * (snapshot_lower_bound + size_of::<PaperAction>());

    println!("\nVisited membership uses the same {LOOKUP_KEY_COUNT} maximum-paper keys.");
    println!("operation                         median         p95         max");
    print_distribution("HashSet contains", hashed_timings);
    print_distribution("linear Vec contains", linear_timings);
    println!("\nRepresentation sizes on this target:");
    println!("  canonical key lower bound:              {key_lower_bound:>8} bytes");
    println!("  selected conservative retained charge:  {selected_charge:>8} bytes/state");
    println!(
        "  full Attempt at depth 20 lower bound:    {full_attempt_lower_bound:>8} bytes/state"
    );
    println!(
        "  HashSet named key storage:               {:>8} bytes",
        hashed.capacity() * size_of::<PaperStateKey>() + keys.len() * key_payload
    );
    println!(
        "  linear Vec named key storage:            {:>8} bytes",
        linear.capacity() * size_of::<PaperStateKey>() + keys.len() * key_payload
    );
    println!("HashSet bytes exclude control bytes and allocator bookkeeping.");
    println!("Lower bounds exclude allocator bookkeeping and collection spare capacity.");
    println!("The production charge includes a 512-byte margin for those unknowns.");
    ExitCode::SUCCESS
}

fn measure_broad_search() {
    let puzzle = puzzle(
        "measure-broad",
        12,
        12,
        (0..16).map(cell).collect(),
        Vec::new(),
        0,
        8,
    );
    let limits = SolverLimits::new(20_000, 128 * 1024 * 1024, 20);
    let started = Instant::now();
    let outcome = Solver::solve(&puzzle, limits, &NeverCancel);
    let elapsed = started.elapsed();
    let SolveOutcome::Exhausted {
        reason: ExhaustionReason::VisitedStates,
        stats,
    } = outcome
    else {
        panic!("the broad search must stop at its visited-state limit: {outcome:?}");
    };
    assert_eq!(stats.visited_states(), limits.max_visited_states());
    println!("\nBroad 12-by-12 search, one bounded solve per process:");
    println!("broad_elapsed_us={}", elapsed.as_micros());
    println!("broad_visited={}", stats.visited_states());
    println!("broad_checked={}", stats.checked_actions());
    println!("broad_retained_bytes={}", stats.retained_memory_bytes());
}

fn representative_puzzles() -> [(&'static str, Puzzle, usize); 4] {
    let dot = puzzle("measure-dot", 4, 4, vec![cell(15)], Vec::new(), 0, 1);
    let fold = puzzle(
        "measure-fold",
        4,
        4,
        vec![cell(0), cell(1)],
        vec![Fold::new(FoldDirection::Right, 1)],
        1,
        1,
    );
    let unsolved = puzzle(
        "measure-unsolved",
        4,
        4,
        vec![cell(0), cell(1)],
        Vec::new(),
        0,
        1,
    );
    let broad = derived_puzzle();
    [
        ("single dot", dot, SIMPLE_ITERATIONS),
        ("one fold and dot", fold, SIMPLE_ITERATIONS),
        ("small unsolved", unsolved, SIMPLE_ITERATIONS),
        ("two-axis stack", broad, BROAD_ITERATIONS),
    ]
}

fn maximum_puzzle() -> Puzzle {
    puzzle("measure-maximum", 12, 12, Vec::new(), Vec::new(), 0, 8)
}

fn maximum_depth_attempt() -> (Attempt, Vec<PaperAction>) {
    let left = Fold::new(FoldDirection::Left, 6);
    let right = Fold::new(FoldDirection::Right, 6);
    let puzzle = puzzle(
        "measure-frontier",
        12,
        12,
        Vec::new(),
        vec![left, right],
        12,
        8,
    );
    let mut attempt = puzzle.start();
    let mut actions = Vec::with_capacity(20);
    for fold_index in 0_u8..12 {
        let action = PaperAction::Fold(if fold_index.is_multiple_of(2) {
            left
        } else {
            right
        });
        attempt
            .apply(action)
            .expect("the measured alternating fold should be legal");
        actions.push(action);
    }
    let coordinate = attempt
        .dimensions()
        .coordinate(0, 6)
        .expect("the measured repeated dot coordinate should be valid");
    for _ in 0..8 {
        let action = PaperAction::Dot(coordinate);
        attempt
            .apply(action)
            .expect("the measured repeated dot should be legal");
        actions.push(action);
    }
    assert_eq!(actions.len(), 20);
    assert_eq!(attempt.action_count().get(), 20);
    (attempt, actions)
}

fn derived_puzzle() -> Puzzle {
    let folds = vec![
        Fold::new(FoldDirection::Right, 3),
        Fold::new(FoldDirection::Down, 3),
    ];
    let template = puzzle("measure-source", 6, 6, Vec::new(), folds.clone(), 2, 1);
    let mut source = template.start();
    source
        .apply(PaperAction::Fold(folds[0]))
        .expect("the first measured fold should be legal");
    source
        .apply(PaperAction::Fold(folds[1]))
        .expect("the second measured fold should be legal");
    let coordinate = source
        .dimensions()
        .coordinate(3, 3)
        .expect("the measured dot coordinate should be valid");
    source
        .apply(PaperAction::Dot(coordinate))
        .expect("the measured dot should be legal");
    puzzle(
        "measure-broad",
        6,
        6,
        source.ink().cell_ids().collect(),
        folds,
        2,
        1,
    )
}

fn puzzle(
    name: &str,
    width: u8,
    height: u8,
    target: Vec<CellId>,
    folds: Vec<Fold>,
    fold_budget: u8,
    stroke_budget: u8,
) -> Puzzle {
    let identity =
        PuzzleIdentity::new("measurement", name).expect("the measurement identity should be valid");
    Puzzle::new(
        PuzzleSpec::new(identity, width, height)
            .with_target_cells(target)
            .with_allowed_folds(folds)
            .with_allowed_brushes(vec![BrushRule::Dot])
            .with_budgets(fold_budget, stroke_budget),
    )
    .expect("the measurement puzzle should be valid")
}

fn lookup_keys() -> Vec<PaperStateKey> {
    let puzzle = puzzle("measure-keys", 12, 12, Vec::new(), Vec::new(), 0, 8);
    let dimensions = puzzle.dimensions();
    let first = dimensions
        .coordinate(0, 0)
        .expect("the first key coordinate should be valid");
    let mut keys = Vec::with_capacity(LOOKUP_KEY_COUNT);
    keys.push(puzzle.start().state_key());

    for index in 0..dimensions.cell_count() {
        let mut attempt = puzzle.start();
        let coordinate = dimensions
            .coordinate(
                u8::try_from(index / usize::from(dimensions.width().get()))
                    .expect("a key row must fit u8"),
                u8::try_from(index % usize::from(dimensions.width().get()))
                    .expect("a key column must fit u8"),
            )
            .expect("an enumerated key coordinate should be valid");
        attempt
            .apply(PaperAction::Dot(coordinate))
            .expect("an enumerated key dot should be valid");
        keys.push(attempt.state_key());
    }
    for index in 1..dimensions.cell_count() {
        if keys.len() == LOOKUP_KEY_COUNT {
            break;
        }
        let mut attempt = puzzle.start();
        attempt
            .apply(PaperAction::Dot(first))
            .expect("the first paired key dot should be valid");
        let coordinate = dimensions
            .coordinate(
                u8::try_from(index / usize::from(dimensions.width().get()))
                    .expect("a paired key row must fit u8"),
                u8::try_from(index % usize::from(dimensions.width().get()))
                    .expect("a paired key column must fit u8"),
            )
            .expect("an enumerated paired key coordinate should be valid");
        attempt
            .apply(PaperAction::Dot(coordinate))
            .expect("an enumerated paired key dot should be valid");
        keys.push(attempt.state_key());
    }
    assert_eq!(keys.len(), LOOKUP_KEY_COUNT);
    keys
}

fn completed_stats(outcome: &SolveOutcome) -> SolveStats {
    match outcome {
        SolveOutcome::Solved(solution) => solution.stats(),
        SolveOutcome::Unsolved(stats)
        | SolveOutcome::Cancelled(stats)
        | SolveOutcome::Exhausted { stats, .. } => *stats,
        SolveOutcome::Invalid(error) => panic!("measurement limits should be valid: {error:?}"),
    }
}

fn lookup_hash(set: &HashSet<PaperStateKey>, keys: &[PaperStateKey]) -> u64 {
    keys.iter()
        .map(|key| u64::from(set.contains(black_box(key))))
        .sum()
}

fn lookup_linear(entries: &[PaperStateKey], keys: &[PaperStateKey]) -> u64 {
    keys.iter()
        .map(|key| u64::from(entries.contains(black_box(key))))
        .sum()
}

fn measure(iterations: usize, mut operation: impl FnMut() -> u64) -> Nanoseconds {
    assert!(iterations > 0);
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        let mut checksum = 0_u64;
        for _ in 0..iterations {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        black_box(checksum);
        samples.push(started.elapsed().as_nanos() / iterations as u128);
    }
    samples.sort_unstable();
    Nanoseconds {
        median: samples[SAMPLE_COUNT / 2],
        p95: samples[(SAMPLE_COUNT * 95).div_ceil(100) - 1],
        maximum: samples[SAMPLE_COUNT - 1],
    }
}

fn print_distribution(label: &str, distribution: Nanoseconds) {
    println!(
        "{label:<30}{:>12}{:>12}{:>12}",
        distribution.median, distribution.p95, distribution.maximum
    );
}

fn cell(value: u8) -> CellId {
    CellId::new(value).expect("the measurement cell should be globally valid")
}
