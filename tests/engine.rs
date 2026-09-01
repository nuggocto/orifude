use orifude::domain::attempt::{ActionError, Attempt};
use orifude::domain::paper::{
    BrushRule, CellId, Coordinate, Fold, FoldCount, FoldDirection, InkPattern, LineStroke,
    MAX_ACTIONS, MAX_PHYSICAL_CELLS, PaperAction, PaperError, Row, StackView, StrokeAxis,
    StrokeCount,
};
use orifude::domain::puzzle::{
    CURRENT_PUZZLE_FORMAT_VERSION, IdentityErrorReason, IdentityPart, MAX_ALLOWED_FOLDS,
    MAX_BRUSH_RULES, Puzzle, PuzzleError, PuzzleIdentity, PuzzleSpec,
};
use orifude::domain::replay::{ENGINE_COMPATIBILITY_VERSION, Replay, ReplayError, ReplayMetadata};
use orifude::domain::score::{Par, Score};

fn identity(name: &str) -> PuzzleIdentity {
    PuzzleIdentity::new("official", name).expect("the test identity should be valid")
}

fn cell(value: u8) -> CellId {
    CellId::new(value).expect("the test cell should be globally valid")
}

fn coordinate(puzzle: &Puzzle, row: u8, column: u8) -> Coordinate {
    puzzle
        .dimensions()
        .coordinate(row, column)
        .expect("the test coordinate should belong to the puzzle")
}

fn fold_puzzle(width: u8, height: u8, folds: Vec<Fold>, budget: u8) -> Puzzle {
    Puzzle::new(
        PuzzleSpec::new(identity("fold-paper"), width, height)
            .with_allowed_folds(folds)
            .with_budgets(budget, 0),
    )
    .expect("the fold puzzle should be valid")
}

fn ink_puzzle(
    width: u8,
    height: u8,
    target: Vec<CellId>,
    brushes: Vec<BrushRule>,
    budget: u8,
) -> Puzzle {
    Puzzle::new(
        PuzzleSpec::new(identity("ink-paper"), width, height)
            .with_target_cells(target)
            .with_allowed_brushes(brushes)
            .with_budgets(0, budget),
    )
    .expect("the ink puzzle should be valid")
}

fn stack_ids(attempt: &Attempt, row: u8, column: u8) -> Vec<u8> {
    let mut stack = StackView::new();
    let coordinate = attempt
        .dimensions()
        .coordinate(row, column)
        .expect("the test coordinate should be valid");
    attempt
        .stack_at(coordinate, &mut stack)
        .expect("the stack should be readable");
    stack.cell_ids().iter().map(|id| id.get()).collect()
}

fn assert_observable_invariants(attempt: &Attempt) {
    let dimensions = attempt.dimensions();
    let mut seen = [false; MAX_PHYSICAL_CELLS];
    let mut stack = StackView::new();

    for row in 0..dimensions.height().get() {
        for column in 0..dimensions.width().get() {
            let coordinate = dimensions
                .coordinate(row, column)
                .expect("an enumerated coordinate should be valid");
            attempt
                .stack_at(coordinate, &mut stack)
                .expect("an enumerated stack should be readable");
            for (layer, &cell_id) in stack.cell_ids().iter().enumerate() {
                assert!(!seen[cell_id.index()]);
                seen[cell_id.index()] = true;
                let physical = attempt
                    .physical_cell(cell_id)
                    .expect("every stack identity should resolve");
                assert_eq!(physical.coordinate(), coordinate);
                assert_eq!(usize::from(physical.layer().get()), layer);
            }
        }
    }

    assert!(
        seen[..dimensions.cell_count()]
            .iter()
            .all(|present| *present)
    );
    assert!(
        seen[dimensions.cell_count()..]
            .iter()
            .all(|present| !present)
    );
    assert_eq!(
        usize::from(attempt.action_count().get()),
        attempt.actions().count()
    );
}

