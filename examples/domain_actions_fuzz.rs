use std::hint::black_box;
use std::io::{self, Read};
use std::process::ExitCode;

use orifude::domain::attempt::Attempt;
use orifude::domain::paper::{
    BrushRule, Fold, FoldDirection, LineStroke, MAX_ACTIONS, MAX_PHYSICAL_CELLS, PaperAction,
    StackView, StrokeAxis,
};
use orifude::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};
use orifude::domain::replay::Replay;

const MAX_FUZZ_INPUT_BYTES: usize = MAX_ACTIONS as usize * 4;

fn main() -> ExitCode {
    let mut input = Vec::with_capacity(MAX_FUZZ_INPUT_BYTES + 1);
    match io::stdin()
        .take(u64::try_from(MAX_FUZZ_INPUT_BYTES + 1).expect("the fuzz bound must fit in u64"))
        .read_to_end(&mut input)
    {
        Ok(_) if input.len() <= MAX_FUZZ_INPUT_BYTES => {
            exercise(black_box(&input));
            ExitCode::SUCCESS
        }
        Ok(_) => {
            eprintln!("domain action input exceeds {MAX_FUZZ_INPUT_BYTES} bytes");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("failed to read domain action input: {error}");
            ExitCode::FAILURE
        }
    }
}

fn exercise(input: &[u8]) {
    assert!(input.len() <= MAX_FUZZ_INPUT_BYTES);
    let puzzle = fuzz_puzzle();
    let mut attempt = puzzle.start();

    for bytes in input.chunks(4).take(usize::from(MAX_ACTIONS)) {
        let opcode = bytes[0] % 9;
        let first = bytes.get(1).copied().unwrap_or(0);
        let second = bytes.get(2).copied().unwrap_or(0);
        let third = bytes.get(3).copied().unwrap_or(0);

        if opcode == 7 {
            let before = attempt.clone();
            if attempt.undo().is_err() {
                assert_eq!(attempt, before);
            } else {
                assert_observable_invariants(&attempt);
            }
            continue;
        }
        if opcode == 8 {
            attempt.reset();
            assert_observable_invariants(&attempt);
            continue;
        }

        let action = decode_action(&puzzle, opcode, first, second, third);
        let before = attempt.clone();
        if attempt.apply(action).is_err() {
            assert_eq!(attempt, before);
        } else {
            assert_observable_invariants(&attempt);
        }
    }

    let replay = Replay::from_attempt(&attempt);
    let replayed = replay
        .execute(&puzzle)
        .expect("successful bounded actions must replay from a fresh paper");
    assert_eq!(replayed.state_key(), attempt.state_key());
}

fn fuzz_puzzle() -> Puzzle {
    let identity = PuzzleIdentity::new("internal", "domain-actions")
        .expect("the fixed fuzz identity must be valid");
    let mut folds = Vec::new();
    for crease in 1..4 {
        for direction in [
            FoldDirection::Left,
            FoldDirection::Right,
            FoldDirection::Up,
            FoldDirection::Down,
        ] {
            folds.push(Fold::new(direction, crease));
        }
    }
    let mut brushes = vec![BrushRule::Dot];
    for length in 2..=4 {
        brushes.push(BrushRule::Line {
            axis: StrokeAxis::Horizontal,
            length,
        });
        brushes.push(BrushRule::Line {
            axis: StrokeAxis::Vertical,
            length,
        });
    }
    Puzzle::new(
        PuzzleSpec::new(identity, 4, 4)
            .with_allowed_folds(folds)
            .with_allowed_brushes(brushes)
            .with_budgets(12, 8),
    )
    .expect("the fixed fuzz puzzle must validate")
}

fn decode_action(puzzle: &Puzzle, opcode: u8, first: u8, second: u8, third: u8) -> PaperAction {
    let dimensions = puzzle.dimensions();
    match opcode {
        0..=3 => {
            let direction = match opcode {
                0 => FoldDirection::Left,
                1 => FoldDirection::Right,
                2 => FoldDirection::Up,
                _ => FoldDirection::Down,
            };
            PaperAction::Fold(Fold::new(direction, first % 3 + 1))
        }
        4 => PaperAction::Dot(
            dimensions
                .coordinate(first % 4, second % 4)
                .expect("reduced coordinates must fit the fuzz paper"),
        ),
        5 => {
            let row = first % 4;
            let start = second % 4;
            let end = third % 4;
            PaperAction::Line(LineStroke::new(
                dimensions
                    .coordinate(row, start)
                    .expect("reduced coordinates must fit the fuzz paper"),
                dimensions
                    .coordinate(row, end)
                    .expect("reduced coordinates must fit the fuzz paper"),
            ))
        }
        _ => {
            let column = first % 4;
            let start = second % 4;
            let end = third % 4;
            PaperAction::Line(LineStroke::new(
                dimensions
                    .coordinate(start, column)
                    .expect("reduced coordinates must fit the fuzz paper"),
                dimensions
                    .coordinate(end, column)
                    .expect("reduced coordinates must fit the fuzz paper"),
            ))
        }
    }
}

fn assert_observable_invariants(attempt: &Attempt) {
    let dimensions = attempt.dimensions();
    let mut seen = [false; MAX_PHYSICAL_CELLS];
    let mut stack = StackView::new();
    for row in 0..dimensions.height().get() {
        for column in 0..dimensions.width().get() {
            let coordinate = dimensions
                .coordinate(row, column)
                .expect("enumerated coordinates must fit the paper");
            attempt
                .stack_at(coordinate, &mut stack)
                .expect("enumerated stacks must be readable");
            for (layer, &cell_id) in stack.cell_ids().iter().enumerate() {
                assert!(!seen[cell_id.index()]);
                seen[cell_id.index()] = true;
                let cell = attempt
                    .physical_cell(cell_id)
                    .expect("stack identities must resolve");
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_corpus_preserves_invariants_and_replay_equivalence() {
        let cases: [&[u8]; 6] = [
            &[],
            &[0],
            &[0; MAX_FUZZ_INPUT_BYTES],
            &[u8::MAX; MAX_FUZZ_INPUT_BYTES],
            b"foldinkundoreset",
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        ];
        for input in cases {
            exercise(input);
        }
    }
}
