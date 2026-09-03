//! Bounded community puzzle-pack parsing and validation.

mod archive;
mod format;

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

pub use format::{PackMetadata, PuzzleContent};

pub const CURRENT_PACK_FORMAT_VERSION: u16 = 1;
pub const MAX_PUZZLE_BYTES: u64 = 64 * 1024;
pub const MAX_METADATA_BYTES: u64 = 32 * 1024;
pub const MAX_NOTE_BYTES: u64 = 16 * 1024;
pub const MAX_PUZZLES: usize = 128;
pub const MAX_FILES: usize = 256;
pub const MAX_ARCHIVE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_EXTRACTED_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_MANAGED_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_INSTALLED_PACKS: usize = 32;
pub const MAX_PATH_DEPTH: usize = 4;
pub const MAX_COMPONENT_BYTES: usize = 80;
pub const MAX_RELATIVE_PATH_BYTES: usize = 128;
pub const MAX_VALIDATION_ISSUES: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackIssue {
    location: Box<str>,
    problem: Box<str>,
}

impl PackIssue {
    fn new(location: impl Into<Box<str>>, problem: impl Into<Box<str>>) -> Self {
        Self {
            location: safe_diagnostic_text(location.into()),
            problem: safe_diagnostic_text(problem.into()),
        }
    }

    #[must_use]
    pub fn location(&self) -> &str {
        &self.location
    }

    #[must_use]
    pub fn problem(&self) -> &str {
        &self.problem
    }
}

fn safe_diagnostic_text(text: Box<str>) -> Box<str> {
    if !text.chars().any(char::is_control) {
        return text;
    }
    text.chars()
        .map(|character| {
            if character.is_control() {
                '?'
            } else {
                character
            }
        })
        .collect::<String>()
        .into_boxed_str()
}

#[derive(Debug)]
pub enum PackError {
    Io(io::Error),
    Invalid { issues: Box<[PackIssue]> },
    Archive,
    SourceType,
}

impl PackError {
    fn one(location: impl Into<Box<str>>, problem: impl Into<Box<str>>) -> Self {
        Self::Invalid {
            issues: vec![PackIssue::new(location, problem)].into_boxed_slice(),
        }
    }

    #[must_use]
    pub fn issues(&self) -> &[PackIssue] {
        match self {
            Self::Invalid { issues } => issues,
            Self::Io(_) | Self::Archive | Self::SourceType => &[],
        }
    }
}

impl fmt::Display for PackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(_) => formatter.write_str("puzzle-pack I/O failed"),
            Self::Invalid { issues } => write!(
                formatter,
                "puzzle pack failed validation with {} issue(s)",
                issues.len()
            ),
            Self::Archive => formatter.write_str("puzzle-pack archive is invalid or unsupported"),
            Self::SourceType => {
                formatter.write_str("puzzle-pack source is not a directory or ZIP archive")
            }
        }
    }
}

impl Error for PackError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(source) => Some(source),
            Self::Invalid { .. } | Self::Archive | Self::SourceType => None,
        }
    }
}

impl From<io::Error> for PackError {
    fn from(source: io::Error) -> Self {
        Self::Io(source)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedPack {
    metadata: PackMetadata,
    puzzles: Box<[PuzzleContent]>,
    files: BTreeMap<String, Vec<u8>>,
    fingerprint: [u8; 32],
    extracted_bytes: u64,
}

impl ValidatedPack {
    #[must_use]
    pub const fn metadata(&self) -> &PackMetadata {
        &self.metadata
    }

    #[must_use]
    pub fn puzzles(&self) -> &[PuzzleContent] {
        &self.puzzles
    }

    #[must_use]
    pub const fn fingerprint(&self) -> [u8; 32] {
        self.fingerprint
    }

    #[must_use]
    pub fn fingerprint_hex(&self) -> String {
        fingerprint_hex(self.fingerprint)
    }

    #[must_use]
    pub const fn extracted_bytes(&self) -> u64 {
        self.extracted_bytes
    }

    pub(crate) fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
}

/// Resolves and validates a local directory or ZIP source by its actual type.
///
/// # Errors
///
/// Returns a bounded validation, archive, or filesystem error. The filename
/// extension does not select the parser.
pub fn validate_source(path: &Path) -> Result<ValidatedPack, PackError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(PackError::one("source", "symbolic links are not accepted"));
    }
    if metadata.is_dir() {
        return validate_directory(path);
    }
    if metadata.is_file() {
        let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        if size > MAX_ARCHIVE_BYTES {
            return Err(PackError::one(
                "archive",
                "compressed size exceeds the limit",
            ));
        }
        let bytes = read_file_bounded(path, MAX_ARCHIVE_BYTES as u64)?;
        return archive::validate_archive_bytes(&bytes);
    }
    Err(PackError::SourceType)
}