#[test]
fn puzzle_construction_canonicalizes_valid_rules() {
    let right = Fold::new(FoldDirection::Right, 2);
    let left = Fold::new(FoldDirection::Left, 2);
    let par = Par::new(
        FoldCount::new(1).expect("one fold is valid"),
        StrokeCount::new(1).expect("one stroke is valid"),
    );
    let puzzle = Puzzle::new(
        PuzzleSpec::new(identity("first-paper"), 4, 4)
            .with_target_cells(vec![cell(5), cell(6)])
            .with_allowed_folds(vec![right, left])
            .with_allowed_brushes(vec![
                BrushRule::Line {
                    axis: StrokeAxis::Horizontal,
                    length: 2,
                },
                BrushRule::Dot,
            ])
            .with_budgets(2, 2)
            .with_par(par),
    )
    .expect("the complete puzzle should validate");

    assert_eq!(puzzle.format_version(), CURRENT_PUZZLE_FORMAT_VERSION);
    assert_eq!(puzzle.identity().pack_id(), "official");
    assert_eq!(puzzle.identity().puzzle_id(), "first-paper");
    assert_eq!(puzzle.allowed_folds(), [left, right]);
    assert_eq!(
        puzzle.allowed_brushes(),
        [
            BrushRule::Dot,
            BrushRule::Line {
                axis: StrokeAxis::Horizontal,
                length: 2,
            }
        ]
    );
    assert_eq!(puzzle.par(), Some(par));
    assert_observable_invariants(&puzzle.start());
}

#[test]
fn an_attempt_owns_the_validated_puzzle_it_needs() {
    let attempt = {
        let puzzle = ink_puzzle(4, 4, Vec::new(), vec![BrushRule::Dot], 1);
        puzzle.start()
    };

    assert_eq!(attempt.puzzle().identity().puzzle_id(), "ink-paper");
    assert_observable_invariants(&attempt);
}

#[test]
fn puzzle_identity_enforces_the_portable_ascii_grammar() {
    let cases = [
        ("", IdentityErrorReason::Empty),
        ("Upper", IdentityErrorReason::InvalidCharacter),
        ("two--parts", IdentityErrorReason::InvalidHyphen),
        ("trailing-", IdentityErrorReason::InvalidHyphen),
    ];
    for (puzzle_id, reason) in cases {
        assert_eq!(
            PuzzleIdentity::new("official", puzzle_id),
            Err(PuzzleError::InvalidIdentity {
                part: IdentityPart::Puzzle,
                reason,
            })
        );
    }

    let oversized = "a".repeat(65);
    assert_eq!(
        PuzzleIdentity::new(&oversized, "paper"),
        Err(PuzzleError::InvalidIdentity {
            part: IdentityPart::Pack,
            reason: IdentityErrorReason::TooLong,
        })
    );
}

#[test]
fn puzzle_construction_rejects_malformed_rules_and_budgets() {
    let left = Fold::new(FoldDirection::Left, 2);
    let cases = [
        (
            PuzzleSpec::new(identity("bad-version"), 4, 4).with_format_version(2),
            PuzzleError::UnsupportedFormatVersion {
                found: 2,
                supported: CURRENT_PUZZLE_FORMAT_VERSION,
            },
        ),
        (
            PuzzleSpec::new(identity("duplicate-target"), 4, 4)
                .with_target_cells(vec![cell(1), cell(1)])
                .with_allowed_brushes(vec![BrushRule::Dot])
                .with_budgets(0, 1),
            PuzzleError::DuplicateTargetCell { cell_id: cell(1) },
        ),
        (
            PuzzleSpec::new(identity("duplicate-fold"), 4, 4)
                .with_allowed_folds(vec![left, left])
                .with_budgets(1, 0),
            PuzzleError::DuplicateAllowedFold { fold: left },
        ),
        (
            PuzzleSpec::new(identity("bad-crease"), 4, 4)
                .with_allowed_folds(vec![Fold::new(FoldDirection::Left, 4)])
                .with_budgets(1, 0),
            PuzzleError::AllowedCreaseOutsidePaper {
                fold: Fold::new(FoldDirection::Left, 4),
                extent: 4,
            },
        ),
        (
            PuzzleSpec::new(identity("short-line"), 4, 4)
                .with_allowed_brushes(vec![BrushRule::Line {
                    axis: StrokeAxis::Horizontal,
                    length: 1,
                }])
                .with_budgets(0, 1),
            PuzzleError::LineLengthOutOfRange {
                axis: StrokeAxis::Horizontal,
                length: 1,
                extent: 4,
            },
        ),
        (
            PuzzleSpec::new(identity("fold-budget"), 4, 4).with_budgets(1, 0),
            PuzzleError::FoldBudgetWithoutRules,
        ),
        (
            PuzzleSpec::new(identity("stroke-budget"), 4, 4).with_budgets(0, 1),
            PuzzleError::StrokeBudgetWithoutRules,
        ),
        (
            PuzzleSpec::new(identity("target-budget"), 4, 4).with_target_cells(vec![cell(0)]),
            PuzzleError::TargetNeedsStroke,
        ),
    ];

    for (spec, expected) in cases {
        assert_eq!(Puzzle::new(spec), Err(expected));
    }
}

