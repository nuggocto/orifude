//! Versioned, bounded, deterministic puzzle generation.

use std::error::Error;
use std::fmt;

use crate::domain::attempt::Attempt;
use crate::domain::paper::{BrushRule, Fold, InkPattern, MAX_ACTIONS, PaperAction};
use crate::domain::puzzle::{
    MAX_ALLOWED_FOLDS, MAX_BRUSH_RULES, Puzzle, PuzzleError, PuzzleIdentity, PuzzleSpec,
};
use crate::domain::replay::{Replay, ReplayMetadata};
use crate::domain::score::Par;
use crate::solver::{
    Cancellation, CatalogError, ExhaustionReason, InvalidSolverInput, Solution, SolveOutcome,
    Solver, SolverLimits, action_catalog,
};

pub const CURRENT_GENERATOR_COMPATIBILITY_VERSION: u16 = 1;
pub const MAX_GENERATION_ATTEMPTS: u16 = 512;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CalendarDate {
    year: u16,
    month: u8,
    day: u8,
}

impl CalendarDate {
    /// Constructs a Gregorian calendar date without consulting a system clock.
    ///
    /// # Errors
    ///
    /// Returns a typed error for years outside 1..=9999 or invalid month-day
    /// combinations.
    pub const fn new(year: u16, month: u8, day: u8) -> Result<Self, CalendarDateError> {
        if year == 0 || year > 9_999 {
            return Err(CalendarDateError::Year { found: year });
        }
        if month == 0 || month > 12 {
            return Err(CalendarDateError::Month { found: month });
        }
        let maximum_day = days_in_month(year, month);
        if day == 0 || day > maximum_day {
            return Err(CalendarDateError::Day {
                found: day,
                maximum: maximum_day,
            });
        }
        Ok(Self { year, month, day })
    }

    #[must_use]
    pub const fn year(self) -> u16 {
        self.year
    }

    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    #[must_use]
    pub const fn day(self) -> u8 {
        self.day
    }
}

impl fmt::Display for CalendarDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalendarDateError {
    Year { found: u16 },
    Month { found: u8 },
    Day { found: u8, maximum: u8 },
}

impl fmt::Display for CalendarDateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Year { found } => write!(formatter, "year {found} must be between 1 and 9999"),
            Self::Month { found } => write!(formatter, "month {found} must be between 1 and 12"),
            Self::Day { found, maximum } => {
                write!(formatter, "day {found} must be between 1 and {maximum}")
            }
        }
    }
}

impl Error for CalendarDateError {}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GenerationSeed {
    compatibility_version: u16,
    value: u64,
}

impl GenerationSeed {
    #[must_use]
    pub const fn new(compatibility_version: u16, value: u64) -> Self {
        Self {
            compatibility_version,
            value,
        }
    }

    #[must_use]
    pub const fn current(value: u64) -> Self {
        Self::new(CURRENT_GENERATOR_COMPATIBILITY_VERSION, value)
    }

    /// Derives a daily seed through the current compatibility path.
    ///
    /// # Panics
    ///
    /// Panics only if the current compatibility constant has no matching
    /// implementation, which is a programmer-error invariant.
    #[must_use]
    pub fn for_date(date: CalendarDate) -> Self {
        Self::for_date_with_version(CURRENT_GENERATOR_COMPATIBILITY_VERSION, date)
            .expect("the current generator compatibility must remain supported")
    }

    /// Derives a daily seed through the requested preserved compatibility path.
    ///
    /// # Errors
    ///
    /// Returns an error when this build cannot reproduce that generator
    /// compatibility version.
    pub fn for_date_with_version(
        compatibility_version: u16,
        date: CalendarDate,
    ) -> Result<Self, GenerationError> {
        match compatibility_version {
            1 => Ok(Self::new(compatibility_version, daily_seed_v1(date))),
            found => Err(GenerationError::UnsupportedCompatibility {
                found,
                supported: CURRENT_GENERATOR_COMPATIBILITY_VERSION,
            }),
        }
    }

    #[must_use]
    pub const fn compatibility_version(self) -> u16 {
        self.compatibility_version
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.value
    }
}

impl fmt::Display for GenerationSeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "v{}-{:016x}",
            self.compatibility_version, self.value
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratorConfig {
    template_identity: PuzzleIdentity,
    width: u8,
    height: u8,
    allowed_folds: Box<[Fold]>,
    allowed_brushes: Box<[BrushRule]>,
    fold_budget: u8,
    stroke_budget: u8,
    minimum_source_actions: u8,
    maximum_source_actions: u8,
    maximum_attempts: u16,
    solver_limits: SolverLimits,
}

