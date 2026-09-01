use std::fmt::Write as FmtWrite;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use orifude::domain::paper::{
    CellId, Coordinate, Fold, FoldDirection, InkPattern, MAX_PHYSICAL_CELLS, Paper, PaperError,
    PaperSpec, StackView,
};

const MAX_MENU_ATTEMPTS_PER_RUN: usize = 12;
const MAX_INPUT_BYTES: usize = 16;
const MAX_STACK_TEXT_BYTES: usize = MAX_PHYSICAL_CELLS * 4 + 1;
const PAPER_SPEC: PaperSpec = PaperSpec::new(4, 4, 4, 2, 6);

const LEFT: Fold = Fold::new(FoldDirection::Left, 2);
const RIGHT: Fold = Fold::new(FoldDirection::Right, 2);
const UP: Fold = Fold::new(FoldDirection::Up, 2);
const DOWN: Fold = Fold::new(FoldDirection::Down, 2);

const EXERCISES: [Exercise; 6] = [
    Exercise {
        name: "right half folds left",
        folds: &[LEFT],
        focus_row: 1,
        focus_column: 1,
        expected_stack: &[5, 6],
    },
    Exercise {
        name: "left half folds right",
        folds: &[RIGHT],
        focus_row: 1,
        focus_column: 2,
        expected_stack: &[6, 5],
    },
    Exercise {
        name: "bottom half folds up",
        folds: &[UP],
        focus_row: 1,
        focus_column: 1,
        expected_stack: &[5, 9],
    },
    Exercise {
        name: "top half folds down",
        folds: &[DOWN],
        focus_row: 2,
        focus_column: 1,
        expected_stack: &[9, 5],
    },
    Exercise {
        name: "right folds left, then bottom folds up",
        folds: &[LEFT, UP],
        focus_row: 1,
        focus_column: 1,
        expected_stack: &[5, 6, 10, 9],
    },
    Exercise {
        name: "left folds right, then top folds down",
        folds: &[RIGHT, DOWN],
        focus_row: 2,
        focus_column: 2,
        expected_stack: &[10, 9, 5, 6],
    },
];

#[derive(Clone, Copy)]
struct Exercise {
    name: &'static str,
    folds: &'static [Fold],
    focus_row: u8,
    focus_column: u8,
    expected_stack: &'static [u8],
}

struct BoundedInput {
    bytes: [u8; MAX_INPUT_BYTES],
    length: usize,
    ended: bool,
    too_long: bool,
}

impl BoundedInput {
    fn trimmed(&self) -> &[u8] {
        let mut start = 0;
        let mut end = self.length;
        while start < end && self.bytes[start].is_ascii_whitespace() {
            start += 1;
        }
        while end > start && self.bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        &self.bytes[start..end]
    }
}