#[test]
fn puzzle_par_cannot_exceed_action_budgets() {
    let par = Par::new(
        FoldCount::new(2).expect("two folds are valid"),
        StrokeCount::new(1).expect("one stroke is valid"),
    );
    let spec = PuzzleSpec::new(identity("bad-par"), 4, 4)
        .with_allowed_folds(vec![Fold::new(FoldDirection::Left, 2)])
        .with_allowed_brushes(vec![BrushRule::Dot])
        .with_budgets(1, 1)
        .with_par(par);
    assert_eq!(
        Puzzle::new(spec),
        Err(PuzzleError::ParExceedsBudget {
            par,
            fold_budget: FoldCount::new(1).expect("one fold is valid"),
            stroke_budget: StrokeCount::new(1).expect("one stroke is valid"),
        })
    );
}

#[test]
fn puzzle_rule_collections_enforce_zero_one_maximum_and_maximum_plus_one() {
    let mut every_fold = Vec::new();
    for crease in 1..12 {
        every_fold.push(Fold::new(FoldDirection::Left, crease));
        every_fold.push(Fold::new(FoldDirection::Right, crease));
        every_fold.push(Fold::new(FoldDirection::Up, crease));
        every_fold.push(Fold::new(FoldDirection::Down, crease));
    }
    assert_eq!(every_fold.len(), MAX_ALLOWED_FOLDS);
    let maximum = Puzzle::new(
        PuzzleSpec::new(identity("maximum-rules"), 12, 12)
            .with_allowed_folds(every_fold)
            .with_budgets(12, 0),
    )
    .expect("every distinct fold rule should fit the maximum board");
    assert_eq!(maximum.allowed_folds().len(), MAX_ALLOWED_FOLDS);

    let too_many_folds = vec![Fold::new(FoldDirection::Left, 1); MAX_ALLOWED_FOLDS + 1];
    assert_eq!(
        Puzzle::new(
            PuzzleSpec::new(identity("too-many-folds"), 12, 12)
                .with_allowed_folds(too_many_folds)
                .with_budgets(1, 0)
        ),
        Err(PuzzleError::TooManyAllowedFolds {
            count: MAX_ALLOWED_FOLDS + 1,
            limit: MAX_ALLOWED_FOLDS,
        })
    );

    let mut every_brush = vec![BrushRule::Dot];
    for length in 2..=12 {
        every_brush.push(BrushRule::Line {
            axis: StrokeAxis::Horizontal,
            length,
        });
        every_brush.push(BrushRule::Line {
            axis: StrokeAxis::Vertical,
            length,
        });
    }
    assert_eq!(every_brush.len(), MAX_BRUSH_RULES);
    let maximum = Puzzle::new(
        PuzzleSpec::new(identity("maximum-brushes"), 12, 12)
            .with_allowed_brushes(every_brush)
            .with_budgets(0, 8),
    )
    .expect("every distinct brush rule should fit the maximum board");
    assert_eq!(maximum.allowed_brushes().len(), MAX_BRUSH_RULES);

    assert_eq!(
        Puzzle::new(
            PuzzleSpec::new(identity("too-many-brushes"), 12, 12)
                .with_allowed_brushes(vec![BrushRule::Dot; MAX_BRUSH_RULES + 1])
                .with_budgets(0, 1)
        ),
        Err(PuzzleError::TooManyBrushRules {
            count: MAX_BRUSH_RULES + 1,
            limit: MAX_BRUSH_RULES,
        })
    );
}

#[test]
fn maximum_length_line_inks_one_complete_board_edge() {
    let target = (0..12).map(cell).collect::<Vec<_>>();
    let puzzle = ink_puzzle(
        12,
        12,
        target,
        vec![BrushRule::Line {
            axis: StrokeAxis::Horizontal,
            length: 12,
        }],
        1,
    );
    let mut attempt = puzzle.start();
    let line = LineStroke::new(coordinate(&puzzle, 0, 11), coordinate(&puzzle, 0, 0));

    attempt
        .stamp_line(line)
        .expect("the maximum line should fit the board edge");

    assert_eq!(attempt.ink().len(), 12);
    assert!(attempt.result().is_success());
}

#[test]
fn consecutive_folds_may_use_different_allowed_creases_on_one_axis() {
    let outer = Fold::new(FoldDirection::Left, 2);
    let inner = Fold::new(FoldDirection::Left, 1);
    let puzzle = fold_puzzle(4, 4, vec![outer, inner], 2);
    let mut attempt = puzzle.start();

    attempt.fold(outer).expect("the first fold should succeed");
    attempt.fold(inner).expect("the second fold should succeed");

    assert_eq!(stack_ids(&attempt, 1, 0), [4, 7, 6, 5]);
    assert_observable_invariants(&attempt);
}

