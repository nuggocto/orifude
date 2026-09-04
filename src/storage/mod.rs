//! Durable single-process progress and installed-pack storage.

mod paths;
mod replay;

use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{
    Connection, ErrorCode, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
    limits::Limit, params,
};

use crate::domain::puzzle::Puzzle;
use crate::domain::puzzle::PuzzleIdentity;
use crate::domain::replay::Replay;
use crate::generator::CalendarDate;
use crate::packs::{
    MAX_INSTALLED_PACKS, MAX_MANAGED_BYTES, PackError, ValidatedPack, fingerprint_hex,
    validate_directory, validate_source,
};

pub use paths::{AppPaths, PathError};
pub use replay::{CURRENT_REPLAY_FORMAT_VERSION, DecodedReplay, MAX_REPLAY_BYTES};

const SCHEMA_VERSION: u32 = 3;
const PAGE_SIZE: u64 = 4 * 1024;
const PAGE_SIZE_DB: i64 = 4 * 1024;
const MAIN_FILE_LIMIT: u64 = 128 * 1024 * 1024;
const MAX_PAGE_COUNT: u64 = MAIN_FILE_LIMIT / PAGE_SIZE;
const MAX_PAGE_COUNT_DB: i64 = 32 * 1024;
const NONESSENTIAL_RESERVE: u64 = 16 * 1024 * 1024;
const RESERVE_PAGES: u64 = NONESSENTIAL_RESERVE / PAGE_SIZE;
const TRANSIENT_SIDECAR_LIMIT: u64 = 132 * 1024 * 1024;
const RECENT_REPLAYS_DB: i64 = 20;
const PRUNE_BATCH_DB: i64 = 256;
const MAX_MANAGED_ENTRIES: usize = MAX_INSTALLED_PACKS + 2;
const MAX_INSTALLED_PACKS_DB: u64 = 32;
const MAX_DATABASE_VALUE_BYTES: i32 = 1024 * 1024;
pub const PROGRESS_PAGE_SIZE: usize = 128;
const PROGRESS_PAGE_QUERY_DB: i64 = 129;
const RESERVED_PACK_IDS: [&str; 4] = [
    "orifude-lesson",
    "orifude-journey",
    "orifude-daily",
    "orifude-endless",
];

