use orifude::domain::paper::{BrushRule, Fold, FoldDirection, PaperAction};
use orifude::domain::puzzle::{
    IdentityErrorReason, IdentityPart, MAX_ALLOWED_FOLDS, MAX_BRUSH_RULES, MAX_ID_BYTES,
    PuzzleError,
};
use orifude::domain::score::Par;
use orifude::generator::{
    CalendarDate, CandidateRejection, GenerationError, GenerationOutcome, GenerationSeed,
    Generator, GeneratorConfig, MAX_GENERATION_ATTEMPTS,
};
use orifude::solver::{
    CancellationFlag, ExhaustionReason, MAX_SOLVER_DEPTH, MAX_SOLVER_MEMORY_BYTES, NeverCancel,
    SolverLimits,
};

fn working_generator(attempts: u16) -> Generator {
    Generator::new(
        GeneratorConfig::new("generated-tests", 4, 4)
            .expect("the generator identity should be valid")
            .with_rules(
                vec![Fold::new(FoldDirection::Right, 1)],
                vec![BrushRule::Dot],
            )
            .expect("the generator rules should fit their storage bounds")
            .with_budgets(1, 1)
            .with_attempt_limit(attempts),
    )
    .expect("the generation policy should be valid")
}

fn generated(outcome: GenerationOutcome) -> orifude::generator::GeneratedPuzzle {
    match outcome {
        GenerationOutcome::Generated { puzzle, .. } => *puzzle,
        other => panic!("expected a generated puzzle, found {other:?}"),
    }
}

#[test]
fn calendar_dates_and_daily_seeds_are_explicit_and_stable() {
    assert!(CalendarDate::new(2024, 2, 29).is_ok());
    assert!(CalendarDate::new(2023, 2, 29).is_err());
    assert!(CalendarDate::new(0, 1, 1).is_err());
    assert!(CalendarDate::new(2026, 13, 1).is_err());

    let date = CalendarDate::new(2026, 9, 1).expect("the fixture date should be valid");
    let current = GenerationSeed::for_date(date);
    let preserved = GenerationSeed::for_date_with_version(1, date)
        .expect("the first compatibility path should remain supported");

    assert_eq!(current, preserved);
    assert_eq!(current.to_string(), "v1-d049608d2164f808");
}

#[test]
fn generation_is_reproducible_valid_and_replay_verified() {
    let generator = working_generator(32);
    let seed = GenerationSeed::current(1);

    let (first, first_stats) = match generator.generate(seed, &NeverCancel) {
        GenerationOutcome::Generated { puzzle, stats } => (*puzzle, stats),
        other => panic!("expected a generated puzzle, found {other:?}"),
    };
    let second = generated(generator.generate(seed, &NeverCancel));

    assert_eq!(first, second);
    assert_eq!(first.seed(), seed);
    assert_eq!(first.candidate_attempt(), 1);
    assert_eq!(first_stats.solver_calls(), 2);
    assert!(!first.puzzle().target().is_empty());
    assert_eq!(
        first.puzzle().par(),
        Some(Par::new(
            first.solution().score().folds(),
            first.solution().score().strokes(),
        ))
    );
    assert!(first.solution().score().folds().get() > 0);
    assert!(first.solution().score().strokes().get() > 0);
    let replayed = first
        .solution()
        .replay()
        .execute(first.puzzle())
        .expect("the generated solution should match the final puzzle revision");
    assert!(replayed.result().is_success());
    assert_eq!(replayed.result().score(), first.solution().score());
}

#[test]
fn preserved_daily_output_has_a_cross_platform_golden() {
    let generator = working_generator(32);
    let date = CalendarDate::new(2026, 9, 1).expect("the fixture date should be valid");
    let seed = GenerationSeed::for_date_with_version(1, date)
        .expect("the preserved compatibility path should remain supported");

    let result = generated(generator.generate(seed, &NeverCancel));
    let target: Vec<_> = result
        .puzzle()
        .target()
        .cell_ids()
        .map(orifude::domain::paper::CellId::get)
        .collect();

    assert_eq!(target, [0, 1]);
    let coordinate = result
        .puzzle()
        .dimensions()
        .coordinate(0, 1)
        .expect("the golden coordinate should be valid");
    assert_eq!(
        result.solution().replay().actions(),
        [
            PaperAction::Fold(Fold::new(FoldDirection::Right, 1)),
            PaperAction::Dot(coordinate),
        ]
    );
    assert_eq!(result.candidate_attempt(), 0);
    assert_eq!(
        result.puzzle().identity().puzzle_id(),
        "paper-v1-d049608d2164f808-0"
    );
}

#[test]
fn generation_attempts_end_at_the_configured_bound() {
    let generator = Generator::new(
        GeneratorConfig::new("generated-tests", 4, 4)
            .expect("the generator identity should be valid")
            .with_rules(
                vec![Fold::new(FoldDirection::Left, 1)],
                vec![BrushRule::Dot],
            )
            .expect("the generator rules should fit their storage bounds")
            .with_budgets(1, 1)
            .with_attempt_limit(3),
    )
    .expect("the bounded failure policy should be valid");
    let seed = GenerationSeed::current(11);

    let outcome = generator.generate(seed, &NeverCancel);

    assert!(matches!(
        outcome,
        GenerationOutcome::Exhausted {
            seed: found,
            last_rejection: CandidateRejection::InvalidSequence,
            stats,
        } if found == seed
            && stats.attempted_candidates() == 3
            && stats.solver_calls() == 0
    ));
}