#[test]
fn three_same_axis_folds_build_an_eight_layer_stack() {
    let folds = [
        Fold::new(FoldDirection::Left, 4),
        Fold::new(FoldDirection::Left, 2),
        Fold::new(FoldDirection::Left, 1),
    ];
    let puzzle = fold_puzzle(8, 4, folds.to_vec(), 3);
    let mut attempt = puzzle.start();

    for fold in folds {
        attempt.fold(fold).expect("the nested fold should succeed");
    }

    assert_eq!(stack_ids(&attempt, 1, 0), [8, 15, 12, 11, 10, 13, 14, 9]);
    assert_observable_invariants(&attempt);
}

#[test]
fn off_center_folds_work_in_every_direction() {
    let cases = [
        (Fold::new(FoldDirection::Left, 3), 1, 1, [6, 9]),
        (Fold::new(FoldDirection::Right, 2), 1, 3, [8, 5]),
        (Fold::new(FoldDirection::Up, 3), 1, 1, [6, 21]),
        (Fold::new(FoldDirection::Down, 2), 3, 1, [16, 1]),
    ];

    for (fold, row, column, expected) in cases {
        let puzzle = fold_puzzle(5, 5, vec![fold], 1);
        let mut attempt = puzzle.start();
        attempt
            .fold(fold)
            .expect("the short side should fold inward");

        assert_eq!(stack_ids(&attempt, row, column), expected);
        assert_observable_invariants(&attempt);
    }
}

#[test]
fn illegal_folds_leave_the_complete_attempt_unchanged() {
    let allowed = Fold::new(FoldDirection::Left, 2);
    let disallowed = Fold::new(FoldDirection::Right, 2);
    let puzzle = fold_puzzle(5, 4, vec![allowed], 1);
    let mut attempt = puzzle.start();
    let before = attempt.clone();

    assert_eq!(
        attempt.fold(disallowed),
        Err(ActionError::FoldNotAllowed { fold: disallowed })
    );
    assert_eq!(attempt, before);

    assert_eq!(
        attempt.fold(allowed),
        Err(ActionError::Paper(PaperError::FoldLeavesPaper {
            direction: FoldDirection::Left,
            crease: 2,
            index: 4,
            extent: 5,
        }))
    );
    assert_eq!(attempt, before);
}

#[test]
fn empty_moving_sides_and_exhausted_budgets_do_not_mutate() {
    let left = Fold::new(FoldDirection::Left, 2);
    let puzzle = fold_puzzle(4, 4, vec![left], 1);
    let mut attempt = puzzle.start();
    attempt.fold(left).expect("the first fold should succeed");
    let before = attempt.clone();

    assert_eq!(
        attempt.fold(left),
        Err(ActionError::Paper(PaperError::FoldBudgetExhausted {
            limit: FoldCount::new(1).expect("one fold is valid"),
        }))
    );
    assert_eq!(attempt, before);

    let puzzle = fold_puzzle(4, 4, vec![left], 2);
    let mut attempt = puzzle.start();
    attempt.fold(left).expect("the first fold should succeed");
    let before = attempt.clone();
    assert_eq!(
        attempt.fold(left),
        Err(ActionError::Paper(PaperError::EmptyMovingSide {
            direction: FoldDirection::Left,
        }))
    );
    assert_eq!(attempt, before);
}

#[test]
fn horizontal_and_vertical_lines_ink_every_layer_in_their_footprint() {
    let left = Fold::new(FoldDirection::Left, 2);
    let puzzle = Puzzle::new(
        PuzzleSpec::new(identity("line-paper"), 4, 4)
            .with_target_cells(vec![cell(4), cell(5), cell(6), cell(7)])
            .with_allowed_folds(vec![left])
            .with_allowed_brushes(vec![
                BrushRule::Line {
                    axis: StrokeAxis::Horizontal,
                    length: 2,
                },
                BrushRule::Line {
                    axis: StrokeAxis::Vertical,
                    length: 2,
                },
            ])
            .with_budgets(1, 2),
    )
    .expect("the line puzzle should validate");
    let mut attempt = puzzle.start();
    attempt.fold(left).expect("the fold should succeed");
    let reversed = LineStroke::new(coordinate(&puzzle, 1, 1), coordinate(&puzzle, 1, 0));
    let forward = LineStroke::new(coordinate(&puzzle, 1, 0), coordinate(&puzzle, 1, 1));
    assert_eq!(reversed, forward);

    attempt
        .stamp_line(reversed)
        .expect("reversed endpoints should describe the same horizontal line");

    assert_eq!(
        attempt
            .ink()
            .cell_ids()
            .map(CellId::get)
            .collect::<Vec<_>>(),
        [4, 5, 6, 7]
    );
    assert!(attempt.result().is_success());
    assert_observable_invariants(&attempt);

    let vertical = LineStroke::new(coordinate(&puzzle, 0, 1), coordinate(&puzzle, 1, 1));
    attempt
        .stamp_line(vertical)
        .expect("the vertical line should ink both folded stacks");
    assert_eq!(
        attempt
            .ink()
            .cell_ids()
            .map(CellId::get)
            .collect::<Vec<_>>(),
        [1, 2, 4, 5, 6, 7]
    );
}

