//! Durable single-process progress and installed-pack storage.

mod database;
mod managed_packs;
mod paths;
mod progress;
mod replay;
mod settings;

use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OpenFlags};

use crate::packs::{MAX_INSTALLED_PACKS, PackError, ValidatedPack};

pub use managed_packs::{InstallOutcome, RegisteredPack};
pub use paths::{AppPaths, PathError};
pub use progress::{DailyHistory, DailyKey, ProgressPage, PuzzleProgress};
pub use replay::{CURRENT_REPLAY_FORMAT_VERSION, DecodedReplay, MAX_REPLAY_BYTES};
pub use settings::{ColorMode, GlyphMode, KeyBindings, Settings};

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

        database::verify_database_path(&paths.database())?;

        let mut connection = Connection::open_with_flags(
            paths.database(),
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::ZERO)?;
        let schema_version = database::check_schema_version(&connection)?;
        database::configure_runtime_limits(&connection)?;
        database::configure_database(&connection)?;
        database::migrate(&mut connection, schema_version)?;
        database::verify_database(&connection)?;
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
        storage.verify_footprint()?;
        storage.recover_pack_licenses()?;
        let _registered = storage.registered_packs()?;
        Ok(storage)
    }

    #[must_use]
    pub const fn paths(&self) -> &AppPaths {
        &self.paths
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

    fn verify_footprint(&self) -> Result<(), StorageError> {
        let footprint = self.footprint()?;
        if footprint.main > MAIN_FILE_LIMIT || footprint.sidecars() > TRANSIENT_SIDECAR_LIMIT {
            return Err(StorageError::Full);
        }
        Ok(())
    }
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

#[cfg_attr(
    not(unix),
    expect(
        clippy::unnecessary_wraps,
        reason = "The shared interface is fallible when setting Unix permissions."
    )
)]
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

#[cfg_attr(
    not(unix),
    expect(
        clippy::unnecessary_wraps,
        reason = "The shared interface is fallible when synchronizing Unix directories."
    )
)]
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