/// Parses a bounded replay document and validates it through the domain engine.
///
/// # Errors
///
/// Returns when bytes are oversized, malformed, incompatible, or contain an
/// invalid puzzle or action sequence.
pub fn decode_replay_bytes(bytes: &[u8]) -> Result<DecodedReplay, StorageError> {
    replay::decode(bytes).map_err(|_| StorageError::ReplayData)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorMode {
    Auto,
    Color,
    Monochrome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlyphMode {
    Unicode,
    Ascii,
}

impl GlyphMode {
    const fn database_value(self) -> &'static str {
        match self {
            Self::Unicode => "unicode",
            Self::Ascii => "ascii",
        }
    }

    fn from_database(value: &str) -> Result<Self, StorageError> {
        match value {
            "unicode" => Ok(Self::Unicode),
            "ascii" => Ok(Self::Ascii),
            _ => Err(StorageError::Corrupt),
        }
    }
}

impl ColorMode {
    const fn database_value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Color => "color",
            Self::Monochrome => "monochrome",
        }
    }

    fn from_database(value: &str) -> Result<Self, StorageError> {
        match value {
            "auto" => Ok(Self::Auto),
            "color" => Ok(Self::Color),
            "monochrome" => Ok(Self::Monochrome),
            _ => Err(StorageError::Corrupt),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyBindings {
    pub fold: char,
    pub brush: char,
    pub undo: char,
    pub reset: char,
    pub preview: char,
    pub help: char,
    pub quit: char,
}

impl KeyBindings {
    #[must_use]
    pub fn is_conflict_free(self) -> bool {
        let keys = [
            self.fold,
            self.brush,
            self.undo,
            self.reset,
            self.preview,
            self.help,
            self.quit,
        ];
        keys.iter().enumerate().all(|(index, key)| {
            (key.is_ascii_graphic() || (index == 4 && *key == ' '))
                && !matches!(key, 'h' | 'j' | 'k' | 'l' | 't' | 'v' | 'x')
                && keys.iter().filter(|candidate| *candidate == key).count() == 1
        })
    }

    fn database_values(self) -> [String; 7] {
        [
            self.fold,
            self.brush,
            self.undo,
            self.reset,
            self.preview,
            self.help,
            self.quit,
        ]
        .map(|key| key.to_string())
    }

    fn from_database(values: [&str; 7]) -> Result<Self, StorageError> {
        let mut keys = ['\0'; 7];
        for (index, value) in values.into_iter().enumerate() {
            let mut characters = value.chars();
            keys[index] = characters.next().ok_or(StorageError::Corrupt)?;
            if characters.next().is_some() {
                return Err(StorageError::Corrupt);
            }
        }
        let bindings = Self {
            fold: keys[0],
            brush: keys[1],
            undo: keys[2],
            reset: keys[3],
            preview: keys[4],
            help: keys[5],
            quit: keys[6],
        };
        bindings
            .is_conflict_free()
            .then_some(bindings)
            .ok_or(StorageError::Corrupt)
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            fold: 'f',
            brush: 'b',
            undo: 'u',
            reset: 'r',
            preview: ' ',
            help: '?',
            quit: 'q',
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Settings {
    pub color_mode: ColorMode,
    pub glyph_mode: GlyphMode,
    pub reduced_motion: bool,
    pub instant_reveal: bool,
    pub lesson_complete: bool,
    pub bindings: KeyBindings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            color_mode: ColorMode::Auto,
            glyph_mode: GlyphMode::Unicode,
            reduced_motion: false,
            instant_reveal: false,
            lesson_complete: false,
            bindings: KeyBindings::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PuzzleProgress {
    pub pack_id: Box<str>,
    pub puzzle_id: Box<str>,
    pub attempt_count: u64,
    pub best_folds: u8,
    pub best_strokes: u8,
    pub best_replay_id: i64,
    pub updated_at_unix_seconds: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressPage {
    pub entries: Vec<PuzzleProgress>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DailyHistory {
    pub day: CalendarDate,
    pub generator_version: u16,
    pub pack_id: Box<str>,
    pub puzzle_id: Box<str>,
    pub completed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DailyKey {
    pub day: CalendarDate,
    pub generator_version: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredPack {
    pub id: Box<str>,
    pub title: Box<str>,
    pub description: Option<Box<str>>,
    pub authors: Box<str>,
    pub license: Box<str>,
    pub fingerprint: [u8; 32],
    pub extracted_bytes: u64,
    pub installed_at_unix_seconds: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallOutcome {
    Installed(RegisteredPack),
    AlreadyPresent(RegisteredPack),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageFootprint {
    pub main: u64,
    pub journal: u64,
    pub wal: u64,
    pub shared_memory: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqlitePolicy {
    pub page_size: u64,
    pub max_page_count: u64,
    pub journal_mode: Box<str>,
    pub cache_spill: bool,
}

impl StorageFootprint {
    #[must_use]
    pub const fn sidecars(self) -> u64 {
        self.journal
            .saturating_add(self.wal)
            .saturating_add(self.shared_memory)
    }
}

#[derive(Debug)]
pub enum StorageError {
    Io(io::Error),
    Sqlite(rusqlite::Error),
    Locked,
    Full,
    ReadOnly,
    Corrupt,
    UnsupportedSchema { found: u32, supported: u32 },
    UnsupportedPageSize { found: u64, required: u64 },
    ResourceLimit,
    InvalidSettings,
    InvalidCompletion,
    ReplayData,
    Pack(PackError),
    PackConflict,
    PackFingerprint,
    PackCleanup,
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(_) => formatter.write_str("local storage I/O failed"),
            Self::Sqlite(_) => formatter.write_str("the local database operation failed"),
            Self::Locked => formatter.write_str("another Orifude process owns the local database"),
            Self::Full => {
                formatter.write_str("local storage is full or its hard limit was reached")
            }
            Self::ReadOnly => formatter.write_str("the local storage path is read-only"),
            Self::Corrupt => {
                formatter.write_str("the local database is corrupt; keep it for recovery")
            }
            Self::UnsupportedSchema { found, supported } => write!(
                formatter,
                "database schema {found} is newer than supported schema {supported}"
            ),
            Self::UnsupportedPageSize { found, required } => write!(
                formatter,
                "database page size {found} does not match required size {required}"
            ),
            Self::ResourceLimit => {
                formatter.write_str("a local storage resource bound was reached")
            }
            Self::InvalidSettings => formatter.write_str("the requested settings are invalid"),
            Self::InvalidCompletion => {
                formatter.write_str("only a valid successful replay can be saved as a completion")
            }
            Self::ReplayData => formatter.write_str("saved replay data is invalid or incompatible"),
            Self::Pack(_) => formatter.write_str("the puzzle pack was not accepted"),
            Self::PackConflict => {
                formatter.write_str("an installed pack already uses this pack identity")
            }
            Self::PackFingerprint => formatter
                .write_str("installed pack content does not match its recorded fingerprint"),
            Self::PackCleanup => {
                formatter.write_str("pack state changed, but managed-file cleanup needs a retry")
            }
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(source) => Some(source),
            Self::Sqlite(source) => Some(source),
            Self::Pack(source) => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for StorageError {
    fn from(source: io::Error) -> Self {
        match source.kind() {
            io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => Self::ReadOnly,
            io::ErrorKind::StorageFull | io::ErrorKind::FileTooLarge => Self::Full,
            _ => Self::Io(source),
        }
    }
}

impl From<PackError> for StorageError {
    fn from(source: PackError) -> Self {
        Self::Pack(source)
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(source: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(error, _) = &source {
            return match error.code {
                ErrorCode::DatabaseBusy
                | ErrorCode::DatabaseLocked
                | ErrorCode::FileLockingProtocolFailed => Self::Locked,
                ErrorCode::DiskFull => Self::Full,
                ErrorCode::ReadOnly | ErrorCode::PermissionDenied | ErrorCode::CannotOpen => {
                    Self::ReadOnly
                }
                ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase => Self::Corrupt,
                ErrorCode::TooBig => Self::ResourceLimit,
                _ => Self::Sqlite(source),
            };
        }
        if matches!(
            source,
            rusqlite::Error::FromSqlConversionFailure(..)
                | rusqlite::Error::IntegralValueOutOfRange(..)
                | rusqlite::Error::Utf8Error(..)
                | rusqlite::Error::InvalidColumnType(..)
        ) {
            return Self::Corrupt;
        }
        Self::Sqlite(source)
    }
}

pub struct Storage {
    connection: Connection,
    paths: AppPaths,
    _lock: File,
    loaded_pack: Option<ValidatedPack>,
}

impl Storage {
    /// Opens the single writable database, migrates it, and reconciles one
    /// interrupted pack operation before returning registry state.
    ///
    /// # Errors
    ///
    /// Returns a typed lock, corruption, schema, capacity, permission, pack
    /// recovery, or underlying I/O error. Existing data is never reset.
    pub fn open(paths: AppPaths) -> Result<Self, StorageError> {
        create_private_directory(paths.data())?;
        create_private_directory(paths.config())?;
        create_private_directory(paths.cache())?;
        create_managed_directory(&paths.managed_packs())?;

        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(paths.lock())?;
        set_private_file(&lock)?;
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => StorageError::Locked,
            std::fs::TryLockError::Error(error)
                if error.kind() == io::ErrorKind::PermissionDenied =>
            {
                StorageError::Locked
            }
            std::fs::TryLockError::Error(error) => StorageError::Io(error),
        })?;

        verify_database_path(&paths.database())?;

        let mut connection = Connection::open_with_flags(
            paths.database(),
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::ZERO)?;
        let schema_version = check_schema_version(&connection)?;
        configure_runtime_limits(&connection)?;
        configure_database(&connection)?;
        migrate(&mut connection, schema_version)?;
        verify_database(&connection)?;
        sync_directory(paths.data())?;

        let mut storage = Self {
            connection,
            paths,
            _lock: lock,
            loaded_pack: None,
        };
        storage.reconcile_pack_install()?;
        storage.reconcile_registered_pack_paths()?;
        storage.cleanup_orphan_packs()?;
        storage.verify_registry_bounds()?;
        let _registered = storage.registered_packs()?;
        storage.verify_footprint()?;
        Ok(storage)
    }

    #[must_use]
    pub const fn paths(&self) -> &AppPaths {
        &self.paths
    }

    /// Reads rendering-independent user preferences.
    ///
    /// # Errors
    ///
    /// Returns when persisted settings are corrupt or SQLite cannot read them.
    pub fn settings(&self) -> Result<Settings, StorageError> {
        let row = self.connection.query_row(
            "SELECT color_mode, glyph_mode, reduced_motion, instant_reveal,
                    lesson_complete, bind_fold, bind_brush, bind_undo, bind_reset,
                    bind_preview, bind_help, bind_quit
             FROM settings WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, bool>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                ))
            },
        )?;
        Ok(Settings {
            color_mode: ColorMode::from_database(&row.0)?,
            glyph_mode: GlyphMode::from_database(&row.1)?,
            reduced_motion: row.2,
            instant_reveal: row.3,
            lesson_complete: row.4,
            bindings: KeyBindings::from_database([
                &row.5, &row.6, &row.7, &row.8, &row.9, &row.10, &row.11,
            ])?,
        })
    }

    /// Durably writes preferences without depending on terminal UI types.
    ///
    /// # Errors
    ///
    /// Returns a typed database or filesystem error with prior settings intact.
    pub fn save_settings(&mut self, settings: Settings) -> Result<(), StorageError> {
        if !settings.bindings.is_conflict_free() {
            return Err(StorageError::InvalidSettings);
        }
        self.verify_footprint()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let color = settings.color_mode.database_value();
        let glyphs = settings.glyph_mode.database_value();
        let bindings = settings.bindings.database_values();
        let changed = transaction.execute(
            "UPDATE settings
             SET color_mode = ?1, glyph_mode = ?2, reduced_motion = ?3,
                 instant_reveal = ?4, lesson_complete = ?5, bind_fold = ?6,
                 bind_brush = ?7, bind_undo = ?8, bind_reset = ?9,
                 bind_preview = ?10, bind_help = ?11, bind_quit = ?12
             WHERE singleton = 1",
            params![
                color,
                glyphs,
                settings.reduced_motion,
                settings.instant_reveal,
                settings.lesson_complete,
                bindings[0],
                bindings[1],
                bindings[2],
                bindings[3],
                bindings[4],
                bindings[5],
                bindings[6],
            ],
        )?;
        if changed != 1 {
            return Err(StorageError::Corrupt);
        }
        transaction.commit().map_err(StorageError::from)
    }

    /// Saves one successful attempt, replay, and best-progress update in one
    /// durable transaction.
    ///
    /// # Errors
    ///
    /// Rejects failed or incompatible replays before opening the transaction.
    /// Capacity failure rolls back all three records together.
    pub fn record_completion(
        &mut self,
        puzzle: &Puzzle,
        replay: &Replay,
        completed_at_unix_seconds: i64,
        undo_count: u64,
        hints_used: bool,
    ) -> Result<PuzzleProgress, StorageError> {
        self.record_completion_inner(
            puzzle,
            replay,
            completed_at_unix_seconds,
            undo_count,
            hints_used,
            None,
        )
    }

    /// Saves a successful daily attempt and marks its day complete atomically.
    ///
    /// # Errors
    ///
    /// Uses the same validation and rollback guarantees as [`Self::record_completion`].
    pub fn record_daily_completion(
        &mut self,
        daily: DailyKey,
        puzzle: &Puzzle,
        replay: &Replay,
        completed_at_unix_seconds: i64,
        undo_count: u64,
        hints_used: bool,
    ) -> Result<PuzzleProgress, StorageError> {
        if daily.generator_version == 0 {
            return Err(StorageError::ResourceLimit);
        }
        self.record_completion_inner(
            puzzle,
            replay,
            completed_at_unix_seconds,
            undo_count,
            hints_used,
            Some((daily.day, daily.generator_version)),
        )
    }

    fn record_completion_inner(
        &mut self,
        puzzle: &Puzzle,
        replay: &Replay,
        completed_at_unix_seconds: i64,
        undo_count: u64,
        hints_used: bool,
        daily: Option<(CalendarDate, u16)>,
    ) -> Result<PuzzleProgress, StorageError> {
        let attempt = replay
            .execute(puzzle)
            .map_err(|_| StorageError::InvalidCompletion)?;
        let result = attempt.result();
        if !result.is_success() {
            return Err(StorageError::InvalidCompletion);
        }
        let payload = replay::encode(puzzle, replay).map_err(|_| StorageError::ReplayData)?;
        let score = result.score();
        let record = CompletionRecord {
            pack_id: puzzle.identity().pack_id(),
            puzzle_id: puzzle.identity().puzzle_id(),
            completed_at: completed_at_unix_seconds,
            folds: score.folds().get(),
            strokes: score.strokes().get(),
            undo_count,
            hints_used,
            payload: &payload,
        };
        self.verify_footprint()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (replay_id, previous, is_best) = insert_completion_rows(&transaction, &record)?;
        let progress = update_progress(&transaction, &record, replay_id, previous, is_best)?;
        if let Some((day, generator_version)) = daily {
            let changed = transaction.execute(
                "INSERT INTO daily_history(day, generator_version, pack_id, puzzle_id, completed)
                 VALUES (?1, ?2, ?3, ?4, 1)
                 ON CONFLICT(day, generator_version) DO UPDATE SET
                   completed = 1
                 WHERE daily_history.pack_id = excluded.pack_id
                   AND daily_history.puzzle_id = excluded.puzzle_id",
                params![
                    day.to_string(),
                    generator_version,
                    record.pack_id,
                    record.puzzle_id,
                ],
            )?;
            if changed != 1 {
                return Err(StorageError::Corrupt);
            }
        }
        prune_puzzle_history(&transaction, record.pack_id, record.puzzle_id)?;
        let reserve_restored = restore_nonessential_reserve(&transaction)?;
        if !reserve_restored && !is_best {
            transaction.execute(
                "DELETE FROM attempts
                 WHERE id = (
                   SELECT attempt_id FROM replays WHERE id = ?1 AND is_best = 0
                 )",
                [replay_id],
            )?;
        }
        transaction.commit()?;
        Ok(progress)
    }

    /// Returns saved progress for one stable puzzle identity.
    ///
    /// # Errors
    ///
    /// Returns a database error without changing state.
    pub fn progress(
        &self,
        pack_id: &str,
        puzzle_id: &str,
    ) -> Result<Option<PuzzleProgress>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT attempt_count, best_folds, best_strokes, best_replay_id, updated_at
                 FROM progress WHERE pack_id = ?1 AND puzzle_id = ?2",
                params![pack_id, puzzle_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, u8>(1)?,
                        row.get::<_, u8>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?;
        row.map(
            |(attempt_count, best_folds, best_strokes, best_replay_id, updated_at)| {
                Ok(PuzzleProgress {
                    pack_id: pack_id.into(),
                    puzzle_id: puzzle_id.into(),
                    attempt_count: i64_to_u64(attempt_count)?,
                    best_folds,
                    best_strokes,
                    best_replay_id,
                    updated_at_unix_seconds: updated_at,
                })
            },
        )
        .transpose()
    }

    /// Returns one bounded page of saved puzzle summaries, newest first.
    ///
    /// # Errors
    ///
    /// Returns when a row is corrupt or SQLite cannot complete the read.
    pub fn progress_page(&self, offset: u64) -> Result<ProgressPage, StorageError> {
        let offset = i64::try_from(offset).map_err(|_| StorageError::ResourceLimit)?;
        let mut statement = self.connection.prepare(
            "SELECT pack_id, puzzle_id, attempt_count, best_folds, best_strokes,
                    best_replay_id, updated_at
             FROM progress ORDER BY updated_at DESC, pack_id, puzzle_id LIMIT ?1 OFFSET ?2",
        )?;
        let rows = statement.query_map(params![PROGRESS_PAGE_QUERY_DB, offset], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, u8>(3)?,
                row.get::<_, u8>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;
        let mut progress = Vec::with_capacity(PROGRESS_PAGE_SIZE + 1);
        for row in rows {
            let (pack_id, puzzle_id, attempt_count, folds, strokes, replay_id, updated_at) = row?;
            PuzzleIdentity::new(&pack_id, &puzzle_id).map_err(|_| StorageError::Corrupt)?;
            progress.push(PuzzleProgress {
                pack_id: pack_id.into_boxed_str(),
                puzzle_id: puzzle_id.into_boxed_str(),
                attempt_count: i64_to_u64(attempt_count)?,
                best_folds: folds,
                best_strokes: strokes,
                best_replay_id: replay_id,
                updated_at_unix_seconds: updated_at,
            });
        }
        let has_more = progress.len() > PROGRESS_PAGE_SIZE;
        progress.truncate(PROGRESS_PAGE_SIZE);
        Ok(ProgressPage {
            entries: progress,
            has_more,
        })
    }

    /// Returns the newest bounded page of saved puzzle summaries.
    ///
    /// # Errors
    ///
    /// Returns when a row is corrupt or SQLite cannot complete the read.
    pub fn recent_progress(&self) -> Result<Vec<PuzzleProgress>, StorageError> {
        self.progress_page(0).map(|page| page.entries)
    }

    /// Loads and validates the current best replay document.
    ///
    /// # Errors
    ///
    /// Returns when the database or bounded replay document is invalid.
    pub fn best_replay(
        &self,
        pack_id: &str,
        puzzle_id: &str,
    ) -> Result<Option<DecodedReplay>, StorageError> {
        let payload: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT r.payload FROM replays r
                 JOIN progress p ON p.best_replay_id = r.id
                 WHERE p.pack_id = ?1 AND p.puzzle_id = ?2",
                params![pack_id, puzzle_id],
                |row| row.get(0),
            )
            .optional()?;
        let decoded = payload
            .map(|bytes| replay::decode(&bytes).map_err(|_| StorageError::ReplayData))
            .transpose()?;
        if decoded.as_ref().is_some_and(|decoded| {
            decoded.puzzle().identity().pack_id() != pack_id
                || decoded.puzzle().identity().puzzle_id() != puzzle_id
        }) {
            return Err(StorageError::ReplayData);
        }
        Ok(decoded)
    }

    /// Reports whether the saved best replay belongs to this exact gameplay
    /// definition.
    ///
    /// # Errors
    ///
    /// Returns when the database or bounded replay document is invalid.
    pub fn completion_matches(&self, puzzle: &Puzzle) -> Result<bool, StorageError> {
        let identity = puzzle.identity();
        self.best_replay(identity.pack_id(), identity.puzzle_id())
            .map(|saved| saved.is_some_and(|saved| saved.puzzle() == puzzle))
    }

    /// Records an offline daily selection and completion state.
    ///
    /// # Errors
    ///
    /// Returns a typed database error with the prior row intact.
    pub fn record_daily(
        &mut self,
        day: CalendarDate,
        generator_version: u16,
        puzzle: &Puzzle,
        completed: bool,
    ) -> Result<(), StorageError> {
        if generator_version == 0 {
            return Err(StorageError::ResourceLimit);
        }
        self.verify_footprint()?;
        let day = day.to_string();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "INSERT INTO daily_history(day, generator_version, pack_id, puzzle_id, completed)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(day, generator_version) DO UPDATE SET
               completed = daily_history.completed OR excluded.completed
             WHERE daily_history.pack_id = excluded.pack_id
               AND daily_history.puzzle_id = excluded.puzzle_id",
            params![
                day,
                generator_version,
                puzzle.identity().pack_id(),
                puzzle.identity().puzzle_id(),
                completed
            ],
        )?;
        if changed != 1 {
            return Err(StorageError::Corrupt);
        }
        if !restore_nonessential_reserve(&transaction)? {
            return Err(StorageError::Full);
        }
        transaction.commit().map_err(StorageError::from)
    }

    /// Reads one generator-versioned daily selection.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the generator version is invalid or SQLite
    /// cannot read the bounded row.
    pub fn daily_history(
        &self,
        day: CalendarDate,
        generator_version: u16,
    ) -> Result<Option<DailyHistory>, StorageError> {
        if generator_version == 0 {
            return Err(StorageError::ResourceLimit);
        }
        let day_text = day.to_string();
        let history = self
            .connection
            .query_row(
                "SELECT pack_id, puzzle_id, completed FROM daily_history
                 WHERE day = ?1 AND generator_version = ?2",
                params![day_text, generator_version],
                |row| {
                    Ok(DailyHistory {
                        day,
                        generator_version,
                        pack_id: row.get::<_, String>(0)?.into_boxed_str(),
                        puzzle_id: row.get::<_, String>(1)?.into_boxed_str(),
                        completed: row.get(2)?,
                    })
                },
            )
            .optional()?;
        if history.as_ref().is_some_and(|history| {
            PuzzleIdentity::new(&history.pack_id, &history.puzzle_id).is_err()
        }) {
            return Err(StorageError::Corrupt);
        }
        Ok(history)
    }

    /// Returns the bounded installed-pack catalog without parsing pack files.
    ///
    /// # Errors
    ///
    /// Returns when registry data is corrupt or cannot be read.
    pub fn registered_packs(&self) -> Result<Vec<RegisteredPack>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT pack_id, title, description, authors, license, fingerprint,
                    extracted_bytes, installed_at, managed_name
             FROM pack_registry ORDER BY pack_id LIMIT 33",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((registered_pack_from_row(row)?, row.get::<_, String>(8)?))
        })?;
        let registered = rows.collect::<Result<Vec<_>, _>>()?;
        let mut packs = Vec::with_capacity(registered.len());
        for (pack, managed_name) in registered {
            validate_registered_pack(&pack, &managed_name)?;
            packs.push(pack);
        }
        if packs.len() > MAX_INSTALLED_PACKS {
            return Err(StorageError::Corrupt);
        }
        packs.shrink_to_fit();
        Ok(packs)
    }

    /// Validates and atomically installs one local directory or ZIP pack.
    ///
    /// # Errors
    ///
    /// Returns a bounded source, conflict, capacity, recovery, filesystem, or
    /// database error. A committed pending operation is recovered on restart.
    pub fn install_pack(
        &mut self,
        source: &Path,
        installed_at_unix_seconds: i64,
    ) -> Result<InstallOutcome, StorageError> {
        let pack = validate_source(source)?;
        self.install_validated(&pack, installed_at_unix_seconds)
    }

    /// Verifies and loads one selected community pack, replacing any previous
    /// in-memory community pack.
    ///
    /// # Errors
    ///
    /// A missing registry row, invalid managed name, content validation error,
    /// or fingerprint mismatch disables loading without changing the registry.
    pub fn load_pack(&mut self, pack_id: &str) -> Result<Option<&ValidatedPack>, StorageError> {
        let registered = self.registered_pack_row(pack_id)?;
        let Some((summary, managed_name)) = registered else {
            self.loaded_pack = None;
            return Ok(None);
        };
        if !is_fingerprint_name(&managed_name) {
            return Err(StorageError::Corrupt);
        }
        let loaded = self.read_managed_pack(&summary, &managed_name)?;
        self.loaded_pack = Some(loaded);
        Ok(self.loaded_pack.as_ref())
    }

    /// Discards the validated pack cache after a caller has copied the bounded
    /// playable projection it needs.
    pub(crate) fn clear_loaded_pack(&mut self) {
        self.loaded_pack = None;
    }

    /// Removes a pack from play before deleting its managed files. Puzzle
    /// progress and replay records deliberately have no registry foreign key.
    ///
    /// # Errors
    ///
    /// Returns a database error before logical removal, or a cleanup error
    /// after logical removal. Startup retries the latter once.
    pub fn remove_pack(&mut self, pack_id: &str) -> Result<bool, StorageError> {
        let managed_name: Option<String> = self
            .connection
            .query_row(
                "SELECT managed_name FROM pack_registry WHERE pack_id = ?1",
                [pack_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(managed_name) = managed_name else {
            return Ok(false);
        };
        if !is_fingerprint_name(&managed_name) {
            return Err(StorageError::Corrupt);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute("DELETE FROM pack_registry WHERE pack_id = ?1", [pack_id])?;
        transaction.commit()?;
        self.loaded_pack = None;
        let path = self.paths.managed_packs().join(managed_name);
        if path.exists() {
            fs::remove_dir_all(path).map_err(|_| StorageError::PackCleanup)?;
        }
        Ok(true)
    }

    /// Reports actual SQLite file and sidecar lengths.
    ///
    /// # Errors
    ///
    /// Returns when file metadata cannot be read.
    pub fn footprint(&self) -> Result<StorageFootprint, StorageError> {
        let database = self.paths.database();
        Ok(StorageFootprint {
            main: file_length(&database)?,
            journal: file_length(&suffix_path(&database, "-journal"))?,
            wal: file_length(&suffix_path(&database, "-wal"))?,
            shared_memory: file_length(&suffix_path(&database, "-shm"))?,
        })
    }

    /// Reports the policy applied to this writable application connection.
    ///
    /// # Errors
    ///
    /// Returns when SQLite cannot read its active pragma values.
    pub fn sqlite_policy(&self) -> Result<SqlitePolicy, StorageError> {
        let page_size: i64 = self
            .connection
            .query_row("PRAGMA page_size", [], |row| row.get(0))?;
        let max_page_count: i64 =
            self.connection
                .query_row("PRAGMA max_page_count", [], |row| row.get(0))?;
        let journal_mode: String = self
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        let cache_spill: i64 = self
            .connection
            .query_row("PRAGMA cache_spill", [], |row| row.get(0))?;
        Ok(SqlitePolicy {
            page_size: i64_to_u64(page_size)?,
            max_page_count: i64_to_u64(max_page_count)?,
            journal_mode: journal_mode.into_boxed_str(),
            cache_spill: cache_spill != 0,
        })
    }

    #[must_use]
    pub const fn main_file_limit() -> u64 {
        MAIN_FILE_LIMIT
    }

    #[must_use]
    pub const fn transient_sidecar_limit() -> u64 {
        TRANSIENT_SIDECAR_LIMIT
    }

    pub(crate) fn install_validated(
        &mut self,
        pack: &ValidatedPack,
        installed_at_unix_seconds: i64,
    ) -> Result<InstallOutcome, StorageError> {
        if RESERVED_PACK_IDS.contains(&pack.metadata().id()) {
            return Err(StorageError::PackConflict);
        }
        if let Some((existing, managed_name)) = self.registered_pack_row(pack.metadata().id())? {
            if existing.fingerprint == pack.fingerprint() {
                self.read_managed_pack(&existing, &managed_name)?;
                return Ok(InstallOutcome::AlreadyPresent(existing));
            }
            return Err(StorageError::PackConflict);
        }
        let (count, bytes): (i64, i64) = self.connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(extracted_bytes), 0) FROM pack_registry",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let count = i64_to_u64(count)?;
        let bytes = i64_to_u64(bytes)?;
        if count >= MAX_INSTALLED_PACKS_DB
            || bytes
                .checked_add(pack.extracted_bytes())
                .is_none_or(|total| total > MAX_MANAGED_BYTES)
        {
            return Err(StorageError::ResourceLimit);
        }
        let final_name = pack.fingerprint_hex();
        let final_path = self.paths.managed_packs().join(&final_name);
        if final_path.exists() {
            return Err(StorageError::PackConflict);
        }
        self.verify_footprint()?;
        let staging = self.paths.pack_staging();
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|_| StorageError::PackCleanup)?;
        }
        let prepared = (|| {
            create_private_directory(&staging)?;
            write_pack_files(&staging, pack)?;
            sync_directory(&staging)
        })();
        if let Err(error) = prepared {
            if staging.exists() && fs::remove_dir_all(&staging).is_err() {
                return Err(StorageError::PackCleanup);
            }
            return Err(error);
        }

        let pending_result = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute(
                "INSERT INTO pending_install(
                     singleton, pack_id, fingerprint, final_name, installed_at
                 ) VALUES (1, ?1, ?2, ?3, ?4)",
                params![
                    pack.metadata().id(),
                    pack.fingerprint().as_slice(),
                    final_name,
                    installed_at_unix_seconds
                ],
            )?;
            transaction.commit().map_err(StorageError::from)
        })();
        if let Err(error) = pending_result {
            if fs::remove_dir_all(&staging).is_err() {
                return Err(StorageError::PackCleanup);
            }
            return Err(error);
        }

        fs::rename(&staging, &final_path)?;
        sync_directory(&self.paths.managed_packs())?;
        sync_directory(self.paths.data())?;
        let summary = register_pack(
            &mut self.connection,
            pack,
            &final_name,
            installed_at_unix_seconds,
        )?;
        Ok(InstallOutcome::Installed(summary))
    }

    fn registered_pack_row(
        &self,
        pack_id: &str,
    ) -> Result<Option<(RegisteredPack, String)>, StorageError> {
        let registered = self
            .connection
            .query_row(
                "SELECT pack_id, title, description, authors, license, fingerprint,
                        extracted_bytes, installed_at, managed_name
                 FROM pack_registry WHERE pack_id = ?1",
                [pack_id],
                |row| Ok((registered_pack_from_row(row)?, row.get::<_, String>(8)?)),
            )
            .optional()?;
        if let Some((summary, managed_name)) = &registered {
            validate_registered_pack(summary, managed_name)?;
        }
        Ok(registered)
    }

    fn read_managed_pack(
        &self,
        summary: &RegisteredPack,
        managed_name: &str,
    ) -> Result<ValidatedPack, StorageError> {
        validate_registered_pack(summary, managed_name)?;
        let path = self.paths.managed_packs().join(managed_name);
        let loaded = validate_directory(&path).map_err(|error| match error {
            PackError::Io(source) if source.kind() != io::ErrorKind::NotFound => {
                StorageError::from(source)
            }
            PackError::Io(_) | PackError::Invalid { .. } | PackError::SourceType => {
                StorageError::PackFingerprint
            }
            PackError::Archive => StorageError::PackFingerprint,
        })?;
        if loaded.metadata().id() != summary.id.as_ref()
            || loaded.fingerprint() != summary.fingerprint
        {
            return Err(StorageError::PackFingerprint);
        }
        Ok(loaded)
    }

    fn reconcile_pack_install(&mut self) -> Result<(), StorageError> {
        let pending: Option<(String, Vec<u8>, String, i64)> = self
            .connection
            .query_row(
                "SELECT pack_id, fingerprint, final_name, installed_at
                 FROM pending_install WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let staging = self.paths.pack_staging();
        let Some((pack_id, fingerprint, final_name, installed_at)) = pending else {
            if staging.exists() {
                fs::remove_dir_all(staging).map_err(|_| StorageError::PackCleanup)?;
            }
            return Ok(());
        };
        let expected = fingerprint_array(&fingerprint)?;
        if PuzzleIdentity::new(&pack_id, "probe").is_err()
            || !is_fingerprint_name(&final_name)
            || final_name != fingerprint_hex(expected)
        {
            return Err(StorageError::Corrupt);
        }
        if RESERVED_PACK_IDS.contains(&pack_id.as_str()) {
            if staging.exists() {
                fs::remove_dir_all(&staging).map_err(|_| StorageError::PackCleanup)?;
            }
            self.connection
                .execute("DELETE FROM pending_install WHERE singleton = 1", [])?;
            return Ok(());
        }
        let final_path = self.paths.managed_packs().join(&final_name);
        if final_path.is_dir() {
            let pack = validate_directory(&final_path)?;
            if pack.metadata().id() != pack_id || pack.fingerprint() != expected {
                return Err(StorageError::PackFingerprint);
            }
            register_pack(&mut self.connection, &pack, &final_name, installed_at)?;
            if staging.exists() {
                fs::remove_dir_all(staging).map_err(|_| StorageError::PackCleanup)?;
            }
            return Ok(());
        }
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|_| StorageError::PackCleanup)?;
        }
        self.connection
            .execute("DELETE FROM pending_install WHERE singleton = 1", [])?;
        Ok(())
    }

    fn reconcile_registered_pack_paths(&mut self) -> Result<(), StorageError> {
        let registered = {
            let mut statement = self.connection.prepare(
                "SELECT pack_id, managed_name FROM pack_registry ORDER BY pack_id LIMIT 37",
            )?;
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut missing = Vec::new();
        let mut active_count = 0_usize;
        for (pack_id, managed_name) in registered {
            if PuzzleIdentity::new(&pack_id, "probe").is_err() {
                return Err(StorageError::Corrupt);
            }
            if RESERVED_PACK_IDS.contains(&pack_id.as_str()) {
                missing.push(pack_id);
                continue;
            }
            active_count = active_count
                .checked_add(1)
                .ok_or(StorageError::ResourceLimit)?;
            if active_count > MAX_INSTALLED_PACKS || !is_fingerprint_name(&managed_name) {
                return Err(StorageError::Corrupt);
            }
            let path = self.paths.managed_packs().join(managed_name);
            match fs::symlink_metadata(path) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => missing.push(pack_id),
                Err(error) if error.kind() == io::ErrorKind::NotFound => missing.push(pack_id),
                Err(error) => return Err(error.into()),
            }
        }
        if missing.is_empty() {
            return Ok(());
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for pack_id in missing {
            let removed =
                transaction.execute("DELETE FROM pack_registry WHERE pack_id = ?1", [pack_id])?;
            if removed != 1 {
                return Err(StorageError::Corrupt);
            }
        }
        transaction.commit()?;
        Ok(())
    }

    fn cleanup_orphan_packs(&self) -> Result<(), StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT managed_name FROM pack_registry ORDER BY managed_name LIMIT 33")?;
        let live = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
        if live.len() > MAX_INSTALLED_PACKS {
            return Err(StorageError::Corrupt);
        }
        for managed_name in &live {
            if !is_fingerprint_name(managed_name) {
                return Err(StorageError::Corrupt);
            }
        }
        let mut entry_count = 0_usize;
        for entry in fs::read_dir(self.paths.managed_packs())? {
            entry_count = entry_count
                .checked_add(1)
                .ok_or(StorageError::ResourceLimit)?;
            if entry_count > MAX_MANAGED_ENTRIES {
                return Err(StorageError::ResourceLimit);
            }
            let entry = entry?;
            let name = entry.file_name();
            let is_live = name
                .to_str()
                .filter(|name| is_fingerprint_name(name))
                .is_some_and(|name| live.contains(name));
            if !is_live {
                let file_type = entry.file_type()?;
                if file_type.is_dir() && !file_type.is_symlink() {
                    fs::remove_dir_all(entry.path()).map_err(|_| StorageError::PackCleanup)?;
                } else {
                    fs::remove_file(entry.path()).map_err(|_| StorageError::PackCleanup)?;
                }
            }
        }
        Ok(())
    }

    fn verify_registry_bounds(&self) -> Result<(), StorageError> {
        let (count, bytes): (i64, i64) = self.connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(extracted_bytes), 0) FROM pack_registry",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if i64_to_u64(count)? > MAX_INSTALLED_PACKS_DB || i64_to_u64(bytes)? > MAX_MANAGED_BYTES {
            return Err(StorageError::ResourceLimit);
        }
        Ok(())
    }

    fn verify_footprint(&self) -> Result<(), StorageError> {
        let footprint = self.footprint()?;
        if footprint.main > MAIN_FILE_LIMIT || footprint.sidecars() > TRANSIENT_SIDECAR_LIMIT {
            return Err(StorageError::Full);
        }
        Ok(())
    }
}