/// Validates a directory without following links or accepting special files.
///
/// # Errors
///
/// Returns when traversal, a declared resource bound, I/O, or content
/// validation fails.
pub fn validate_directory(root: &Path) -> Result<ValidatedPack, PackError> {
    let metadata = fs::symlink_metadata(root)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(PackError::SourceType);
    }

    let mut files = BTreeMap::new();
    let mut folded_paths = BTreeSet::new();
    let mut pending = vec![(root.to_path_buf(), String::new(), 0_usize)];
    while let Some((directory, prefix, depth)) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| PackError::one("path", "paths must be portable ASCII"))?;
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            validate_relative_path(&relative)?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(PackError::one(relative, "links are not accepted"));
            }
            if file_type.is_dir() {
                if !matches!(relative.as_str(), "puzzles" | "notes") {
                    return Err(PackError::one(
                        relative,
                        "directory is not declared by the schema",
                    ));
                }
                let child_depth = depth
                    .checked_add(1)
                    .ok_or_else(|| PackError::one("path", "directory depth exceeds the limit"))?;
                if child_depth >= MAX_PATH_DEPTH {
                    return Err(PackError::one(
                        relative,
                        "directory depth exceeds the limit",
                    ));
                }
                pending.push((entry.path(), relative, child_depth));
                continue;
            }
            if !file_type.is_file() {
                return Err(PackError::one(relative, "special files are not accepted"));
            }
            if files.len() >= MAX_FILES {
                return Err(PackError::one("pack", "file count exceeds the limit"));
            }
            let folded = relative.to_ascii_lowercase();
            if !folded_paths.insert(folded) {
                return Err(PackError::one(
                    relative,
                    "path duplicates another path by case",
                ));
            }
            let limit = content_limit(&relative)?;
            files.insert(relative, read_file_bounded(&entry.path(), limit)?);
        }
    }
    validate_files(files)
}

/// Parses and validates bounded ZIP bytes without touching the filesystem.
///
/// # Errors
///
/// Returns a bounded archive or content error.
pub fn validate_archive_bytes(bytes: &[u8]) -> Result<ValidatedPack, PackError> {
    archive::validate_archive_bytes(bytes)
}

/// Exercises the bounded pack-metadata parser without filesystem work.
///
/// # Errors
///
/// Returns a syntax or semantic validation error.
pub fn validate_metadata_bytes(bytes: &[u8]) -> Result<PackMetadata, PackError> {
    format::parse_metadata(bytes)
}

/// Exercises one bounded puzzle parser with fixed caller-supplied identities.
///
/// # Errors
///
/// Returns a syntax, identity, or domain validation error.
pub fn validate_puzzle_bytes(
    pack_id: &str,
    puzzle_id: &str,
    bytes: &[u8],
) -> Result<PuzzleContent, PackError> {
    format::parse_puzzle(pack_id, puzzle_id, bytes)
}

pub(crate) fn validate_files(files: BTreeMap<String, Vec<u8>>) -> Result<ValidatedPack, PackError> {
    if files.is_empty() {
        return Err(PackError::one("pack", "pack has no files"));
    }
    let extracted_bytes = files.values().try_fold(0_u64, |total, bytes| {
        total
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| PackError::one("pack", "extracted size exceeds the limit"))
    })?;
    if extracted_bytes > MAX_EXTRACTED_BYTES {
        return Err(PackError::one("pack", "extracted size exceeds the limit"));
    }

    let metadata_bytes = files
        .get("pack.toml")
        .ok_or_else(|| PackError::one("pack.toml", "required metadata is missing"))?;
    let metadata = format::parse_metadata(metadata_bytes)?;
    let mut issues = Vec::new();
    let mut puzzles = Vec::with_capacity(metadata.puzzle_ids().len());
    let declared: BTreeSet<&str> = metadata.puzzle_ids().iter().map(String::as_str).collect();

    for puzzle_id in metadata.puzzle_ids() {
        let path = format!("puzzles/{puzzle_id}.toml");
        match files.get(&path) {
            Some(bytes) => match format::parse_puzzle(metadata.id(), puzzle_id, bytes) {
                Ok(puzzle) => puzzles.push(puzzle),
                Err(PackError::Invalid {
                    issues: puzzle_issues,
                }) => {
                    append_issues(&mut issues, puzzle_issues);
                }
                Err(error) => return Err(error),
            },
            None => push_issue(
                &mut issues,
                PackIssue::new(path, "declared puzzle is missing"),
            ),
        }
    }

    for (path, bytes) in &files {
        if path == "pack.toml" {
            continue;
        }
        let Some((directory, filename)) = path.split_once('/') else {
            push_issue(
                &mut issues,
                PackIssue::new(path.as_str(), "file is not declared by the schema"),
            );
            continue;
        };
        let (stem, extension) = filename.rsplit_once('.').unwrap_or((filename, ""));
        let allowed = match (directory, extension) {
            ("puzzles", "toml") | ("notes", "txt") => declared.contains(stem),
            _ => false,
        };
        if !allowed {
            push_issue(
                &mut issues,
                PackIssue::new(path.as_str(), "file is not declared by the schema"),
            );
        } else if directory == "notes" {
            match std::str::from_utf8(bytes) {
                Ok(note) if note.chars().any(char::is_control) => push_issue(
                    &mut issues,
                    PackIssue::new(path.as_str(), "note contains a control character"),
                ),
                Err(_) => push_issue(
                    &mut issues,
                    PackIssue::new(path.as_str(), "note is not valid UTF-8"),
                ),
                Ok(_) => {}
            }
        }
    }

    if !issues.is_empty() {
        return Err(PackError::Invalid {
            issues: issues.into_boxed_slice(),
        });
    }
    if puzzles.len() != metadata.puzzle_ids().len() {
        return Err(PackError::one(
            "pack",
            "not every declared puzzle was loaded",
        ));
    }
    let fingerprint = format::content_fingerprint(metadata.format_version(), &files);
    Ok(ValidatedPack {
        metadata,
        puzzles: puzzles.into_boxed_slice(),
        files,
        fingerprint,
        extracted_bytes,
    })
}

