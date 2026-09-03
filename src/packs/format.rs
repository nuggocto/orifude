use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::paper::{
    BrushRule, Fold, FoldCount, FoldDirection, LineStroke, MAX_ACTIONS, PaperAction, StrokeAxis,
    StrokeCount,
};
use crate::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};
use crate::domain::replay::{Replay, ReplayMetadata};
use crate::domain::score::Par;

use super::{
    CURRENT_PACK_FORMAT_VERSION, MAX_METADATA_BYTES, MAX_PUZZLE_BYTES, MAX_PUZZLES,
    MAX_VALIDATION_ISSUES, PackError, PackIssue,
};

const MAX_TITLE_SCALARS: usize = 80;
const MAX_DESCRIPTION_SCALARS: usize = 512;
const MAX_AUTHORS: usize = 16;
const MAX_AUTHOR_SCALARS: usize = 80;
const MAX_TUTORIAL_CUES: usize = 16;
const MAX_TUTORIAL_SCALARS: usize = 512;
const MAX_LICENSE_BYTES: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackMetadata {
    format_version: u16,
    id: Box<str>,
    title: Box<str>,
    description: Option<Box<str>>,
    authors: Box<[Box<str>]>,
    license: Box<str>,
    puzzle_ids: Box<[String]>,
}