#[test]
fn invalid_or_disallowed_lines_do_not_ink_or_consume_a_stroke() {
    let puzzle = ink_puzzle(
        4,
        4,
        vec![cell(0), cell(1)],
        vec![BrushRule::Line {
            axis: StrokeAxis::Horizontal,
            length: 2,
        }],
        1,
    );
    let mut attempt = puzzle.start();
    let initial = attempt.clone();
    assert_eq!(
        attempt.stamp_dot(coordinate(&puzzle, 0, 0)),
        Err(ActionError::BrushNotAllowed {
            rule: BrushRule::Dot,
        })
    );
    assert_eq!(attempt, initial);

    let diagonal = LineStroke::new(coordinate(&puzzle, 0, 0), coordinate(&puzzle, 1, 1));
    assert!(matches!(
        attempt.stamp_line(diagonal),
        Err(ActionError::Paper(PaperError::LineIsNotAxisAligned { .. }))
    ));
    assert_eq!(attempt, initial);

    let point = LineStroke::new(coordinate(&puzzle, 0, 0), coordinate(&puzzle, 0, 0));
    assert_eq!(
        attempt.stamp_line(point),
        Err(ActionError::Paper(PaperError::LineIsTooShort))
    );
    assert_eq!(attempt, initial);

    let vertical = LineStroke::new(coordinate(&puzzle, 0, 0), coordinate(&puzzle, 1, 0));
    assert_eq!(
        attempt.stamp_line(vertical),
        Err(ActionError::BrushNotAllowed {
            rule: BrushRule::Line {
                axis: StrokeAxis::Vertical,
                length: 2,
            },
        })
    );
    assert_eq!(attempt, initial);

    let outside_row = Row::new(4).expect("row four is globally valid");
    let outside = LineStroke::new(
        Coordinate::new(outside_row, coordinate(&puzzle, 0, 0).column()),
        Coordinate::new(outside_row, coordinate(&puzzle, 0, 1).column()),
    );
    assert_eq!(
        attempt.stamp_line(outside),
        Err(ActionError::Paper(PaperError::CoordinateOutsidePaper {
            row: 4,
            column: 0,
            width: 4,
            height: 4,
        }))
    );
    assert_eq!(attempt, initial);
}

#[test]
fn a_line_crossing_an_empty_position_is_atomic() {
    let left = Fold::new(FoldDirection::Left, 2);
    let puzzle = Puzzle::new(
        PuzzleSpec::new(identity("empty-line"), 4, 4)
            .with_allowed_folds(vec![left])
            .with_allowed_brushes(vec![BrushRule::Line {
                axis: StrokeAxis::Horizontal,
                length: 2,
            }])
            .with_budgets(1, 1),
    )
    .expect("the empty-line puzzle should validate");
    let mut attempt = puzzle.start();
    attempt.fold(left).expect("the fold should succeed");
    let before = attempt.clone();
    let line = LineStroke::new(coordinate(&puzzle, 1, 1), coordinate(&puzzle, 1, 2));

    assert_eq!(
        attempt.stamp_line(line),
        Err(ActionError::Paper(PaperError::EmptyBrushPosition {
            coordinate: coordinate(&puzzle, 1, 2),
        }))
    );
    assert_eq!(attempt, before);
}