struct CompletionRecord<'a> {
    pack_id: &'a str,
    puzzle_id: &'a str,
    completed_at: i64,
    folds: u8,
    strokes: u8,
    undo_count: u64,
    hints_used: bool,
    payload: &'a [u8],
}

#[derive(Clone, Copy)]
struct ExistingProgress {
    attempt_count: u64,
    best_folds: u8,
    best_strokes: u8,
    best_replay_id: i64,
}

fn insert_completion_rows(
    transaction: &Transaction<'_>,
    record: &CompletionRecord<'_>,
) -> Result<(i64, Option<ExistingProgress>, bool), StorageError> {
    transaction.execute(
        "INSERT INTO attempts(pack_id, puzzle_id, completed_at, folds, strokes, undo_count, hints_used, success)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
        params![
            record.pack_id,
            record.puzzle_id,
            record.completed_at,
            record.folds,
            record.strokes,
            u64_to_i64(record.undo_count)?,
            record.hints_used
        ],
    )?;
    let attempt_id = transaction.last_insert_rowid();
    let previous = transaction
        .query_row(
            "SELECT attempt_count, best_folds, best_strokes, best_replay_id
             FROM progress WHERE pack_id = ?1 AND puzzle_id = ?2",
            params![record.pack_id, record.puzzle_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, u8>(1)?,
                    row.get::<_, u8>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()?
        .map(
            |(count, folds, strokes, replay_id)| -> Result<_, StorageError> {
                Ok(ExistingProgress {
                    attempt_count: i64_to_u64(count)?,
                    best_folds: folds,
                    best_strokes: strokes,
                    best_replay_id: replay_id,
                })
            },
        )
        .transpose()?;
    let is_best = previous.is_none_or(|progress| {
        (record.folds, record.strokes) < (progress.best_folds, progress.best_strokes)
    });
    if is_best && let Some(progress) = previous {
        transaction.execute(
            "UPDATE replays SET is_best = 0 WHERE id = ?1",
            [progress.best_replay_id],
        )?;
    }
    transaction.execute(
        "INSERT INTO replays(attempt_id, pack_id, puzzle_id, created_at, payload, is_best)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            attempt_id,
            record.pack_id,
            record.puzzle_id,
            record.completed_at,
            record.payload,
            is_best
        ],
    )?;
    Ok((transaction.last_insert_rowid(), previous, is_best))
}

fn update_progress(
    transaction: &Transaction<'_>,
    record: &CompletionRecord<'_>,
    replay_id: i64,
    previous: Option<ExistingProgress>,
    is_best: bool,
) -> Result<PuzzleProgress, StorageError> {
    let (attempt_count, best_folds, best_strokes, best_replay_id) = match previous {
        Some(progress) if !is_best => (
            progress
                .attempt_count
                .checked_add(1)
                .ok_or(StorageError::ResourceLimit)?,
            progress.best_folds,
            progress.best_strokes,
            progress.best_replay_id,
        ),
        Some(progress) => (
            progress
                .attempt_count
                .checked_add(1)
                .ok_or(StorageError::ResourceLimit)?,
            record.folds,
            record.strokes,
            replay_id,
        ),
        None => (1, record.folds, record.strokes, replay_id),
    };
    transaction.execute(
        "INSERT INTO progress(pack_id, puzzle_id, attempt_count, best_folds, best_strokes, best_replay_id, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(pack_id, puzzle_id) DO UPDATE SET
           attempt_count = excluded.attempt_count,
           best_folds = excluded.best_folds,
           best_strokes = excluded.best_strokes,
           best_replay_id = excluded.best_replay_id,
           updated_at = excluded.updated_at",
        params![
            record.pack_id,
            record.puzzle_id,
            u64_to_i64(attempt_count)?,
            best_folds,
            best_strokes,
            best_replay_id,
            record.completed_at
        ],
    )?;
    Ok(PuzzleProgress {
        pack_id: record.pack_id.into(),
        puzzle_id: record.puzzle_id.into(),
        attempt_count,
        best_folds,
        best_strokes,
        best_replay_id,
        updated_at_unix_seconds: record.completed_at,
    })
}

fn configure_database(connection: &Connection) -> Result<(), StorageError> {
    let page_count: i64 = connection.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    if page_count == 0 {
        connection.pragma_update(None, "page_size", PAGE_SIZE_DB)?;
    }
    let page_size: i64 = connection.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    if page_size != PAGE_SIZE_DB {
        return Err(StorageError::UnsupportedPageSize {
            found: i64_to_u64(page_size)?,
            required: PAGE_SIZE,
        });
    }
    connection.pragma_update(None, "max_page_count", MAX_PAGE_COUNT_DB)?;
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA trusted_schema = OFF;
         PRAGMA journal_mode = DELETE;
         PRAGMA synchronous = FULL;
         PRAGMA temp_store = MEMORY;
         PRAGMA cache_spill = OFF;
         PRAGMA journal_size_limit = 0;",
    )?;
    let journal_mode: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    if !journal_mode.eq_ignore_ascii_case("delete") {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn check_schema_version(connection: &Connection) -> Result<u32, StorageError> {
    let version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSchema {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(version)
}

fn verify_database_path(path: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(StorageError::Corrupt);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn configure_runtime_limits(connection: &Connection) -> Result<(), StorageError> {
    for (limit, value) in [
        (Limit::SQLITE_LIMIT_LENGTH, MAX_DATABASE_VALUE_BYTES),
        (Limit::SQLITE_LIMIT_SQL_LENGTH, 64 * 1024),
        (Limit::SQLITE_LIMIT_COLUMN, 64),
        (Limit::SQLITE_LIMIT_EXPR_DEPTH, 128),
        (Limit::SQLITE_LIMIT_COMPOUND_SELECT, 16),
        (Limit::SQLITE_LIMIT_VDBE_OP, 100_000),
        (Limit::SQLITE_LIMIT_FUNCTION_ARG, 32),
        (Limit::SQLITE_LIMIT_ATTACHED, 0),
        (Limit::SQLITE_LIMIT_LIKE_PATTERN_LENGTH, 256),
        (Limit::SQLITE_LIMIT_VARIABLE_NUMBER, 64),
        (Limit::SQLITE_LIMIT_TRIGGER_DEPTH, 8),
        (Limit::SQLITE_LIMIT_WORKER_THREADS, 0),
    ] {
        connection.set_limit(limit, value)?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn migrate(connection: &mut Connection, version: u32) -> Result<(), StorageError> {
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    verify_migration_source(connection, version)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if version == 0 {
        transaction.execute_batch(
            "CREATE TABLE schema_metadata(
                 name TEXT PRIMARY KEY NOT NULL,
                 value TEXT NOT NULL
             ) STRICT;
             CREATE TABLE settings(
                 singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                 color_mode TEXT NOT NULL CHECK(color_mode IN ('auto', 'color', 'monochrome')),
                 glyph_mode TEXT NOT NULL CHECK(glyph_mode IN ('unicode', 'ascii')),
                 reduced_motion INTEGER NOT NULL CHECK(reduced_motion IN (0, 1)),
                 instant_reveal INTEGER NOT NULL CHECK(instant_reveal IN (0, 1)),
                 lesson_complete INTEGER NOT NULL CHECK(lesson_complete IN (0, 1)),
                 bind_fold TEXT NOT NULL CHECK(length(bind_fold) = 1),
                 bind_brush TEXT NOT NULL CHECK(length(bind_brush) = 1),
                 bind_undo TEXT NOT NULL CHECK(length(bind_undo) = 1),
                 bind_reset TEXT NOT NULL CHECK(length(bind_reset) = 1),
                 bind_preview TEXT NOT NULL CHECK(length(bind_preview) = 1),
                 bind_help TEXT NOT NULL CHECK(length(bind_help) = 1),
                 bind_quit TEXT NOT NULL CHECK(length(bind_quit) = 1)
             ) STRICT;
             CREATE TABLE progress(
                 pack_id TEXT NOT NULL,
                 puzzle_id TEXT NOT NULL,
                 attempt_count INTEGER NOT NULL CHECK(attempt_count >= 1),
                 best_folds INTEGER NOT NULL CHECK(best_folds BETWEEN 0 AND 12),
                 best_strokes INTEGER NOT NULL CHECK(best_strokes BETWEEN 0 AND 8),
                 best_replay_id INTEGER NOT NULL REFERENCES replays(id),
                 updated_at INTEGER NOT NULL,
                 PRIMARY KEY(pack_id, puzzle_id)
             ) STRICT;
             CREATE INDEX progress_recent
                 ON progress(updated_at DESC, pack_id, puzzle_id);
             CREATE TABLE attempts(
                 id INTEGER PRIMARY KEY,
                 pack_id TEXT NOT NULL,
                 puzzle_id TEXT NOT NULL,
                 completed_at INTEGER NOT NULL,
                 folds INTEGER NOT NULL CHECK(folds BETWEEN 0 AND 12),
                 strokes INTEGER NOT NULL CHECK(strokes BETWEEN 0 AND 8),
                 undo_count INTEGER NOT NULL CHECK(undo_count >= 0),
                 hints_used INTEGER NOT NULL CHECK(hints_used IN (0, 1)),
                 success INTEGER NOT NULL CHECK(success = 1)
             ) STRICT;
             CREATE TABLE replays(
                 id INTEGER PRIMARY KEY,
                 attempt_id INTEGER NOT NULL UNIQUE REFERENCES attempts(id) ON DELETE CASCADE,
                 pack_id TEXT NOT NULL,
                 puzzle_id TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 payload BLOB NOT NULL CHECK(length(payload) <= 65536),
                 is_best INTEGER NOT NULL CHECK(is_best IN (0, 1))
             ) STRICT;
             CREATE INDEX replay_history ON replays(pack_id, puzzle_id, created_at DESC, id DESC);
             CREATE UNIQUE INDEX one_best_replay ON replays(pack_id, puzzle_id) WHERE is_best = 1;
             CREATE TABLE daily_history(
                 day TEXT NOT NULL,
                 generator_version INTEGER NOT NULL CHECK(generator_version >= 1),
                 pack_id TEXT NOT NULL,
                 puzzle_id TEXT NOT NULL,
                 completed INTEGER NOT NULL CHECK(completed IN (0, 1)),
                 PRIMARY KEY(day, generator_version)
             ) STRICT;
             CREATE TABLE pack_registry(
                 pack_id TEXT PRIMARY KEY NOT NULL,
                 title TEXT NOT NULL,
                 description TEXT,
                 authors TEXT NOT NULL,
                 license TEXT NOT NULL,
                 fingerprint BLOB NOT NULL CHECK(length(fingerprint) = 32),
                 managed_name TEXT NOT NULL UNIQUE,
                 extracted_bytes INTEGER NOT NULL CHECK(extracted_bytes >= 0 AND extracted_bytes <= 16777216),
                 installed_at INTEGER NOT NULL
             ) STRICT;
             CREATE TABLE pending_install(
                 singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                 pack_id TEXT NOT NULL,
                 fingerprint BLOB NOT NULL CHECK(length(fingerprint) = 32),
                 final_name TEXT NOT NULL UNIQUE,
                 installed_at INTEGER NOT NULL
             ) STRICT;
             INSERT INTO schema_metadata(name, value) VALUES
                 ('schema', 'orifude-storage'),
                 ('journal-mode', 'delete'),
                 ('page-size', '4096');
             INSERT INTO settings(
                 singleton, color_mode, glyph_mode, reduced_motion, instant_reveal,
                 lesson_complete, bind_fold, bind_brush, bind_undo, bind_reset,
                 bind_preview, bind_help, bind_quit
             ) VALUES (1, 'auto', 'unicode', 0, 0, 0, 'f', 'b', 'u', 'r', ' ', '?', 'q');
             PRAGMA user_version = 3;",
        )?;
    } else {
        if version == 1 {
            transaction.execute_batch(
                "ALTER TABLE settings ADD COLUMN glyph_mode TEXT NOT NULL DEFAULT 'unicode'
                     CHECK(glyph_mode IN ('unicode', 'ascii'));",
            )?;
        }
        transaction.execute_batch(
            "ALTER TABLE settings ADD COLUMN lesson_complete INTEGER NOT NULL DEFAULT 0
                 CHECK(lesson_complete IN (0, 1));
             ALTER TABLE settings ADD COLUMN bind_fold TEXT NOT NULL DEFAULT 'f'
                 CHECK(length(bind_fold) = 1);
             ALTER TABLE settings ADD COLUMN bind_brush TEXT NOT NULL DEFAULT 'b'
                 CHECK(length(bind_brush) = 1);
             ALTER TABLE settings ADD COLUMN bind_undo TEXT NOT NULL DEFAULT 'u'
                 CHECK(length(bind_undo) = 1);
             ALTER TABLE settings ADD COLUMN bind_reset TEXT NOT NULL DEFAULT 'r'
                 CHECK(length(bind_reset) = 1);
             ALTER TABLE settings ADD COLUMN bind_preview TEXT NOT NULL DEFAULT ' '
                 CHECK(length(bind_preview) = 1);
             ALTER TABLE settings ADD COLUMN bind_help TEXT NOT NULL DEFAULT '?'
                 CHECK(length(bind_help) = 1);
             ALTER TABLE settings ADD COLUMN bind_quit TEXT NOT NULL DEFAULT 'q'
                 CHECK(length(bind_quit) = 1);
             CREATE INDEX progress_recent
                 ON progress(updated_at DESC, pack_id, puzzle_id);
             PRAGMA user_version = 3;",
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn verify_migration_source(connection: &Connection, version: u32) -> Result<(), StorageError> {
    if matches!(version, 1 | 2) {
        verify_database(connection)?;
    }
    Ok(())
}

fn verify_database(connection: &Connection) -> Result<(), StorageError> {
    let result: String = connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(StorageError::Corrupt);
    }
    let max_pages: i64 = connection.query_row("PRAGMA max_page_count", [], |row| row.get(0))?;
    if max_pages != MAX_PAGE_COUNT_DB {
        return Err(StorageError::Full);
    }
    let metadata_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM schema_metadata
             WHERE (name = 'schema' AND value = 'orifude-storage')
                OR (name = 'journal-mode' AND value = 'delete')
                OR (name = 'page-size' AND value = '4096')",
            [],
            |row| row.get(0),
        )
        .map_err(|_| StorageError::Corrupt)?;
    let metadata_total: i64 = connection
        .query_row("SELECT COUNT(*) FROM schema_metadata", [], |row| row.get(0))
        .map_err(|_| StorageError::Corrupt)?;
    let settings_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM settings WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| StorageError::Corrupt)?;
    let foreign_key_failure = connection
        .query_row("SELECT 1 FROM pragma_foreign_key_check LIMIT 1", [], |_| {
            Ok(())
        })
        .optional()
        .map_err(|_| StorageError::Corrupt)?;
    if metadata_rows != 3
        || metadata_total != 3
        || settings_rows != 1
        || foreign_key_failure.is_some()
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn prune_puzzle_history(
    transaction: &Transaction<'_>,
    pack_id: &str,
    puzzle_id: &str,
) -> Result<(), StorageError> {
    let mut statement = transaction.prepare(
        "SELECT attempt_id FROM replays
         WHERE pack_id = ?1 AND puzzle_id = ?2
           AND id NOT IN (
             SELECT id FROM replays
             WHERE pack_id = ?1 AND puzzle_id = ?2
             ORDER BY is_best DESC, created_at DESC, id DESC
             LIMIT ?3
           )
         ORDER BY created_at, id LIMIT ?4",
    )?;
    let attempt_ids = statement
        .query_map(
            params![pack_id, puzzle_id, RECENT_REPLAYS_DB, PRUNE_BATCH_DB],
            |row| row.get::<_, i64>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for attempt_id in attempt_ids {
        transaction.execute("DELETE FROM attempts WHERE id = ?1", [attempt_id])?;
    }
    Ok(())
}

fn restore_nonessential_reserve(transaction: &Transaction<'_>) -> Result<bool, StorageError> {
    if available_pages(transaction)? >= RESERVE_PAGES {
        return Ok(true);
    }
    let mut statement = transaction.prepare(
        "SELECT attempt_id FROM replays WHERE is_best = 0
         ORDER BY created_at, id LIMIT ?1",
    )?;
    let attempt_ids = statement
        .query_map([PRUNE_BATCH_DB], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for attempt_id in attempt_ids {
        transaction.execute("DELETE FROM attempts WHERE id = ?1", [attempt_id])?;
    }
    Ok(available_pages(transaction)? >= RESERVE_PAGES)
}

fn available_pages(transaction: &Transaction<'_>) -> Result<u64, StorageError> {
    let page_count: i64 = transaction.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    let freelist: i64 = transaction.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
    let page_count = i64_to_u64(page_count)?;
    let freelist = i64_to_u64(freelist)?;
    let used = page_count.saturating_sub(freelist);
    Ok(MAX_PAGE_COUNT.saturating_sub(used))
}

fn register_pack(
    connection: &mut Connection,
    pack: &ValidatedPack,
    final_name: &str,
    installed_at_unix_seconds: i64,
) -> Result<RegisteredPack, StorageError> {
    let metadata = pack.metadata();
    let authors = metadata
        .authors()
        .iter()
        .map(AsRef::as_ref)
        .collect::<Vec<&str>>()
        .join(", ");
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute(
        "INSERT INTO pack_registry(
             pack_id, title, description, authors, license, fingerprint,
             managed_name, extracted_bytes, installed_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            metadata.id(),
            metadata.title(),
            metadata.description(),
            authors,
            metadata.license(),
            pack.fingerprint().as_slice(),
            final_name,
            u64_to_i64(pack.extracted_bytes())?,
            installed_at_unix_seconds
        ],
    )?;
    let cleared = transaction.execute(
        "DELETE FROM pending_install
         WHERE singleton = 1 AND pack_id = ?1 AND fingerprint = ?2
           AND final_name = ?3 AND installed_at = ?4",
        params![
            metadata.id(),
            pack.fingerprint().as_slice(),
            final_name,
            installed_at_unix_seconds
        ],
    )?;
    if cleared != 1 {
        return Err(StorageError::Corrupt);
    }
    transaction.commit()?;
    Ok(RegisteredPack {
        id: metadata.id().into(),
        title: metadata.title().into(),
        description: metadata.description().map(Into::into),
        authors: authors.into_boxed_str(),
        license: metadata.license().into(),
        fingerprint: pack.fingerprint(),
        extracted_bytes: pack.extracted_bytes(),
        installed_at_unix_seconds,
    })
}

fn registered_pack_from_row(row: &rusqlite::Row<'_>) -> Result<RegisteredPack, rusqlite::Error> {
    let fingerprint: Vec<u8> = row.get(5)?;
    let fingerprint = fingerprint_array(&fingerprint).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Blob,
            Box::new(InvalidFingerprint),
        )
    })?;
    Ok(RegisteredPack {
        id: row.get::<_, String>(0)?.into_boxed_str(),
        title: row.get::<_, String>(1)?.into_boxed_str(),
        description: row.get::<_, Option<String>>(2)?.map(String::into_boxed_str),
        authors: row.get::<_, String>(3)?.into_boxed_str(),
        license: row.get::<_, String>(4)?.into_boxed_str(),
        fingerprint,
        extracted_bytes: u64::try_from(row.get::<_, i64>(6)?).map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Integer,
                Box::new(InvalidFingerprint),
            )
        })?,
        installed_at_unix_seconds: row.get(7)?,
    })
}

fn validate_registered_pack(pack: &RegisteredPack, managed_name: &str) -> Result<(), StorageError> {
    let display_is_valid = |value: &str, maximum: usize| {
        !value.trim().is_empty()
            && value.chars().count() <= maximum
            && !value.chars().any(char::is_control)
    };
    if PuzzleIdentity::new(&pack.id, "probe").is_err()
        || RESERVED_PACK_IDS.contains(&pack.id.as_ref())
        || !display_is_valid(&pack.title, 80)
        || pack
            .description
            .as_deref()
            .is_some_and(|value| !display_is_valid(value, 512))
        || pack.authors.chars().count() > 1_310
        || pack.authors.chars().any(char::is_control)
        || pack.license.len() > 128
        || pack.license.chars().any(char::is_control)
        || spdx::Expression::parse(&pack.license).is_err()
        || pack.extracted_bytes > crate::packs::MAX_EXTRACTED_BYTES
        || !is_fingerprint_name(managed_name)
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

#[derive(Debug)]
struct InvalidFingerprint;

impl fmt::Display for InvalidFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid fingerprint length")
    }
}