impl GeneratorConfig {
    /// Starts a generation policy with a validated pack identity.
    ///
    /// # Errors
    ///
    /// Returns a typed puzzle error before retaining an invalid pack ID.
    pub fn new(pack_id: &str, width: u8, height: u8) -> Result<Self, GenerationError> {
        let template_identity =
            PuzzleIdentity::new(pack_id, "generated-template").map_err(GenerationError::Puzzle)?;
        Ok(Self {
            template_identity,
            width,
            height,
            allowed_folds: Box::default(),
            allowed_brushes: Box::default(),
            fold_budget: 0,
            stroke_budget: 0,
            minimum_source_actions: 2,
            maximum_source_actions: 2,
            maximum_attempts: MAX_GENERATION_ATTEMPTS,
            solver_limits: SolverLimits::default(),
        })
    }

    /// Adds rule collections only when both fit the puzzle storage bounds.
    ///
    /// # Errors
    ///
    /// Returns a typed puzzle error without retaining either collection when
    /// its length exceeds the corresponding bound.
    pub fn with_rules(
        mut self,
        allowed_folds: Vec<Fold>,
        allowed_brushes: Vec<BrushRule>,
    ) -> Result<Self, GenerationError> {
        if allowed_folds.len() > MAX_ALLOWED_FOLDS {
            return Err(GenerationError::Puzzle(PuzzleError::TooManyAllowedFolds {
                count: allowed_folds.len(),
                limit: MAX_ALLOWED_FOLDS,
            }));
        }
        if allowed_brushes.len() > MAX_BRUSH_RULES {
            return Err(GenerationError::Puzzle(PuzzleError::TooManyBrushRules {
                count: allowed_brushes.len(),
                limit: MAX_BRUSH_RULES,
            }));
        }
        self.allowed_folds = allowed_folds.into_boxed_slice();
        self.allowed_brushes = allowed_brushes.into_boxed_slice();
        Ok(self)
    }

    #[must_use]
    pub const fn with_budgets(mut self, fold_budget: u8, stroke_budget: u8) -> Self {
        self.fold_budget = fold_budget;
        self.stroke_budget = stroke_budget;
        self
    }

    #[must_use]
    pub const fn with_source_action_range(mut self, minimum: u8, maximum: u8) -> Self {
        self.minimum_source_actions = minimum;
        self.maximum_source_actions = maximum;
        self
    }

    #[must_use]
    pub const fn with_attempt_limit(mut self, maximum_attempts: u16) -> Self {
        self.maximum_attempts = maximum_attempts;
        self
    }