#[test]
fn results_report_missing_extra_success_and_par_without_changing_state() {
    let par = Par::new(
        FoldCount::new(0).expect("zero folds is valid"),
        StrokeCount::new(1).expect("one stroke is valid"),
    );
    let puzzle = Puzzle::new(
        PuzzleSpec::new(identity("result-paper"), 4, 4)
            .with_target_cells(vec![cell(0)])
            .with_allowed_brushes(vec![BrushRule::Dot])
            .with_budgets(0, 2)
            .with_par(par),
    )
    .expect("the result puzzle should validate");
    let mut attempt = puzzle.start();

    let before = attempt.result();
    assert!(!before.is_success());
    assert_eq!(
        before.comparison().missing().cell_ids().collect::<Vec<_>>(),
        [cell(0)]
    );
    assert_eq!(before.meets_par(), Some(false));

    attempt
        .stamp_dot(coordinate(&puzzle, 0, 0))
        .expect("the target dot should succeed");
    attempt.mark_hint_used();
    let exact = attempt.result();
    assert!(exact.is_success());
    assert_eq!(exact.meets_par(), Some(true));
    assert!(exact.hints_used());
    assert_eq!(
        exact.score(),
        Score::new(FoldCount::new(0).unwrap(), StrokeCount::new(1).unwrap())
    );

    attempt
        .stamp_dot(coordinate(&puzzle, 0, 1))
        .expect("the extra dot should succeed");
    let extra = attempt.result();
    assert!(!extra.is_success());
    assert_eq!(
        extra.comparison().extra().cell_ids().collect::<Vec<_>>(),
        [cell(1)]
    );
    assert_eq!(extra.meets_par(), Some(false));
}

#[test]
fn score_order_uses_folds_first_then_strokes() {
    let one_fold_many_strokes = Score::new(
        FoldCount::new(1).expect("one fold is valid"),
        StrokeCount::new(8).expect("eight strokes are valid"),
    );
    let two_folds_one_stroke = Score::new(
        FoldCount::new(2).expect("two folds are valid"),
        StrokeCount::new(1).expect("one stroke is valid"),
    );
    assert!(one_fold_many_strokes < two_folds_one_stroke);
}

#[test]
fn undo_and_reset_restore_canonical_state_and_replay_history() {
    let left = Fold::new(FoldDirection::Left, 2);
    let puzzle = Puzzle::new(
        PuzzleSpec::new(identity("history-paper"), 4, 4)
            .with_allowed_folds(vec![left])
            .with_allowed_brushes(vec![BrushRule::Dot])
            .with_budgets(1, 1),
    )
    .expect("the history puzzle should validate");
    let mut attempt = puzzle.start();
    let initial_key = attempt.state_key();
    attempt.fold(left).expect("the fold should succeed");
    let folded_key = attempt.state_key();
    attempt
        .stamp_dot(coordinate(&puzzle, 1, 1))
        .expect("the dot should succeed");

    attempt.undo().expect("the dot should undo");
    assert_eq!(attempt.state_key(), folded_key);
    assert_eq!(
        attempt.actions().collect::<Vec<_>>(),
        [PaperAction::Fold(left)]
    );
    assert_eq!(attempt.undo_count(), 1);

    attempt.mark_hint_used();
    attempt.reset();
    assert_eq!(attempt.state_key(), initial_key);
    assert_eq!(attempt.action_count().get(), 0);
    assert_eq!(attempt.actions().count(), 0);
    assert_eq!(attempt.undo_count(), 1);
    assert!(attempt.hints_used());
}

#[test]
fn canonical_keys_match_exact_state_and_have_a_stable_hash() {
    let left = Fold::new(FoldDirection::Left, 2);
    let puzzle = fold_puzzle(4, 4, vec![left], 1);
    let mut first = puzzle.start();
    let second = puzzle.start();

    assert_eq!(first.state_key(), second.state_key());
    assert_eq!(
        first.state_key().stable_hash(),
        second.state_key().stable_hash()
    );
    assert_eq!(second.state_key().stable_hash(), 1_914_948_199_483_280_709);
    first.fold(left).expect("the fold should succeed");
    assert_ne!(first.state_key(), second.state_key());
    assert_ne!(
        first.state_key().stable_hash(),
        second.state_key().stable_hash()
    );
    first.undo().expect("the fold should undo");
    assert_eq!(first.state_key(), second.state_key());
}

