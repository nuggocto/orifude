use orifude::domain::paper::{
    ActionCount, CellId, Column, Coordinate, Face, Fold, FoldAxis, FoldCount, FoldDirection,
    Height, InkPattern, MAX_ACTIONS, MAX_BOARD_HEIGHT, MAX_BOARD_WIDTH, MAX_FOLD_ACTIONS,
    MAX_PHYSICAL_CELLS, MAX_STROKE_ACTIONS, MIN_BOARD_HEIGHT, MIN_BOARD_WIDTH, Orientation, Paper,
    PaperError, PaperSpec, Row, StackView, StrokeCount, Width,
};

fn paper(width: u8, height: u8) -> Paper {
    Paper::new(PaperSpec::new(
        width,
        height,
        MAX_FOLD_ACTIONS,
        MAX_STROKE_ACTIONS,
        MAX_ACTIONS,
    ))
    .expect("the test paper should be valid")
}

fn coordinate(paper: &Paper, row: u8, column: u8) -> Coordinate {
    paper
        .dimensions()
        .coordinate(row, column)
        .expect("the test coordinate should be valid")
}

fn stack_ids(paper: &Paper, row: u8, column: u8) -> Vec<u8> {
    let mut stack = StackView::new();
    paper
        .stack_at(coordinate(paper, row, column), &mut stack)
        .expect("the test stack should be readable");
    stack
        .cell_ids()
        .iter()
        .map(|cell_id| cell_id.get())
        .collect()
}