    #[must_use]
    pub const fn with_solver_limits(mut self, solver_limits: SolverLimits) -> Self {
        self.solver_limits = solver_limits;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Generator {
    config: GeneratorConfig,
    template: Puzzle,
    catalog: Box<[PaperAction]>,
}

impl Generator {
    /// Validates a complete generation policy before retaining it.
    ///
    /// # Errors
    ///
    /// Returns a typed configuration, puzzle, solver-limit, or allocation
    /// error. Generation never starts from a partially valid policy.
    pub fn new(config: GeneratorConfig) -> Result<Self, GenerationError> {
        validate_config(&config)?;
        config
            .solver_limits
            .validate()
            .map_err(GenerationError::SolverLimits)?;

        let template = Puzzle::new(
            PuzzleSpec::new(
                config.template_identity.clone(),
                config.width,
                config.height,
            )
            .with_allowed_folds(config.allowed_folds.to_vec())
            .with_allowed_brushes(config.allowed_brushes.to_vec())
            .with_budgets(config.fold_budget, config.stroke_budget),
        )
        .map_err(GenerationError::Puzzle)?;
        let catalog = action_catalog(&template)
            .map_err(|CatalogError::Allocation| GenerationError::Allocation)?;
        Ok(Self {
            config,
            template,
            catalog,
        })
    }

    #[must_use]
    pub const fn config(&self) -> &GeneratorConfig {
        &self.config
    }

    /// Generates one validated nontrivial puzzle from an explicit seed.
    ///
    /// Candidate construction, validation, solving, and replay verification
    /// all finish within their independent configured bounds.
    #[must_use]
    pub fn generate<C: Cancellation + ?Sized>(
        &self,
        seed: GenerationSeed,
        cancellation: &C,
    ) -> GenerationOutcome {
        match seed.compatibility_version {
            1 => self.generate_v1(seed, cancellation),
            found => GenerationOutcome::Invalid(GenerationError::UnsupportedCompatibility {
                found,
                supported: CURRENT_GENERATOR_COMPATIBILITY_VERSION,
            }),
        }
    }

    fn generate_v1<C: Cancellation + ?Sized>(
        &self,
        seed: GenerationSeed,
        cancellation: &C,
    ) -> GenerationOutcome {
        let mut random = StableRandom::new(seed.value);
        let mut seen_targets = Vec::<InkPattern>::new();
        if seen_targets
            .try_reserve_exact(usize::from(self.config.maximum_attempts))
            .is_err()
        {
            return GenerationOutcome::Invalid(GenerationError::Allocation);
        }
        let mut stats = GenerationStats::default();
        let mut last_rejection = CandidateRejection::InvalidSequence;
        let mut candidate = self.template.start();

        for attempt_index in 0..self.config.maximum_attempts {
            if cancellation.is_cancelled() {
                return GenerationOutcome::Cancelled { seed, stats };
            }
            stats.attempted_candidates = stats
                .attempted_candidates
                .checked_add(1)
                .expect("the generation attempt bound must fit u16");
            match self.build_candidate(&mut random, cancellation, &mut candidate) {
                CandidateBuild::Built => {}
                CandidateBuild::Rejected => {
                    last_rejection = CandidateRejection::InvalidSequence;
                    continue;
                }
                CandidateBuild::Cancelled => {
                    return GenerationOutcome::Cancelled { seed, stats };
                }
            }

            let puzzle =
                match self.validate_candidate(seed, attempt_index, &candidate, &mut seen_targets) {
                    Ok(puzzle) => puzzle,
                    Err(rejection) => {
                        if rejection == CandidateRejection::DuplicateTarget {
                            stats.duplicate_candidates = stats
                                .duplicate_candidates
                                .checked_add(1)
                                .expect("duplicate candidates cannot exceed the attempt bound");
                        }
                        last_rejection = rejection;
                        continue;
                    }
                };

            stats.solver_calls = stats
                .solver_calls
                .checked_add(1)
                .expect("solver calls cannot exceed the attempt bound");
            let solution = match Solver::solve(&puzzle, self.config.solver_limits, cancellation) {
                SolveOutcome::Solved(solution) => solution,
                SolveOutcome::Unsolved(_) => {
                    panic!("a generated candidate action sequence must remain a solution witness")
                }
                SolveOutcome::Exhausted { reason, .. } => {
                    last_rejection = CandidateRejection::SolverExhausted(reason);
                    continue;
                }
                SolveOutcome::Cancelled(_) => {
                    return GenerationOutcome::Cancelled { seed, stats };
                }
                SolveOutcome::Invalid(error) => {
                    return GenerationOutcome::Invalid(GenerationError::SolverLimits(error));
                }
            };
            if solution.score().folds().get() == 0 || solution.score().strokes().get() == 0 {
                last_rejection = CandidateRejection::Trivial;
                continue;
            }
            return self.generated_outcome(seed, attempt_index, &puzzle, solution, stats);
        }

        GenerationOutcome::Exhausted {
            seed,
            last_rejection,
            stats,
        }
    }

    fn build_candidate<C: Cancellation + ?Sized>(
        &self,
        random: &mut StableRandom,
        cancellation: &C,
        candidate: &mut Attempt,
    ) -> CandidateBuild {
        let source_actions = random.inclusive_u8(
            self.config.minimum_source_actions,
            self.config.maximum_source_actions,
        );
        candidate.reset();
        for action_index in 0..source_actions {
            let needs_fold = action_index == 0;
            let needs_brush = action_index + 1 == source_actions && candidate.ink().is_empty();
            let fold_actions = self.template.allowed_folds().len();
            let (range_start, range_len) = if needs_fold {
                (0, fold_actions)
            } else if needs_brush {
                (fold_actions, self.catalog.len() - fold_actions)
            } else {
                (0, self.catalog.len())
            };
            assert!(range_len > 0);
            let start = random.index(range_len);
            let mut accepted = false;
            for offset in 0..range_len {
                if cancellation.is_cancelled() {
                    return CandidateBuild::Cancelled;
                }
                let action = self.catalog[range_start + (start + offset) % range_len];
                assert!(!needs_fold || matches!(action, PaperAction::Fold(_)));
                assert!(!needs_brush || !matches!(action, PaperAction::Fold(_)));
                let ink_before = candidate.ink();
                if candidate.apply(action).is_err() {
                    continue;
                }
                let brush_is_noop =
                    !matches!(action, PaperAction::Fold(_)) && candidate.ink() == ink_before;
                if brush_is_noop {
                    candidate
                        .undo()
                        .expect("a successful candidate action must remain undoable");
                    continue;
                }
                accepted = true;
                break;
            }
            if !accepted {
                return CandidateBuild::Rejected;
            }
        }
        CandidateBuild::Built
    }

    fn validate_candidate(
        &self,
        seed: GenerationSeed,
        attempt_index: u16,
        candidate: &Attempt,
        seen_targets: &mut Vec<InkPattern>,
    ) -> Result<Puzzle, CandidateRejection> {
        assert!(
            candidate.actions().next().is_some(),
            "a built candidate must contain the configured action sequence"
        );
        assert!(
            !candidate.ink().is_empty(),
            "a built candidate must finish with a successful brush action"
        );
        assert!(
            candidate.fold_count().get() <= self.config.fold_budget,
            "production actions must enforce the configured fold budget"
        );
        assert!(
            candidate.stroke_count().get() <= self.config.stroke_budget,
            "production actions must enforce the configured stroke budget"
        );

        let target = candidate.ink();
        if seen_targets.contains(&target) {
            return Err(CandidateRejection::DuplicateTarget);
        }
        seen_targets.push(target);
        let puzzle = self
            .candidate_puzzle(seed, attempt_index, target, None)
            .expect("a candidate built from the validated template must pass puzzle validation");
        assert!(
            !puzzle.start().result().is_success(),
            "a non-empty target must not match fresh uninked paper"
        );
        Ok(puzzle)
    }

    fn generated_outcome(
        &self,
        seed: GenerationSeed,
        attempt_index: u16,
        puzzle: &Puzzle,
        solution: Solution,
        stats: GenerationStats,
    ) -> GenerationOutcome {
        let final_puzzle = self
            .candidate_puzzle(
                seed,
                attempt_index,
                puzzle.target(),
                Some(Par::new(
                    solution.score().folds(),
                    solution.score().strokes(),
                )),
            )
            .expect("a validated candidate must accept its within-budget solver score");
        let replay = Replay::new(
            ReplayMetadata::current(&final_puzzle),
            solution.replay().actions().to_vec(),
        )
        .expect("a bounded solver solution must fit the replay action limit");
        let solution = solution.with_replay(replay);
        let verified = solution
            .replay()
            .execute(&final_puzzle)
            .expect("the rebound solution must execute against the final puzzle revision");
        assert!(verified.result().is_success());
        assert_eq!(verified.result().score(), solution.score());
        GenerationOutcome::Generated {
            puzzle: Box::new(GeneratedPuzzle {
                puzzle: final_puzzle,
                solution,
                seed,
                candidate_attempt: attempt_index,
            }),
            stats,
        }
    }

    fn candidate_puzzle(
        &self,
        seed: GenerationSeed,
        attempt_index: u16,
        target: InkPattern,
        par: Option<Par>,
    ) -> Result<Puzzle, PuzzleError> {
        let puzzle_id = format!(
            "paper-v{}-{:016x}-{attempt_index}",
            seed.compatibility_version, seed.value
        );
        let identity = PuzzleIdentity::new(self.config.template_identity.pack_id(), &puzzle_id)?;
        let mut spec = PuzzleSpec::new(identity, self.config.width, self.config.height)
            .with_target_cells(target.cell_ids().collect())
            .with_allowed_folds(self.config.allowed_folds.to_vec())
            .with_allowed_brushes(self.config.allowed_brushes.to_vec())
            .with_budgets(self.config.fold_budget, self.config.stroke_budget);
        if let Some(par) = par {
            spec = spec.with_par(par);
        }
        Puzzle::new(spec)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedPuzzle {
    puzzle: Puzzle,
    solution: Solution,
    seed: GenerationSeed,
    candidate_attempt: u16,
}

impl GeneratedPuzzle {
    #[must_use]
    pub const fn puzzle(&self) -> &Puzzle {
        &self.puzzle
    }

    #[must_use]
    pub const fn solution(&self) -> &Solution {
        &self.solution
    }

    #[must_use]
    pub const fn seed(&self) -> GenerationSeed {
        self.seed
    }

    #[must_use]
    pub const fn candidate_attempt(&self) -> u16 {
        self.candidate_attempt
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GenerationStats {
    attempted_candidates: u16,
    duplicate_candidates: u16,
    solver_calls: u16,
}

impl GenerationStats {
    #[must_use]
    pub const fn attempted_candidates(self) -> u16 {
        self.attempted_candidates
    }

    #[must_use]
    pub const fn duplicate_candidates(self) -> u16 {
        self.duplicate_candidates
    }

    #[must_use]
    pub const fn solver_calls(self) -> u16 {
        self.solver_calls
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateRejection {
    Trivial,
    DuplicateTarget,
    InvalidSequence,
    SolverExhausted(ExhaustionReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenerationOutcome {
    Generated {
        puzzle: Box<GeneratedPuzzle>,
        stats: GenerationStats,
    },
    Exhausted {
        seed: GenerationSeed,
        last_rejection: CandidateRejection,
        stats: GenerationStats,
    },
    Cancelled {
        seed: GenerationSeed,
        stats: GenerationStats,
    },
    Invalid(GenerationError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenerationError {
    Puzzle(PuzzleError),
    NeedsFold,
    NeedsBrush,
    AttemptLimit { found: u16, maximum: u16 },
    SourceActionRange { minimum: u8, maximum: u8 },
    SourceActionsExceedBudget { maximum: u8, budget: u8 },
    SolverLimits(InvalidSolverInput),
    UnsupportedCompatibility { found: u16, supported: u16 },
    Allocation,
}

impl fmt::Display for GenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Puzzle(error) => write!(formatter, "invalid generator puzzle policy: {error}"),
            Self::NeedsFold => formatter.write_str("generation requires a fold rule and budget"),
            Self::NeedsBrush => formatter.write_str("generation requires a brush rule and budget"),
            Self::AttemptLimit { found, maximum } => write!(
                formatter,
                "generation attempt limit {found} must be between 1 and {maximum}"
            ),
            Self::SourceActionRange { minimum, maximum } => write!(
                formatter,
                "source action range {minimum}..={maximum} must start at 2 and remain ordered"
            ),
            Self::SourceActionsExceedBudget { maximum, budget } => write!(
                formatter,
                "source action maximum {maximum} exceeds the combined action budget {budget}"
            ),
            Self::SolverLimits(error) => write!(formatter, "invalid solver limits: {error:?}"),
            Self::UnsupportedCompatibility { found, supported } => write!(
                formatter,
                "generator compatibility {found} is unsupported; this build accepts {supported}"
            ),
            Self::Allocation => formatter.write_str("generation could not reserve bounded memory"),
        }
    }
}

impl Error for GenerationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Puzzle(error) => Some(error),
            _ => None,
        }
    }
}

enum CandidateBuild {
    Built,
    Rejected,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StableRandom {
    state: u64,
}

impl StableRandom {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn index(&mut self, exclusive_upper: usize) -> usize {
        assert!(exclusive_upper > 0);
        let mapped = (u128::from(self.next_u64())
            * u128::try_from(exclusive_upper).expect("the bounded catalog size must fit u128"))
            >> 64;
        usize::try_from(mapped).expect("the mapped index must remain below the usize upper bound")
    }

    fn inclusive_u8(&mut self, lower: u8, upper: u8) -> u8 {
        assert!(lower <= upper);
        let width = usize::from(upper - lower) + 1;
        lower
            .checked_add(u8::try_from(self.index(width)).expect("a u8 range offset must fit u8"))
            .expect("the bounded inclusive range must fit u8")
    }
}

fn validate_config(config: &GeneratorConfig) -> Result<(), GenerationError> {
    if config.maximum_attempts == 0 || config.maximum_attempts > MAX_GENERATION_ATTEMPTS {
        return Err(GenerationError::AttemptLimit {
            found: config.maximum_attempts,
            maximum: MAX_GENERATION_ATTEMPTS,
        });
    }
    if config.allowed_folds.is_empty() || config.fold_budget == 0 {
        return Err(GenerationError::NeedsFold);
    }
    if config.allowed_brushes.is_empty() || config.stroke_budget == 0 {
        return Err(GenerationError::NeedsBrush);
    }
    if config.minimum_source_actions < 2
        || config.minimum_source_actions > config.maximum_source_actions
        || config.maximum_source_actions > MAX_ACTIONS
    {
        return Err(GenerationError::SourceActionRange {
            minimum: config.minimum_source_actions,
            maximum: config.maximum_source_actions,
        });
    }
    let combined_budget = u16::from(config.fold_budget) + u16::from(config.stroke_budget);
    if u16::from(config.maximum_source_actions) > combined_budget {
        return Err(GenerationError::SourceActionsExceedBudget {
            maximum: config.maximum_source_actions,
            budget: u8::try_from(combined_budget)
                .expect("a combined budget below the source maximum must fit u8"),
        });
    }
    Ok(())
}

const fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

const fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn daily_seed_v1(date: CalendarDate) -> u64 {
    let text = format!("orifude:1:{date}");
    fnv1a64(text.as_bytes())
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