#[test]
fn solver_exhaustion_rejects_one_candidate_without_ending_generation() {
    let generator = Generator::new(
        GeneratorConfig::new("generated-tests", 4, 4)
            .expect("the generator identity should be valid")
            .with_rules(
                vec![Fold::new(FoldDirection::Right, 1)],
                vec![BrushRule::Dot],
            )
            .expect("the generator rules should fit their storage bounds")
            .with_budgets(1, 1)
            .with_attempt_limit(32)
            .with_solver_limits(SolverLimits::new(
                1,
                MAX_SOLVER_MEMORY_BYTES,
                MAX_SOLVER_DEPTH,
            )),
    )
    .expect("the bounded exhaustion policy should be valid");
    let seed = GenerationSeed::current(23);

    let outcome = generator.generate(seed, &NeverCancel);
    let (found_seed, last_rejection, stats) = match outcome {
        GenerationOutcome::Exhausted {
            seed,
            last_rejection,
            stats,
        } => (seed, last_rejection, stats),
        other => panic!("expected bounded generation exhaustion, found {other:?}"),
    };

    assert_eq!(found_seed, seed);
    assert!(matches!(
        last_rejection,
        CandidateRejection::DuplicateTarget
            | CandidateRejection::SolverExhausted(ExhaustionReason::VisitedStates)
    ));
    assert_eq!(stats.attempted_candidates(), 32);
    assert!(stats.solver_calls() > 0);
    assert!(stats.duplicate_candidates() > 0);
    assert_eq!(
        stats.solver_calls() + stats.duplicate_candidates(),
        stats.attempted_candidates()
    );
}

#[test]
fn generation_reports_cancellation_and_invalid_compatibility() {
    let generator = working_generator(8);
    let cancelled = CancellationFlag::new();
    cancelled.cancel();
    let seed = GenerationSeed::current(17);

    assert!(matches!(
        generator.generate(seed, &cancelled),
        GenerationOutcome::Cancelled { seed: found, stats }
            if found == seed && stats.attempted_candidates() == 0
    ));
    assert!(matches!(
        generator.generate(GenerationSeed::new(2, 17), &NeverCancel),
        GenerationOutcome::Invalid(GenerationError::UnsupportedCompatibility {
            found: 2,
            supported: 1,
        })
    ));
}

#[test]
fn generator_configuration_rejects_values_above_its_storage_bounds() {
    let oversized_id = "a".repeat(MAX_ID_BYTES + 1);
    assert_eq!(
        GeneratorConfig::new(&oversized_id, 4, 4),
        Err(GenerationError::Puzzle(PuzzleError::InvalidIdentity {
            part: IdentityPart::Pack,
            reason: IdentityErrorReason::TooLong,
        }))
    );

    let base = GeneratorConfig::new("generated-tests", 4, 4)
        .expect("the generator identity should be valid");
    assert_eq!(
        base.clone().with_rules(
            vec![Fold::new(FoldDirection::Right, 1); MAX_ALLOWED_FOLDS + 1],
            vec![BrushRule::Dot],
        ),
        Err(GenerationError::Puzzle(PuzzleError::TooManyAllowedFolds {
            count: MAX_ALLOWED_FOLDS + 1,
            limit: MAX_ALLOWED_FOLDS,
        }))
    );
    assert_eq!(
        base.with_rules(
            vec![Fold::new(FoldDirection::Right, 1)],
            vec![BrushRule::Dot; MAX_BRUSH_RULES + 1],
        ),
        Err(GenerationError::Puzzle(PuzzleError::TooManyBrushRules {
            count: MAX_BRUSH_RULES + 1,
            limit: MAX_BRUSH_RULES,
        }))
    );
}

#[test]
fn generator_rejects_unbounded_or_incomplete_policies() {
    let base = GeneratorConfig::new("generated-tests", 4, 4)
        .expect("the generator identity should be valid");
    assert_eq!(
        Generator::new(base.clone()),
        Err(GenerationError::NeedsFold)
    );

    let rules = base
        .with_rules(
            vec![Fold::new(FoldDirection::Right, 1)],
            vec![BrushRule::Dot],
        )
        .expect("the generator rules should fit their storage bounds");
    assert_eq!(
        Generator::new(rules.clone().with_budgets(1, 1).with_attempt_limit(0)),
        Err(GenerationError::AttemptLimit {
            found: 0,
            maximum: MAX_GENERATION_ATTEMPTS,
        })
    );
    assert!(matches!(
        Generator::new(
            rules
                .clone()
                .with_budgets(1, 1)
                .with_attempt_limit(MAX_GENERATION_ATTEMPTS + 1)
        ),
        Err(GenerationError::AttemptLimit { .. })
    ));
    assert!(matches!(
        Generator::new(rules.with_budgets(1, 1).with_source_action_range(1, 2)),
        Err(GenerationError::SourceActionRange { .. })
    ));

    let oversized = GeneratorConfig::new("generated-tests", 4, 4)
        .expect("the generator identity should be valid")
        .with_rules(
            vec![Fold::new(FoldDirection::Right, 1)],
            vec![BrushRule::Dot],
        )
        .expect("the generator rules should fit their storage bounds")
        .with_budgets(u8::MAX, u8::MAX);
    assert!(matches!(
        Generator::new(oversized),
        Err(GenerationError::Puzzle(_))
    ));
}