#[test]
fn replay_matches_direct_execution_and_excludes_undone_actions() {
    let left = Fold::new(FoldDirection::Left, 2);
    let puzzle = Puzzle::new(
        PuzzleSpec::new(identity("replay-paper"), 4, 4)
            .with_target_cells(vec![cell(5), cell(6)])
            .with_allowed_folds(vec![left])
            .with_allowed_brushes(vec![BrushRule::Dot])
            .with_budgets(1, 2),
    )
    .expect("the replay puzzle should validate");
    let mut direct = puzzle.start();
    direct.fold(left).expect("the fold should succeed");
    direct
        .stamp_dot(coordinate(&puzzle, 1, 1))
        .expect("the target dot should succeed");
    direct
        .stamp_dot(coordinate(&puzzle, 0, 0))
        .expect("the extra dot should succeed");
    direct.undo().expect("the extra dot should undo");

    let replay = Replay::from_attempt(&direct);
    assert_eq!(replay.actions().len(), 2);
    let replayed = replay.execute(&puzzle).expect("the replay should execute");

    assert_eq!(replayed.state_key(), direct.state_key());
    assert_eq!(replayed.result().comparison(), direct.result().comparison());
    assert_eq!(
        replayed.state_key().stable_hash(),
        14_918_048_313_021_682_743
    );
}

#[test]
fn replay_rejects_a_revised_puzzle_under_the_same_identity() {
    let left = Fold::new(FoldDirection::Left, 2);
    let original = fold_puzzle(4, 4, vec![left], 1);
    let mut attempt = original.start();
    attempt
        .fold(left)
        .expect("the original fold should succeed");
    let replay = Replay::from_attempt(&attempt);

    let revised = fold_puzzle(4, 4, vec![left], 2);

    assert_eq!(replay.metadata().puzzle_revision(), &original.revision());
    assert_ne!(replay.metadata().puzzle_revision(), &revised.revision());
    assert_eq!(
        replay.execute(&revised),
        Err(ReplayError::PuzzleRevisionMismatch)
    );
}

#[test]
fn replay_rejects_oversized_incompatible_and_foreign_data() {
    let puzzle = ink_puzzle(4, 4, Vec::new(), vec![BrushRule::Dot], 1);
    let dot = PaperAction::Dot(coordinate(&puzzle, 0, 0));
    let metadata = ReplayMetadata::current(&puzzle);
    let maximum = Replay::new(metadata.clone(), vec![dot; usize::from(MAX_ACTIONS)])
        .expect("the replay action limit should be accepted");
    assert_eq!(maximum.actions().len(), usize::from(MAX_ACTIONS));
    assert_eq!(
        Replay::new(metadata.clone(), vec![dot; usize::from(MAX_ACTIONS) + 1]),
        Err(ReplayError::TooManyActions {
            count: usize::from(MAX_ACTIONS) + 1,
            limit: MAX_ACTIONS,
        })
    );

    let incompatible = Replay::new(
        ReplayMetadata::new(
            puzzle.identity().clone(),
            puzzle.revision(),
            puzzle.format_version(),
            ENGINE_COMPATIBILITY_VERSION + 1,
        ),
        Vec::new(),
    )
    .expect("the bounded replay should construct");
    assert_eq!(
        incompatible.execute(&puzzle),
        Err(ReplayError::IncompatibleEngine {
            found: ENGINE_COMPATIBILITY_VERSION + 1,
            supported: ENGINE_COMPATIBILITY_VERSION,
        })
    );

    let foreign_identity = PuzzleIdentity::new("official", "another-paper")
        .expect("the foreign identity should be valid");
    let foreign = Replay::new(
        ReplayMetadata::new(
            foreign_identity,
            puzzle.revision(),
            puzzle.format_version(),
            ENGINE_COMPATIBILITY_VERSION,
        ),
        Vec::new(),
    )
    .expect("the bounded replay should construct");
    assert_eq!(
        foreign.execute(&puzzle),
        Err(ReplayError::PuzzleIdentityMismatch)
    );

    let wrong_format = Replay::new(
        ReplayMetadata::new(
            puzzle.identity().clone(),
            puzzle.revision(),
            puzzle.format_version() + 1,
            ENGINE_COMPATIBILITY_VERSION,
        ),
        Vec::new(),
    )
    .expect("the bounded replay should construct");
    assert_eq!(
        wrong_format.execute(&puzzle),
        Err(ReplayError::IncompatiblePuzzleFormat {
            found: puzzle.format_version() + 1,
            expected: puzzle.format_version(),
        })
    );
}

#[test]
fn replay_reports_the_failing_action_without_touching_a_live_attempt() {
    let puzzle = ink_puzzle(4, 4, Vec::new(), vec![BrushRule::Dot], 1);
    let coordinate = coordinate(&puzzle, 0, 0);
    let replay = Replay::new(
        ReplayMetadata::current(&puzzle),
        vec![PaperAction::Dot(coordinate), PaperAction::Dot(coordinate)],
    )
    .expect("the replay is within its static bound");
    let live = puzzle.start();
    let before = live.state_key();

    assert_eq!(
        replay.execute(&puzzle),
        Err(ReplayError::Action {
            index: 1,
            source: ActionError::Paper(PaperError::StrokeBudgetExhausted {
                limit: StrokeCount::new(1).expect("one stroke is valid"),
            }),
        })
    );
    assert_eq!(live.state_key(), before);
}