fn main() -> ExitCode {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut errors = stderr.lock();

    match run(&mut input, &mut output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ignored = writeln!(errors, "error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(input: &mut impl Read, output: &mut impl Write) -> io::Result<()> {
    writeln!(
        output,
        "Orifude paper exercise\n\nEach bracket lists physical cell IDs from bottom to top."
    )?;

    for _ in 0..MAX_MENU_ATTEMPTS_PER_RUN {
        render_menu(output)?;
        let selection = read_bounded_input(input)?;
        if selection.ended && selection.length == 0 {
            return Ok(());
        }
        if selection.too_long {
            write_input_too_long(output)?;
            return Ok(());
        }

        match selection.trimmed() {
            b"q" | b"Q" => return Ok(()),
            [choice @ b'1'..=b'6'] => {
                let index = usize::from(*choice - b'1');
                if !run_exercise(EXERCISES[index], input, output)? {
                    return Ok(());
                }
            }
            _ => writeln!(output, "Please enter one number from 1 to 6, or q.")?,
        }
    }

    writeln!(
        output,
        "This run reached its limit of {MAX_MENU_ATTEMPTS_PER_RUN} menu attempts."
    )
}

fn render_menu(output: &mut impl Write) -> io::Result<()> {
    writeln!(output, "\nChoose a paper:")?;
    for (index, exercise) in EXERCISES.iter().enumerate() {
        writeln!(output, "  {}. {}", index + 1, exercise.name)?;
    }
    write!(output, "  q. quit\n> ")?;
    output.flush()
}

fn run_exercise(
    exercise: Exercise,
    input: &mut impl Read,
    output: &mut impl Write,
) -> io::Result<bool> {
    let mut paper = Paper::new(PAPER_SPEC).map_err(domain_error)?;
    let focus = paper
        .dimensions()
        .coordinate(exercise.focus_row, exercise.focus_column)
        .map_err(domain_error)?;
    let expected_ids = expected_cell_ids(exercise.expected_stack)?;
    let target =
        InkPattern::from_cell_ids(paper.dimensions(), &expected_ids).map_err(domain_error)?;

    writeln!(output, "\n{}", exercise.name)?;
    render_paper(&paper, output)?;
    writeln!(
        output,
        "Focus: row {}, column {}. Predict the top physical cell after {} fold(s).",
        exercise.focus_row,
        exercise.focus_column,
        exercise.folds.len()
    )?;
    write!(output, "Top cell ID, or q to stop: ")?;
    output.flush()?;

    let prediction = read_bounded_input(input)?;
    if prediction.ended && prediction.length == 0 {
        return Ok(false);
    }
    if prediction.too_long {
        write_input_too_long(output)?;
        return Ok(false);
    }
    if matches!(prediction.trimmed(), b"q" | b"Q") {
        return Ok(false);
    }
    let predicted_cell = parse_cell_id(&prediction);
    if predicted_cell.is_none() {
        writeln!(
            output,
            "The prediction must be a physical cell ID from 0 to 143."
        )?;
    }

    for &fold in exercise.folds {
        paper.fold(fold).map_err(domain_error)?;
        writeln!(output, "\nAfter folding {}:", fold.direction())?;
        render_paper(&paper, output)?;
    }

    let actual_stack = stack_at(&paper, focus)?;
    if actual_stack != expected_ids {
        return Err(io::Error::other(
            "a paper exercise no longer matches its reviewed stack",
        ));
    }
    let top_cell = actual_stack
        .last()
        .copied()
        .ok_or_else(|| io::Error::other("the exercise focus unexpectedly became empty"))?;
    match predicted_cell {
        Some(prediction) if prediction == top_cell => {
            writeln!(
                output,
                "Prediction: correct. The top cell is {}.",
                top_cell.get()
            )?;
        }
        Some(_) => {
            writeln!(
                output,
                "Prediction: not this time. The top cell is {}.",
                top_cell.get()
            )?;
        }
        None => writeln!(output, "The top cell is {}.", top_cell.get())?,
    }

    paper.stamp_dot(focus).map_err(domain_error)?;
    let comparison = paper.compare_ink(target).map_err(domain_error)?;
    if !comparison.is_exact() {
        return Err(io::Error::other(
            "the exercise dot did not produce its reviewed target",
        ));
    }
    writeln!(
        output,
        "A dot at the focus inks {} layer(s). Target comparison: exact.",
        actual_stack.len()
    )?;

    paper.undo().map_err(domain_error)?;
    if !paper.ink().is_empty() {
        return Err(io::Error::other("undo did not restore the uninked paper"));
    }
    writeln!(output, "Undo restored the complete uninked folded state.")?;
    Ok(true)
}

fn render_paper(paper: &Paper, output: &mut impl Write) -> io::Result<()> {
    let dimensions = paper.dimensions();
    let mut stack = StackView::new();
    for row in 0..dimensions.height().get() {
        for column in 0..dimensions.width().get() {
            let coordinate = dimensions.coordinate(row, column).map_err(domain_error)?;
            paper
                .stack_at(coordinate, &mut stack)
                .map_err(domain_error)?;
            let mut cell = String::with_capacity(MAX_STACK_TEXT_BYTES);
            cell.push('[');
            if stack.is_empty() {
                cell.push_str("..");
            } else {
                for (index, cell_id) in stack.cell_ids().iter().enumerate() {
                    if index > 0 {
                        cell.push('<');
                    }
                    write!(cell, "{:02}", cell_id.get())
                        .expect("writing to a bounded String must succeed");
                }
            }
            cell.push(']');
            write!(output, "{cell:<16}")?;
        }
        writeln!(output)?;
    }
    Ok(())
}

fn stack_at(paper: &Paper, coordinate: Coordinate) -> io::Result<Vec<CellId>> {
    let mut stack = StackView::new();
    paper
        .stack_at(coordinate, &mut stack)
        .map_err(domain_error)?;
    Ok(stack.cell_ids().to_vec())
}

fn expected_cell_ids(values: &[u8]) -> io::Result<Vec<CellId>> {
    if values.len() > MAX_PHYSICAL_CELLS {
        return Err(io::Error::other("an exercise stack exceeds the cell limit"));
    }
    values
        .iter()
        .map(|&value| CellId::new(value).map_err(domain_error))
        .collect()
}

fn parse_cell_id(input: &BoundedInput) -> Option<CellId> {
    if input.too_long || input.trimmed().is_empty() {
        return None;
    }

    let mut value = 0_u16;
    for &byte in input.trimmed() {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u16::from(byte - b'0'))?;
    }
    let value = u8::try_from(value).ok()?;
    CellId::new(value).ok()
}

fn read_bounded_input(input: &mut impl Read) -> io::Result<BoundedInput> {
    let mut result = BoundedInput {
        bytes: [0; MAX_INPUT_BYTES],
        length: 0,
        ended: false,
        too_long: false,
    };
    let mut byte = [0_u8; 1];

    for _ in 0..=MAX_INPUT_BYTES {
        let read = input.read(&mut byte)?;
        if read == 0 {
            result.ended = true;
            return Ok(result);
        }
        if byte[0] == b'\n' {
            return Ok(result);
        }
        if result.length == MAX_INPUT_BYTES {
            result.too_long = true;
            return Ok(result);
        }
        result.bytes[result.length] = byte[0];
        result.length += 1;
    }

    unreachable!("the bounded input loop must return after the limit plus one byte")
}

fn write_input_too_long(output: &mut impl Write) -> io::Result<()> {
    writeln!(
        output,
        "Input exceeded the {MAX_INPUT_BYTES}-byte limit. The exercise will stop."
    )
}

fn domain_error(error: PaperError) -> io::Error {
    io::Error::other(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_six_papers_produce_their_reviewed_bottom_to_top_stack() {
        assert_eq!(EXERCISES.len(), 6);

        for exercise in EXERCISES {
            let mut paper = Paper::new(PAPER_SPEC).expect("the paper should be valid");
            for &fold in exercise.folds {
                paper.fold(fold).expect("the exercise fold should be valid");
            }
            let focus = paper
                .dimensions()
                .coordinate(exercise.focus_row, exercise.focus_column)
                .expect("the focus should be valid");
            let actual = stack_at(&paper, focus).expect("the focus stack should be readable");
            let expected = expected_cell_ids(exercise.expected_stack)
                .expect("the reviewed stack should contain valid identities");
            assert_eq!(actual, expected, "{}", exercise.name);
        }
    }

    #[test]
    fn keyboard_session_exercises_prediction_ink_comparison_and_undo() {
        let mut input = &b"5\n9\nq\n"[..];
        let mut output = Vec::new();

        run(&mut input, &mut output).expect("the bounded session should succeed");

        let output = String::from_utf8(output).expect("the exercise output should be UTF-8");
        assert!(output.contains("Prediction: correct. The top cell is 9."));
        assert!(output.contains("A dot at the focus inks 4 layer(s)."));
        assert!(output.contains("Target comparison: exact."));
        assert!(output.contains("Undo restored the complete uninked folded state."));
    }

    #[test]
    fn oversized_menu_input_terminates_without_interpreting_its_tail() {
        let hostile = b"12345678901234565\x1b[31m\n5\n9\nq\n";
        let mut input = &hostile[..];
        let mut output = Vec::new();

        run(&mut input, &mut output).expect("oversized input should stop cleanly");

        let output = String::from_utf8(output).expect("the exercise output should be UTF-8");
        assert!(output.contains("Input exceeded the 16-byte limit. The exercise will stop."));
        assert_eq!(output.matches("Choose a paper:").count(), 1);
        assert!(!output.contains("Focus:"));
        assert!(!output.contains("12345678901234565"));
        assert!(!output.contains('\x1b'));
    }

    #[test]
    fn oversized_prediction_stops_before_the_paper_changes() {
        let mut input = &b"5\n12345678901234565\nq\n"[..];
        let mut output = Vec::new();

        run(&mut input, &mut output).expect("oversized input should stop cleanly");

        let output = String::from_utf8(output).expect("the exercise output should be UTF-8");
        assert!(output.contains("Input exceeded the 16-byte limit. The exercise will stop."));
        assert!(!output.contains("After folding"));
        assert!(!output.contains("Prediction:"));
    }
}