fn assert_observable_invariants(paper: &Paper) {
    let dimensions = paper.dimensions();
    let mut seen = [false; MAX_PHYSICAL_CELLS];
    let mut stack = StackView::new();

    for row in 0..dimensions.height().get() {
        for column in 0..dimensions.width().get() {
            let coordinate = dimensions
                .coordinate(row, column)
                .expect("an enumerated coordinate should be valid");
            paper
                .stack_at(coordinate, &mut stack)
                .expect("an enumerated stack should be readable");

            for (layer, &cell_id) in stack.cell_ids().iter().enumerate() {
                assert!(!seen[cell_id.index()]);
                seen[cell_id.index()] = true;
                let cell = paper
                    .physical_cell(cell_id)
                    .expect("every stack identity should resolve");
                assert_eq!(cell.coordinate(), coordinate);
                assert_eq!(usize::from(cell.layer().get()), layer);
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
    assert_eq!(paper.history_len(), paper.action_count());
}

#[test]
fn scalar_newtypes_enforce_the_recorded_bounds() {
    assert_eq!(
        Width::new(MIN_BOARD_WIDTH - 1),
        Err(PaperError::WidthOutOfRange { value: 3 })
    );
    assert_eq!(Width::new(MIN_BOARD_WIDTH).map(Width::get), Ok(4));
    assert_eq!(Width::new(MAX_BOARD_WIDTH).map(Width::get), Ok(12));
    assert_eq!(
        Width::new(MAX_BOARD_WIDTH + 1),
        Err(PaperError::WidthOutOfRange { value: 13 })
    );

    assert_eq!(
        Height::new(MIN_BOARD_HEIGHT - 1),
        Err(PaperError::HeightOutOfRange { value: 3 })
    );
    assert_eq!(Height::new(MIN_BOARD_HEIGHT).map(Height::get), Ok(4));
    assert_eq!(Height::new(MAX_BOARD_HEIGHT).map(Height::get), Ok(12));
    assert_eq!(
        Height::new(MAX_BOARD_HEIGHT + 1),
        Err(PaperError::HeightOutOfRange { value: 13 })
    );

    assert_eq!(FoldCount::new(MAX_FOLD_ACTIONS).map(FoldCount::get), Ok(12));
    assert_eq!(
        FoldCount::new(MAX_FOLD_ACTIONS + 1),
        Err(PaperError::FoldBudgetOutOfRange { value: 13 })
    );
    assert_eq!(
        StrokeCount::new(MAX_STROKE_ACTIONS).map(StrokeCount::get),
        Ok(8)
    );
    assert_eq!(
        StrokeCount::new(MAX_STROKE_ACTIONS + 1),
        Err(PaperError::StrokeBudgetOutOfRange { value: 9 })
    );
    assert_eq!(ActionCount::new(MAX_ACTIONS).map(ActionCount::get), Ok(64));
    assert_eq!(
        ActionCount::new(MAX_ACTIONS + 1),
        Err(PaperError::ActionBudgetOutOfRange { value: 65 })
    );

    assert_eq!(CellId::new(143).map(CellId::get), Ok(143));
    assert_eq!(
        CellId::new(144),
        Err(PaperError::CellIdOutOfRange { value: 144 })
    );
    assert_eq!(Row::new(11).map(Row::get), Ok(11));
    assert_eq!(Row::new(12), Err(PaperError::RowOutOfRange { value: 12 }));
    assert_eq!(Column::new(11).map(Column::get), Ok(11));
    assert_eq!(
        Column::new(12),
        Err(PaperError::ColumnOutOfRange { value: 12 })
    );
}

#[test]
fn paper_construction_reports_malformed_dimensions_and_budgets() {
    let cases = [
        (
            PaperSpec::new(3, 4, 1, 1, 2),
            PaperError::WidthOutOfRange { value: 3 },
        ),
        (
            PaperSpec::new(4, 13, 1, 1, 2),
            PaperError::HeightOutOfRange { value: 13 },
        ),
        (
            PaperSpec::new(4, 4, 13, 1, 2),
            PaperError::FoldBudgetOutOfRange { value: 13 },
        ),
        (
            PaperSpec::new(4, 4, 1, 9, 2),
            PaperError::StrokeBudgetOutOfRange { value: 9 },
        ),
        (
            PaperSpec::new(4, 4, 1, 1, 65),
            PaperError::ActionBudgetOutOfRange { value: 65 },
        ),
    ];

    for (spec, expected) in cases {
        assert_eq!(Paper::new(spec), Err(expected));
    }
}

#[test]
fn rectangular_construction_assigns_stable_row_major_identities() {
    let mut paper = paper(4, 6);
    let initial = paper.clone();
    let dimensions = paper.dimensions();

    assert_eq!(dimensions.cell_count(), 24);
    assert_eq!(
        paper.cell_ids().map(CellId::get).collect::<Vec<_>>(),
        (0..24).collect::<Vec<_>>()
    );
    assert_eq!(
        dimensions
            .cell_id(coordinate(&paper, 2, 3))
            .map(CellId::get),
        Ok(11)
    );
    assert_eq!(
        dimensions.original_coordinate(CellId::new(11).expect("11 is globally valid")),
        Ok(coordinate(&paper, 2, 3))
    );

    let cell = paper
        .physical_cell(CellId::new(11).expect("11 is globally valid"))
        .expect("cell 11 should belong to the paper");
    assert_eq!(cell.coordinate(), coordinate(&paper, 2, 3));
    assert_eq!(cell.layer().get(), 0);
    assert_eq!(cell.face(), Face::Front);
    assert_eq!(cell.orientation(), Orientation::North);

    let last_cell = CellId::new(23).expect("23 is globally valid");
    assert_eq!(
        dimensions.original_coordinate(last_cell),
        Ok(coordinate(&paper, 5, 3))
    );
    paper
        .fold(Fold::new(FoldDirection::Left, 2))
        .expect("the width should use crease 2");
    paper
        .fold(Fold::new(FoldDirection::Up, 3))
        .expect("the height should use crease 3");
    assert_observable_invariants(&paper);
    paper.undo().expect("the horizontal fold should undo");
    paper.undo().expect("the vertical fold should undo");
    assert_eq!(paper, initial);
}

#[test]
fn vertical_half_folds_reflect_cells_and_place_moved_layers_on_top() {
    let mut left = paper(4, 4);
    left.fold(Fold::new(FoldDirection::Left, 2))
        .expect("the right half should fold left");
    assert_eq!(stack_ids(&left, 1, 1), [5, 6]);
    assert_eq!(stack_ids(&left, 1, 0), [4, 7]);
    assert!(stack_ids(&left, 1, 2).is_empty());
    let moved = left
        .physical_cell(CellId::new(6).expect("6 is globally valid"))
        .expect("cell 6 should exist");
    assert_eq!(moved.face(), Face::Back);
    assert_eq!(moved.orientation(), Orientation::North);

    let mut right = paper(4, 4);
    right
        .fold(Fold::new(FoldDirection::Right, 2))
        .expect("the left half should fold right");
    assert_eq!(stack_ids(&right, 1, 2), [6, 5]);
    assert_eq!(stack_ids(&right, 1, 3), [7, 4]);
    assert!(stack_ids(&right, 1, 1).is_empty());
    assert_observable_invariants(&left);
    assert_observable_invariants(&right);
}

#[test]
fn horizontal_half_folds_reflect_orientation_and_layer_order() {
    let mut up = paper(4, 4);
    up.fold(Fold::new(FoldDirection::Up, 2))
        .expect("the bottom half should fold up");
    assert_eq!(stack_ids(&up, 1, 1), [5, 9]);
    assert_eq!(stack_ids(&up, 0, 1), [1, 13]);
    let moved = up
        .physical_cell(CellId::new(9).expect("9 is globally valid"))
        .expect("cell 9 should exist");
    assert_eq!(moved.face(), Face::Back);
    assert_eq!(moved.orientation(), Orientation::South);

    let mut down = paper(4, 4);
    down.fold(Fold::new(FoldDirection::Down, 2))
        .expect("the top half should fold down");
    assert_eq!(stack_ids(&down, 2, 1), [9, 5]);
    assert_eq!(stack_ids(&down, 3, 1), [13, 1]);
    assert!(stack_ids(&down, 1, 1).is_empty());
    assert_observable_invariants(&up);
    assert_observable_invariants(&down);
}

#[test]
fn a_second_fold_reverses_the_complete_moving_stack() {
    let mut paper = paper(4, 4);
    paper
        .fold(Fold::new(FoldDirection::Left, 2))
        .expect("the first fold should succeed");
    paper
        .fold(Fold::new(FoldDirection::Up, 2))
        .expect("the second fold should succeed");

    assert_eq!(stack_ids(&paper, 1, 1), [5, 6, 10, 9]);
    assert_eq!(stack_ids(&paper, 0, 0), [0, 3, 15, 12]);
    assert_observable_invariants(&paper);
}

#[test]
fn a_moved_stack_may_land_at_an_empty_bounded_position() {
    let mut paper = paper(4, 4);
    paper
        .fold(Fold::new(FoldDirection::Right, 2))
        .expect("the first fold should move all paper right");
    assert!(stack_ids(&paper, 1, 0).is_empty());

    paper
        .fold(Fold::new(FoldDirection::Left, 2))
        .expect("the folded stacks should be allowed to land in empty positions");

    assert_eq!(stack_ids(&paper, 1, 0), [4, 7]);
    assert_eq!(stack_ids(&paper, 1, 1), [5, 6]);
    assert_observable_invariants(&paper);
}

#[test]
fn a_dot_marks_every_cell_in_the_stack_and_comparison_is_exact() {
    let mut paper = paper(4, 4);
    paper
        .fold(Fold::new(FoldDirection::Left, 2))
        .expect("the fold should create a two-cell stack");
    paper
        .stamp_dot(coordinate(&paper, 1, 1))
        .expect("the stack should accept a dot");

    assert_eq!(
        paper.ink().cell_ids().map(CellId::get).collect::<Vec<_>>(),
        [5, 6]
    );
    let exact = InkPattern::from_cell_ids(
        paper.dimensions(),
        &[
            CellId::new(5).expect("5 is globally valid"),
            CellId::new(6).expect("6 is globally valid"),
        ],
    )
    .expect("the exact target should be valid");
    assert!(
        paper
            .compare_ink(exact)
            .expect("dimensions should match")
            .is_exact()
    );

    let different = InkPattern::from_cell_ids(
        paper.dimensions(),
        &[
            CellId::new(5).expect("5 is globally valid"),
            CellId::new(7).expect("7 is globally valid"),
        ],
    )
    .expect("the different target should be valid");
    let comparison = paper
        .compare_ink(different)
        .expect("dimensions should match");
    assert_eq!(
        comparison
            .missing()
            .cell_ids()
            .map(CellId::get)
            .collect::<Vec<_>>(),
        [7]
    );
    assert_eq!(
        comparison
            .extra()
            .cell_ids()
            .map(CellId::get)
            .collect::<Vec<_>>(),
        [6]
    );
}

#[test]
fn undo_restores_the_complete_prior_state_after_each_action() {
    let mut paper = paper(4, 4);
    let initial = paper.clone();
    paper
        .fold(Fold::new(FoldDirection::Left, 2))
        .expect("the fold should succeed");
    let folded = paper.clone();
    paper
        .stamp_dot(coordinate(&paper, 1, 1))
        .expect("the dot should succeed");

    paper.undo().expect("the dot should be undoable");
    assert_eq!(paper, folded);
    paper.undo().expect("the fold should be undoable");
    assert_eq!(paper, initial);

    let before_failed_undo = paper.clone();
    assert_eq!(paper.undo(), Err(PaperError::NothingToUndo));
    assert_eq!(paper, before_failed_undo);
}

#[test]
fn invalid_folds_and_exhausted_budgets_leave_state_unchanged() {
    let invalid_folds = [
        (
            Fold::new(FoldDirection::Left, 0),
            PaperError::CreaseOutsidePaper {
                axis: FoldAxis::Vertical,
                crease: 0,
                extent: 4,
            },
        ),
        (
            Fold::new(FoldDirection::Left, 1),
            PaperError::CreaseIsNotHalfFold {
                axis: FoldAxis::Vertical,
                crease: 1,
                extent: 4,
            },
        ),
        (
            Fold::new(FoldDirection::Up, 4),
            PaperError::CreaseOutsidePaper {
                axis: FoldAxis::Horizontal,
                crease: 4,
                extent: 4,
            },
        ),
    ];

    for (fold, expected) in invalid_folds {
        let mut paper = paper(4, 4);
        let before = paper.clone();
        assert_eq!(paper.fold(fold), Err(expected));
        assert_eq!(paper, before);
    }

    let mut empty_side = paper(4, 4);
    empty_side
        .fold(Fold::new(FoldDirection::Left, 2))
        .expect("the first fold should succeed");
    let before_empty = empty_side.clone();
    assert_eq!(
        empty_side.fold(Fold::new(FoldDirection::Left, 2)),
        Err(PaperError::EmptyMovingSide {
            direction: FoldDirection::Left,
        })
    );
    assert_eq!(empty_side, before_empty);

    let mut no_folds =
        Paper::new(PaperSpec::new(4, 4, 0, 1, 1)).expect("zero is a valid fold budget");
    let before_no_folds = no_folds.clone();
    assert_eq!(
        no_folds.fold(Fold::new(FoldDirection::Left, 2)),
        Err(PaperError::FoldBudgetExhausted {
            limit: FoldCount::new(0).expect("zero is valid"),
        })
    );
    assert_eq!(no_folds, before_no_folds);

    let mut one_action =
        Paper::new(PaperSpec::new(4, 4, 1, 1, 1)).expect("one action should be valid");
    one_action
        .fold(Fold::new(FoldDirection::Left, 2))
        .expect("the one allowed action should succeed");
    let before_exhausted = one_action.clone();
    assert_eq!(
        one_action.stamp_dot(coordinate(&one_action, 1, 1)),
        Err(PaperError::ActionBudgetExhausted {
            limit: ActionCount::new(1).expect("one is valid"),
        })
    );
    assert_eq!(one_action, before_exhausted);

    let mut odd_width = paper(5, 4);
    let before_odd_fold = odd_width.clone();
    assert_eq!(
        odd_width.fold(Fold::new(FoldDirection::Left, 2)),
        Err(PaperError::CreaseIsNotHalfFold {
            axis: FoldAxis::Vertical,
            crease: 2,
            extent: 5,
        })
    );
    assert_eq!(odd_width, before_odd_fold);
}

#[test]
fn empty_and_foreign_positions_are_rejected_without_inking() {
    let mut paper = paper(4, 4);
    paper
        .fold(Fold::new(FoldDirection::Left, 2))
        .expect("the right side should become empty");

    let before_empty = paper.clone();
    assert_eq!(
        paper.stamp_dot(coordinate(&paper, 1, 3)),
        Err(PaperError::EmptyBrushPosition {
            coordinate: coordinate(&paper, 1, 3),
        })
    );
    assert_eq!(paper, before_empty);

    let foreign = Coordinate::new(
        Row::new(4).expect("row 4 is globally valid"),
        Column::new(0).expect("column 0 is globally valid"),
    );
    let before_foreign = paper.clone();
    assert_eq!(
        paper.stamp_dot(foreign),
        Err(PaperError::CoordinateOutsidePaper {
            row: 4,
            column: 0,
            width: 4,
            height: 4,
        })
    );
    assert_eq!(paper, before_foreign);
}

#[test]
fn every_supported_even_size_preserves_identity_and_total_layer_order() {
    for extent in [4, 6, 8, 10, 12] {
        for direction in [
            FoldDirection::Left,
            FoldDirection::Right,
            FoldDirection::Up,
            FoldDirection::Down,
        ] {
            let mut paper = paper(extent, extent);
            paper
                .fold(Fold::new(direction, extent / 2))
                .expect("every centered fold on a fresh even paper should succeed");
            assert_observable_invariants(&paper);
            paper
                .undo()
                .expect("every successful fold should be undoable");
            assert_observable_invariants(&paper);
        }
    }
}

#[test]
fn target_construction_and_comparison_reject_cross_paper_state() {
    let sheet = paper(4, 4);
    let foreign_dimensions = paper(6, 4).dimensions();
    let foreign_target = InkPattern::empty(foreign_dimensions);
    assert_eq!(
        sheet.compare_ink(foreign_target),
        Err(PaperError::TargetDimensionsDiffer {
            paper: sheet.dimensions(),
            target: foreign_dimensions,
        })
    );

    let first_cell = CellId::new(0).expect("cell zero is globally valid");
    let oversized = vec![first_cell; MAX_PHYSICAL_CELLS + 1];
    assert_eq!(
        InkPattern::from_cell_ids(sheet.dimensions(), &oversized),
        Err(PaperError::TooManyTargetCells {
            count: MAX_PHYSICAL_CELLS + 1,
        })
    );

    let foreign_cell = CellId::new(16).expect("cell 16 is globally valid");
    assert_eq!(
        InkPattern::from_cell_ids(sheet.dimensions(), &[foreign_cell]),
        Err(PaperError::CellOutsidePaper {
            cell_id: foreign_cell,
            cell_count: 16,
        })
    );
}
