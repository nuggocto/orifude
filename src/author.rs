use std::error::Error;
use std::fmt::{self, Write as _};
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cli::{CommandOutcome, ExitStatus};
use crate::domain::paper::{FoldDirection, PaperAction};
use crate::packs::{PackError, ValidatedPack, validate_source};
use crate::solver::{NeverCancel, SolveOutcome, Solver, SolverLimits};
use crate::storage::{AppPaths, InstallOutcome, PathError, Storage, StorageError};

const MAX_AUTHOR_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub enum AuthorError {
    Pack(PackError),
    Paths(PathError),
    Storage(StorageError),
    Output(io::Error),
    Clock,
    Solver {
        puzzle_id: Box<str>,
        reason: &'static str,
    },
    Command,
    OutputLimit,
}

impl fmt::Display for AuthorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pack(_) => formatter.write_str("could not read the local puzzle pack"),
            Self::Paths(_) => formatter.write_str("could not locate local Orifude data"),
            Self::Storage(_) => formatter.write_str("could not update local puzzle packs"),
            Self::Output(_) => formatter.write_str("could not write command output"),
            Self::Clock => formatter.write_str("could not read a valid installation time"),
            Self::Solver { puzzle_id, reason } => {
                write!(formatter, "solver {reason} for puzzle {puzzle_id}")
            }
            Self::Command => formatter.write_str("command is not an author operation"),
            Self::OutputLimit => formatter.write_str("author command output exceeded its limit"),
        }
    }
}

impl Error for AuthorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Pack(error) => Some(error),
            Self::Paths(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::Output(error) => Some(error),
            Self::Clock | Self::Solver { .. } | Self::Command | Self::OutputLimit => None,
        }
    }
}

/// Runs one parsed local author command against caller-provided streams.
///
/// # Errors
///
/// Returns a typed pack, storage, solver, clock, command, output, or resource
/// error. Validation failures with structured issues are written to `stderr`
/// and returned as a normal failure status.
pub fn execute_author(
    command: CommandOutcome,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<ExitStatus, AuthorError> {
    match command {
        CommandOutcome::Verify(path) => match validate_source(&path) {
            Ok(pack) => {
                let message = format!(
                    "Verified {} puzzle(s) in pack {} [{}].\n",
                    pack.puzzles().len(),
                    pack.metadata().id(),
                    pack.fingerprint_hex()
                );
                write_bounded(stdout, &message)?;
                Ok(ExitStatus::Success)
            }
            Err(error) => report_pack_error(stderr, error),
        },
        CommandOutcome::Solve(path) => match validate_source(&path) {
            Ok(pack) => solve_pack(&pack, stdout),
            Err(error) => report_pack_error(stderr, error),
        },
        CommandOutcome::PackInstall(path) => match validate_source(&path) {
            Ok(pack) => install_pack(&pack, stdout),
            Err(error) => report_pack_error(stderr, error),
        },
        CommandOutcome::PackList => list_packs(stdout),
        CommandOutcome::PackRemove(pack_id) => remove_pack(&pack_id, stdout),
        CommandOutcome::Play | CommandOutcome::Exit(_) => Err(AuthorError::Command),
    }
}

fn solve_pack(pack: &ValidatedPack, stdout: &mut impl Write) -> Result<ExitStatus, AuthorError> {
    let mut output = String::new();
    writeln!(output, "pack = \"{}\"", pack.metadata().id()).expect("string writes cannot fail");
    for content in pack.puzzles() {
        let solution = match Solver::solve(content.puzzle(), SolverLimits::default(), &NeverCancel)
        {
            SolveOutcome::Solved(solution) => solution,
            SolveOutcome::Unsolved(_) => {
                return Err(AuthorError::Solver {
                    puzzle_id: content.puzzle().identity().puzzle_id().into(),
                    reason: "found no solution",
                });
            }
            SolveOutcome::Exhausted { .. } => {
                return Err(AuthorError::Solver {
                    puzzle_id: content.puzzle().identity().puzzle_id().into(),
                    reason: "reached a resource limit",
                });
            }
            SolveOutcome::Cancelled(_) => {
                return Err(AuthorError::Solver {
                    puzzle_id: content.puzzle().identity().puzzle_id().into(),
                    reason: "was cancelled",
                });
            }
            SolveOutcome::Invalid(_) => {
                return Err(AuthorError::Solver {
                    puzzle_id: content.puzzle().identity().puzzle_id().into(),
                    reason: "received invalid limits",
                });
            }
        };
        writeln!(
            output,
            "\n[puzzle.{}]\nfolds = {}\nstrokes = {}\nsolution = [",
            content.puzzle().identity().puzzle_id(),
            solution.score().folds().get(),
            solution.score().strokes().get()
        )
        .expect("string writes cannot fail");
        for action in solution.replay().actions() {
            writeln!(output, "  {},", action_toml(*action)).expect("string writes cannot fail");
        }
        output.push_str("]\n");
        if output.len() > MAX_AUTHOR_OUTPUT_BYTES {
            return Err(AuthorError::OutputLimit);
        }
    }
    write_bounded(stdout, &output)?;
    Ok(ExitStatus::Success)
}

fn action_toml(action: PaperAction) -> String {
    match action {
        PaperAction::Fold(fold) => format!(
            "{{ kind = \"fold\", direction = \"{}\", crease = {} }}",
            direction_name(fold.direction()),
            fold.crease()
        ),
        PaperAction::Dot(coordinate) => format!(
            "{{ kind = \"dot\", row = {}, column = {} }}",
            coordinate.row().get(),
            coordinate.column().get()
        ),
        PaperAction::Line(line) => format!(
            "{{ kind = \"line\", start_row = {}, start_column = {}, end_row = {}, end_column = {} }}",
            line.start().row().get(),
            line.start().column().get(),
            line.end().row().get(),
            line.end().column().get()
        ),
    }
}

const fn direction_name(direction: FoldDirection) -> &'static str {
    match direction {
        FoldDirection::Left => "left",
        FoldDirection::Right => "right",
        FoldDirection::Up => "up",
        FoldDirection::Down => "down",
    }
}