impl Error for InvalidFingerprint {}

fn fingerprint_array(bytes: &[u8]) -> Result<[u8; 32], StorageError> {
    bytes.try_into().map_err(|_| StorageError::Corrupt)
}

fn write_pack_files(root: &Path, pack: &ValidatedPack) -> Result<(), StorageError> {
    let mut directories = std::collections::BTreeSet::new();
    for (relative, contents) in pack.files() {
        let destination = root.join(relative);
        if let Some(parent) = destination.parent() {
            create_private_directory(parent)?;
            directories.insert(parent.to_path_buf());
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        set_private_file(&file)?;
        let mut file = file;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    for directory in directories {
        sync_directory(&directory)?;
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), StorageError> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn create_managed_directory(path: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(StorageError::Corrupt);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    create_private_directory(path)
}

fn set_private_file(file: &File) -> Result<(), StorageError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), StorageError> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    // Rust has no portable directory barrier on Windows. Startup reconciles a
    // registry row whose rename did not survive to the complete no-pack state.
    let _ = path;
    Ok(())
}

fn file_length(path: &Path) -> Result<u64, StorageError> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

fn suffix_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

fn is_fingerprint_name(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn u64_to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::ResourceLimit)
}

fn i64_to_u64(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::Corrupt)
}

#[cfg(test)]
mod tests {
    use super::{
        MAIN_FILE_LIMIT, MAX_PAGE_COUNT, PAGE_SIZE, StorageError, TRANSIENT_SIDECAR_LIMIT,
    };

    #[test]
    fn sqlite_capacity_and_permission_codes_keep_typed_recovery_paths() {
        let full = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL),
            None,
        );
        let read_only = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_READONLY),
            None,
        );
        assert!(matches!(StorageError::from(full), StorageError::Full));
        assert!(matches!(
            StorageError::from(read_only),
            StorageError::ReadOnly
        ));
    }

    #[test]
    fn rollback_journal_budget_covers_one_record_per_database_page() {
        const RECORD_FRAMING_BYTES: u64 = 8;
        const MAX_SQLITE_SECTOR_BYTES: u64 = 64 * 1024;
        let largest_journal =
            MAX_PAGE_COUNT * (PAGE_SIZE + RECORD_FRAMING_BYTES) + MAX_SQLITE_SECTOR_BYTES;
        assert_eq!(MAIN_FILE_LIMIT, MAX_PAGE_COUNT * PAGE_SIZE);
        assert!(largest_journal <= TRANSIENT_SIDECAR_LIMIT);
    }
}