fn append_issues(target: &mut Vec<PackIssue>, source: Box<[PackIssue]>) {
    for issue in source {
        push_issue(target, issue);
    }
}

fn push_issue(issues: &mut Vec<PackIssue>, issue: PackIssue) {
    if issues.len() < MAX_VALIDATION_ISSUES {
        issues.push(issue);
    }
}

pub(crate) fn validate_relative_path(relative: &str) -> Result<(), PackError> {
    if relative.is_empty()
        || relative.len() > MAX_RELATIVE_PATH_BYTES
        || !relative.is_ascii()
        || relative.starts_with('/')
        || relative.contains('\\')
    {
        return Err(PackError::one("path", "path is not portable and relative"));
    }
    let components: Vec<&str> = relative.split('/').collect();
    if components.len() > MAX_PATH_DEPTH {
        return Err(PackError::one(relative, "path depth exceeds the limit"));
    }
    for component in components {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.len() > MAX_COMPONENT_BYTES
            || component.ends_with('.')
            || component.ends_with(' ')
        {
            return Err(PackError::one(relative, "path component is not portable"));
        }
        let stem = component.split('.').next().unwrap_or(component);
        if is_windows_device(stem) {
            return Err(PackError::one(relative, "path uses a reserved device name"));
        }
        if !component.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
        }) {
            return Err(PackError::one(
                relative,
                "path contains a disallowed character",
            ));
        }
    }
    Ok(())
}

fn is_windows_device(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && matches!(&upper[..3], "COM" | "LPT")
            && matches!(upper.as_bytes()[3], b'1'..=b'9'))
}

pub(crate) fn content_limit(relative: &str) -> Result<u64, PackError> {
    if relative == "pack.toml" {
        return Ok(MAX_METADATA_BYTES);
    }
    if let Some((directory, filename)) = relative.split_once('/') {
        let extension = filename.rsplit_once('.').map(|(_, extension)| extension);
        if directory == "puzzles" && extension == Some("toml") {
            return Ok(MAX_PUZZLE_BYTES);
        }
        if directory == "notes" && extension == Some("txt") {
            return Ok(MAX_NOTE_BYTES);
        }
    }
    Err(PackError::one(
        relative,
        "file is not declared by the schema",
    ))
}

fn read_file_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, PackError> {
    let path_metadata = fs::symlink_metadata(path)?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(PackError::one(
            "file",
            "file type changed during validation",
        ));
    }
    let file = File::open(path)?;
    validate_open_file(&path_metadata, &file)?;
    if file.metadata()?.len() > limit {
        return Err(PackError::one("file", "file size exceeds its limit"));
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(PackError::one("file", "file size exceeds its limit"));
    }
    Ok(bytes)
}

#[cfg(unix)]
fn validate_open_file(path_metadata: &fs::Metadata, file: &File) -> Result<(), PackError> {
    use std::os::unix::fs::MetadataExt;

    let opened = file.metadata()?;
    if opened.nlink() != 1
        || opened.dev() != path_metadata.dev()
        || opened.ino() != path_metadata.ino()
    {
        return Err(PackError::one(
            "file",
            "hard links and files changed during validation are not accepted",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_open_file(_path_metadata: &fs::Metadata, file: &File) -> Result<(), PackError> {
    let information = winapi_util::file::information(file)?;
    let file_type = winapi_util::file::typ(file)?;
    if information.number_of_links() != 1 || !file_type.is_disk() {
        return Err(PackError::one(
            "file",
            "hard links and special files are not accepted",
        ));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn validate_open_file(_path_metadata: &fs::Metadata, file: &File) -> Result<(), PackError> {
    if !file.metadata()?.is_file() {
        return Err(PackError::one("file", "special files are not accepted"));
    }
    Ok(())
}

#[must_use]
pub fn fingerprint_hex(fingerprint: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(64);
    for byte in fingerprint {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}