fn install_pack(pack: &ValidatedPack, stdout: &mut impl Write) -> Result<ExitStatus, AuthorError> {
    let paths = AppPaths::runtime().map_err(AuthorError::Paths)?;
    let mut storage = Storage::open(paths).map_err(AuthorError::Storage)?;
    let installed_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .ok_or(AuthorError::Clock)?;
    let outcome = storage
        .install_validated(pack, installed_at)
        .map_err(AuthorError::Storage)?;
    let (verb, summary) = match outcome {
        InstallOutcome::Installed(summary) => ("Installed", summary),
        InstallOutcome::AlreadyPresent(summary) => ("Already installed", summary),
    };
    write_bounded(stdout, &format!("{verb} pack {}.\n", summary.id))?;
    Ok(ExitStatus::Success)
}

fn list_packs(stdout: &mut impl Write) -> Result<ExitStatus, AuthorError> {
    let paths = AppPaths::runtime().map_err(AuthorError::Paths)?;
    let storage = Storage::open(paths).map_err(AuthorError::Storage)?;
    let packs = storage.registered_packs().map_err(AuthorError::Storage)?;
    let mut output = String::new();
    if packs.is_empty() {
        output.push_str("No puzzle packs are installed.\n");
    } else {
        for pack in packs {
            writeln!(output, "{}\t{}", pack.id, pack.title).expect("string writes cannot fail");
        }
    }
    write_bounded(stdout, &output)?;
    Ok(ExitStatus::Success)
}

fn remove_pack(pack_id: &str, stdout: &mut impl Write) -> Result<ExitStatus, AuthorError> {
    let paths = AppPaths::runtime().map_err(AuthorError::Paths)?;
    let mut storage = Storage::open(paths).map_err(AuthorError::Storage)?;
    let removed = storage.remove_pack(pack_id).map_err(AuthorError::Storage)?;
    let message = if removed {
        format!("Removed pack {pack_id}. Saved progress was kept.\n")
    } else {
        format!("Pack {pack_id} is not installed.\n")
    };
    write_bounded(stdout, &message)?;
    Ok(ExitStatus::Success)
}

fn report_pack_error(stderr: &mut impl Write, error: PackError) -> Result<ExitStatus, AuthorError> {
    if error.issues().is_empty() {
        return Err(AuthorError::Pack(error));
    }
    let mut output = format!("error: {error}\n");
    for issue in error.issues() {
        writeln!(output, "{}: {}", issue.location(), issue.problem())
            .expect("string writes cannot fail");
    }
    write_bounded(stderr, &output)?;
    Ok(ExitStatus::Failure)
}

fn write_bounded(stream: &mut impl Write, content: &str) -> Result<(), AuthorError> {
    if content.len() > MAX_AUTHOR_OUTPUT_BYTES {
        return Err(AuthorError::OutputLimit);
    }
    stream
        .write_all(content.as_bytes())
        .and_then(|()| stream.flush())
        .map_err(AuthorError::Output)
}