impl PackMetadata {
    #[must_use]
    pub const fn format_version(&self) -> u16 {
        self.format_version
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    #[must_use]
    pub fn authors(&self) -> &[Box<str>] {
        &self.authors
    }

    #[must_use]
    pub fn license(&self) -> &str {
        &self.license
    }

    #[must_use]
    pub fn puzzle_ids(&self) -> &[String] {
        &self.puzzle_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PuzzleContent {
    puzzle: Puzzle,
    title: Box<str>,
    description: Option<Box<str>>,
    tutorial_cues: Box<[Box<str>]>,
    author: Option<Box<str>>,
    license: Option<Box<str>>,
    solution: Option<Replay>,
}

impl PuzzleContent {
    #[must_use]
    pub const fn puzzle(&self) -> &Puzzle {
        &self.puzzle
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    #[must_use]
    pub fn tutorial_cues(&self) -> &[Box<str>] {
        &self.tutorial_cues
    }

    #[must_use]
    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    #[must_use]
    pub fn license(&self) -> Option<&str> {
        self.license.as_deref()
    }

    /// Returns the optional author-supplied solution after it has been
    /// replayed through the production engine.
    #[must_use]
    pub const fn solution(&self) -> Option<&Replay> {
        self.solution.as_ref()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackDocument {
    format_version: u16,
    id: String,
    title: String,
    description: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    license: String,
    puzzles: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PuzzleDocument {
    pub(crate) format_version: u16,
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) width: u8,
    pub(crate) height: u8,
    pub(crate) target: Vec<String>,
    pub(crate) folds: Vec<FoldDocument>,
    pub(crate) brushes: Vec<BrushDocument>,
    pub(crate) fold_budget: u8,
    pub(crate) stroke_budget: u8,
    pub(crate) par: Option<ParDocument>,
    #[serde(default)]
    pub(crate) tutorial_cues: Vec<String>,
    pub(crate) author: Option<String>,
    pub(crate) license: Option<String>,
    #[serde(default)]
    pub(crate) solution: Option<Vec<ActionDocument>>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FoldDocument {
    pub(crate) direction: DirectionDocument,
    pub(crate) crease: u8,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DirectionDocument {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum BrushDocument {
    Dot {},
    Line { axis: AxisDocument, length: u8 },
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AxisDocument {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParDocument {
    pub(crate) folds: u8,
    pub(crate) strokes: u8,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum ActionDocument {
    Fold {
        direction: DirectionDocument,
        crease: u8,
    },
    Dot {
        row: u8,
        column: u8,
    },
    Line {
        start_row: u8,
        start_column: u8,
        end_row: u8,
        end_column: u8,
    },
}

pub(super) fn parse_metadata(bytes: &[u8]) -> Result<PackMetadata, PackError> {
    if bytes.len() as u64 > MAX_METADATA_BYTES {
        return Err(PackError::one(
            "pack.toml",
            "metadata size exceeds the limit",
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| PackError::one("pack.toml", "metadata is not valid UTF-8"))?;
    let document: PackDocument = toml::from_str(text)
        .map_err(|_| PackError::one("pack.toml", "metadata TOML is invalid"))?;
    let mut issues = Vec::with_capacity(MAX_VALIDATION_ISSUES);
    if document.format_version != CURRENT_PACK_FORMAT_VERSION {
        record_issue(
            &mut issues,
            "pack.toml",
            "pack format version is unsupported",
        );
    }
    if PuzzleIdentity::new(&document.id, "probe").is_err() {
        record_issue(&mut issues, "pack.id", "pack ID is invalid");
    }
    validate_display(
        &document.title,
        MAX_TITLE_SCALARS,
        "pack.title",
        &mut issues,
    );
    if let Some(description) = &document.description {
        validate_display(
            description,
            MAX_DESCRIPTION_SCALARS,
            "pack.description",
            &mut issues,
        );
    }
    if document.authors.len() > MAX_AUTHORS {
        record_issue(
            &mut issues,
            "pack.authors",
            "author count exceeds the limit",
        );
    }
    for author in document.authors.iter().take(MAX_VALIDATION_ISSUES) {
        if issues.len() == MAX_VALIDATION_ISSUES {
            break;
        }
        validate_display(author, MAX_AUTHOR_SCALARS, "pack.authors", &mut issues);
    }
    validate_license(&document.license, "pack.license", &mut issues);
    if document.puzzles.is_empty() || document.puzzles.len() > MAX_PUZZLES {
        record_issue(
            &mut issues,
            "pack.puzzles",
            "puzzle count is outside the limit",
        );
    }
    let mut puzzle_ids = BTreeSet::new();
    for puzzle_id in document.puzzles.iter().take(MAX_PUZZLES) {
        if issues.len() == MAX_VALIDATION_ISSUES {
            break;
        }
        if PuzzleIdentity::new(&document.id, puzzle_id).is_err() {
            record_issue(&mut issues, "pack.puzzles", "puzzle ID is invalid");
        }
        if !puzzle_ids.insert(puzzle_id.to_ascii_lowercase()) {
            record_issue(&mut issues, "pack.puzzles", "puzzle ID is duplicated");
        }
    }
    if !issues.is_empty() {
        return Err(PackError::Invalid {
            issues: issues.into_boxed_slice(),
        });
    }
    Ok(PackMetadata {
        format_version: document.format_version,
        id: document.id.into_boxed_str(),
        title: document.title.into_boxed_str(),
        description: document.description.map(String::into_boxed_str),
        authors: document
            .authors
            .into_iter()
            .map(String::into_boxed_str)
            .collect(),
        license: document.license.into_boxed_str(),
        puzzle_ids: document.puzzles.into_boxed_slice(),
    })
}

pub(super) fn parse_puzzle(
    pack_id: &str,
    expected_id: &str,
    bytes: &[u8],
) -> Result<PuzzleContent, PackError> {
    if bytes.len() as u64 > MAX_PUZZLE_BYTES {
        return Err(PackError::one("puzzle", "puzzle size exceeds the limit"));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| PackError::one("puzzle", "puzzle is not valid UTF-8"))?;
    let document: PuzzleDocument =
        toml::from_str(text).map_err(|_| PackError::one("puzzle", "puzzle TOML is invalid"))?;
    puzzle_from_document(pack_id, expected_id, document)
}

pub(crate) fn puzzle_from_document(
    pack_id: &str,
    expected_id: &str,
    document: PuzzleDocument,
) -> Result<PuzzleContent, PackError> {
    let mut issues = Vec::with_capacity(MAX_VALIDATION_ISSUES);
    collect_puzzle_display_issues(expected_id, &document, &mut issues);
    let (identity, target, par) = validate_puzzle_parts(pack_id, &document, &mut issues);
    let puzzle = build_puzzle(&document, identity, target, par, &mut issues);
    let solution = puzzle
        .as_ref()
        .and_then(|puzzle| validate_solution(puzzle, document.solution.as_deref(), &mut issues));

    if !issues.is_empty() {
        return Err(PackError::Invalid {
            issues: issues.into_boxed_slice(),
        });
    }
    let puzzle =
        puzzle.ok_or_else(|| PackError::one("puzzle", "puzzle validation is incomplete"))?;
    Ok(PuzzleContent {
        puzzle,
        title: document.title.into_boxed_str(),
        description: document.description.map(String::into_boxed_str),
        tutorial_cues: document
            .tutorial_cues
            .into_iter()
            .map(String::into_boxed_str)
            .collect(),
        author: document.author.map(String::into_boxed_str),
        license: document.license.map(String::into_boxed_str),
        solution,
    })
}

fn validate_solution(
    puzzle: &Puzzle,
    document: Option<&[ActionDocument]>,
    issues: &mut Vec<PackIssue>,
) -> Option<Replay> {
    let actions = document?;
    if actions.len() > usize::from(MAX_ACTIONS) {
        record_issue(
            issues,
            "puzzle.solution",
            "solution action count is outside the limit",
        );
        return None;
    }

    let dimensions = puzzle.dimensions();
    let mut converted = Vec::with_capacity(actions.len());
    for action in actions {
        let converted_action = match *action {
            ActionDocument::Fold { direction, crease } => {
                Some(PaperAction::Fold(Fold::new(direction.into(), crease)))
            }
            ActionDocument::Dot { row, column } => dimensions
                .coordinate(row, column)
                .ok()
                .map(PaperAction::Dot),
            ActionDocument::Line {
                start_row,
                start_column,
                end_row,
                end_column,
            } => dimensions
                .coordinate(start_row, start_column)
                .ok()
                .zip(dimensions.coordinate(end_row, end_column).ok())
                .map(|(start, end)| PaperAction::Line(LineStroke::new(start, end))),
        };
        let Some(converted_action) = converted_action else {
            record_issue(
                issues,
                "puzzle.solution",
                "solution coordinate is outside the paper",
            );
            return None;
        };
        converted.push(converted_action);
    }

    let replay = Replay::new(ReplayMetadata::current(puzzle), converted)
        .expect("the checked solution count must fit the replay bound");
    match replay.execute(puzzle) {
        Ok(attempt) if attempt.result().is_success() => Some(replay),
        Ok(_) => {
            record_issue(
                issues,
                "puzzle.solution",
                "solution does not match the target exactly",
            );
            None
        }
        Err(_) => {
            record_issue(
                issues,
                "puzzle.solution",
                "solution contains an illegal action",
            );
            None
        }
    }
}

fn collect_puzzle_display_issues(
    expected_id: &str,
    document: &PuzzleDocument,
    issues: &mut Vec<PackIssue>,
) {
    if document.id != expected_id {
        record_issue(
            issues,
            "puzzle.id",
            "puzzle ID does not match its declared path",
        );
    }
    validate_display(&document.title, MAX_TITLE_SCALARS, "puzzle.title", issues);
    if let Some(description) = &document.description {
        validate_display(
            description,
            MAX_DESCRIPTION_SCALARS,
            "puzzle.description",
            issues,
        );
    }
    if document.tutorial_cues.len() > MAX_TUTORIAL_CUES {
        record_issue(
            issues,
            "puzzle.tutorial_cues",
            "tutorial cue count exceeds the limit",
        );
    }
    for cue in document.tutorial_cues.iter().take(MAX_VALIDATION_ISSUES) {
        if issues.len() == MAX_VALIDATION_ISSUES {
            break;
        }
        validate_display(cue, MAX_TUTORIAL_SCALARS, "puzzle.tutorial_cues", issues);
    }
    if let Some(author) = &document.author {
        validate_display(author, MAX_AUTHOR_SCALARS, "puzzle.author", issues);
    }
    if let Some(license) = &document.license {
        validate_license(license, "puzzle.license", issues);
    }
}

type ValidatedPuzzleParts = (
    Option<PuzzleIdentity>,
    Option<Vec<crate::domain::paper::CellId>>,
    ValidatedPar,
);

#[derive(Clone, Copy)]
enum ValidatedPar {
    Absent,
    Present(Par),
    Invalid,
}

fn validate_puzzle_parts(
    pack_id: &str,
    document: &PuzzleDocument,
    issues: &mut Vec<PackIssue>,
) -> ValidatedPuzzleParts {
    let identity = if let Ok(identity) = PuzzleIdentity::new(pack_id, &document.id) {
        Some(identity)
    } else {
        record_issue(issues, "puzzle.id", "puzzle identity is invalid");
        None
    };
    let dimensions = if let Ok(dimensions) =
        crate::domain::paper::Dimensions::new(document.width, document.height)
    {
        Some(dimensions)
    } else {
        record_issue(issues, "puzzle.dimensions", "puzzle dimensions are invalid");
        None
    };
    let target =
        dimensions.and_then(
            |dimensions| match target_cells(dimensions, &document.target) {
                Ok(target) => Some(target),
                Err(PackError::Invalid {
                    issues: target_issues,
                }) => {
                    for issue in target_issues {
                        if issues.len() == MAX_VALIDATION_ISSUES {
                            break;
                        }
                        issues.push(issue);
                    }
                    None
                }
                Err(_) => {
                    record_issue(issues, "puzzle.target", "target grid is invalid");
                    None
                }
            },
        );
    let par = match document.par {
        None => ValidatedPar::Absent,
        Some(par) => {
            let folds = FoldCount::new(par.folds);
            let strokes = StrokeCount::new(par.strokes);
            if let (Ok(folds), Ok(strokes)) = (folds, strokes) {
                ValidatedPar::Present(Par::new(folds, strokes))
            } else {
                record_issue(issues, "puzzle.par", "par score is invalid");
                ValidatedPar::Invalid
            }
        }
    };
    (identity, target, par)
}

fn build_puzzle(
    document: &PuzzleDocument,
    identity: Option<PuzzleIdentity>,
    target: Option<Vec<crate::domain::paper::CellId>>,
    par: ValidatedPar,
    issues: &mut Vec<PackIssue>,
) -> Option<Puzzle> {
    if matches!(par, ValidatedPar::Invalid) {
        return None;
    }
    let identity = identity?;
    let target = target?;
    let folds = document
        .folds
        .iter()
        .map(|fold| Fold::new(fold.direction.into(), fold.crease))
        .collect();
    let brushes = document
        .brushes
        .iter()
        .copied()
        .map(BrushRule::from)
        .collect();
    let mut spec = PuzzleSpec::new(identity, document.width, document.height)
        .with_format_version(document.format_version)
        .with_target_cells(target)
        .with_allowed_folds(folds)
        .with_allowed_brushes(brushes)
        .with_budgets(document.fold_budget, document.stroke_budget);
    if let ValidatedPar::Present(par) = par {
        spec = spec.with_par(par);
    }
    if let Ok(puzzle) = Puzzle::new(spec) {
        Some(puzzle)
    } else {
        record_issue(issues, "puzzle.rules", "puzzle gameplay rules are invalid");
        None
    }
}

fn target_cells(
    dimensions: crate::domain::paper::Dimensions,
    rows: &[String],
) -> Result<Vec<crate::domain::paper::CellId>, PackError> {
    if rows.len() != usize::from(dimensions.height().get()) {
        return Err(PackError::one(
            "puzzle.target",
            "target grid height does not match the paper",
        ));
    }
    let mut target = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        if row.len() != usize::from(dimensions.width().get())
            || !row.bytes().all(|cell| matches!(cell, b'.' | b'#'))
        {
            return Err(PackError::one(
                "puzzle.target",
                "target grid must use one ASCII dot or hash per cell",
            ));
        }
        let row_index = u8::try_from(row_index)
            .map_err(|_| PackError::one("puzzle.target", "target row is outside the paper"))?;
        for (column_index, cell) in row.bytes().enumerate() {
            if cell == b'#' {
                let column_index = u8::try_from(column_index).map_err(|_| {
                    PackError::one("puzzle.target", "target column is outside the paper")
                })?;
                let coordinate = dimensions
                    .coordinate(row_index, column_index)
                    .map_err(|_| {
                        PackError::one("puzzle.target", "target cell is outside the paper")
                    })?;
                target.push(
                    dimensions
                        .cell_id(coordinate)
                        .map_err(|_| PackError::one("puzzle.target", "target cell is invalid"))?,
                );
            }
        }
    }
    Ok(target)
}

fn validate_display(
    value: &str,
    max_scalars: usize,
    location: &'static str,
    issues: &mut Vec<PackIssue>,
) {
    if issues.len() == MAX_VALIDATION_ISSUES {
        return;
    }
    if value.is_empty() || value.chars().count() > max_scalars {
        record_issue(issues, location, "display text length is outside the limit");
    }
    if value.chars().any(char::is_control) {
        record_issue(
            issues,
            location,
            "display text contains a control character",
        );
    }
}

fn validate_license(value: &str, location: &'static str, issues: &mut Vec<PackIssue>) {
    if value.is_empty()
        || value.len() > MAX_LICENSE_BYTES
        || !value.is_ascii()
        || spdx::Expression::parse(value).is_err()
    {
        record_issue(
            issues,
            location,
            "license is not a valid bounded SPDX expression",
        );
    }
}

fn record_issue(
    issues: &mut Vec<PackIssue>,
    location: impl Into<Box<str>>,
    problem: impl Into<Box<str>>,
) {
    if issues.len() < MAX_VALIDATION_ISSUES {
        issues.push(PackIssue::new(location, problem));
    }
}

impl From<DirectionDocument> for FoldDirection {
    fn from(direction: DirectionDocument) -> Self {
        match direction {
            DirectionDocument::Left => Self::Left,
            DirectionDocument::Right => Self::Right,
            DirectionDocument::Up => Self::Up,
            DirectionDocument::Down => Self::Down,
        }
    }
}

impl From<BrushDocument> for BrushRule {
    fn from(brush: BrushDocument) -> Self {
        match brush {
            BrushDocument::Dot {} => Self::Dot,
            BrushDocument::Line { axis, length } => Self::Line {
                axis: match axis {
                    AxisDocument::Horizontal => StrokeAxis::Horizontal,
                    AxisDocument::Vertical => StrokeAxis::Vertical,
                },
                length,
            },
        }
    }
}

pub(super) fn content_fingerprint(
    format_version: u16,
    files: &BTreeMap<String, Vec<u8>>,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"orifude-pack\0");
    digest.update(format_version.to_be_bytes());
    for (path, bytes) in files {
        digest.update((path.len() as u64).to_be_bytes());
        digest.update(path.as_bytes());
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    digest.finalize().into()
}