#[test]
fn fold_and_failed_action_properties_hold_across_every_board_boundary() {
    let mut case_count = 0_usize;
    for width in 4..=12 {
        for height in 4..=12 {
            for direction in [
                FoldDirection::Left,
                FoldDirection::Right,
                FoldDirection::Up,
                FoldDirection::Down,
            ] {
                let extent = match direction.axis() {
                    orifude::domain::paper::FoldAxis::Vertical => width,
                    orifude::domain::paper::FoldAxis::Horizontal => height,
                };
                for crease in 1..extent {
                    case_count += 1;
                    let fold = Fold::new(direction, crease);
                    let puzzle = fold_puzzle(width, height, vec![fold], 1);
                    let mut attempt = puzzle.start();
                    let before = attempt.state_key();
                    match attempt.fold(fold) {
                        Ok(()) => {
                            assert_observable_invariants(&attempt);
                            attempt.undo().expect("every legal fold should undo");
                            assert_eq!(attempt.state_key(), before);
                        }
                        Err(_) => assert_eq!(attempt.state_key(), before),
                    }
                }
            }
        }
    }
    assert_eq!(case_count, 2_268);
}

#[test]
fn replay_and_direct_execution_property_holds_for_fixed_action_sequences() {
    const SEEDS: [u64; 8] = [0, 1, 2, 3, 0x55aa, 0xdead_beef, u32::MAX as u64, u64::MAX];
    const ACTIONS_PER_SEED: usize = 32;
    let folds = [
        Fold::new(FoldDirection::Left, 1),
        Fold::new(FoldDirection::Left, 2),
        Fold::new(FoldDirection::Left, 3),
        Fold::new(FoldDirection::Right, 1),
        Fold::new(FoldDirection::Right, 2),
        Fold::new(FoldDirection::Right, 3),
        Fold::new(FoldDirection::Up, 1),
        Fold::new(FoldDirection::Up, 2),
        Fold::new(FoldDirection::Up, 3),
        Fold::new(FoldDirection::Down, 1),
        Fold::new(FoldDirection::Down, 2),
        Fold::new(FoldDirection::Down, 3),
    ];
    let puzzle = Puzzle::new(
        PuzzleSpec::new(identity("property-paper"), 4, 4)
            .with_allowed_folds(folds.to_vec())
            .with_allowed_brushes(vec![BrushRule::Dot])
            .with_budgets(12, 8),
    )
    .expect("the property puzzle should validate");

    for seed in SEEDS {
        let mut state = seed;
        let mut attempt = puzzle.start();
        for _ in 0..ACTIONS_PER_SEED {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let before = attempt.clone();
            let action = if state & 1 == 0 {
                let index = usize::try_from(state % folds.len() as u64)
                    .expect("the bounded fold index should fit usize");
                PaperAction::Fold(folds[index])
            } else {
                let row = u8::try_from((state >> 8) % 4).expect("row should fit u8");
                let column = u8::try_from((state >> 16) % 4).expect("column should fit u8");
                PaperAction::Dot(coordinate(&puzzle, row, column))
            };
            if attempt.apply(action).is_err() {
                assert_eq!(attempt, before);
            } else {
                assert_observable_invariants(&attempt);
            }
        }

        let replay = Replay::from_attempt(&attempt);
        let replayed = replay
            .execute(&puzzle)
            .expect("successful direct actions should replay");
        assert_eq!(replayed.state_key(), attempt.state_key(), "seed {seed}");
    }
}

#[test]
fn target_pattern_rejects_a_foreign_cell_at_the_construction_boundary() {
    let spec = PuzzleSpec::new(identity("foreign-target"), 4, 4)
        .with_target_cells(vec![cell(16)])
        .with_allowed_brushes(vec![BrushRule::Dot])
        .with_budgets(0, 1);
    assert_eq!(
        Puzzle::new(spec),
        Err(PuzzleError::Paper(PaperError::CellOutsidePaper {
            cell_id: cell(16),
            cell_count: 16,
        }))
    );

    let dimensions =
        orifude::domain::paper::Dimensions::new(4, 4).expect("the dimensions should be valid");
    assert_eq!(
        InkPattern::from_cell_ids(dimensions, &[cell(16)]),
        Err(PaperError::CellOutsidePaper {
            cell_id: cell(16),
            cell_count: 16,
        })
    );
}
