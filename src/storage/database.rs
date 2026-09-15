//! SQLite startup policy, schema validation, and transactional migrations.

use std::{fs, io, path::Path};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, limits::Limit};

use crate::storage::{
    MAX_DATABASE_VALUE_BYTES, MAX_PAGE_COUNT_DB, PAGE_SIZE, PAGE_SIZE_DB, SCHEMA_VERSION,
    StorageError, i64_to_u64,
};

pub(super) fn configure_database(connection: &Connection) -> Result<(), StorageError> {
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

pub(super) fn check_schema_version(connection: &Connection) -> Result<u32, StorageError> {
    let version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSchema {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(version)
}

pub(super) fn verify_database_path(path: &Path) -> Result<(), StorageError> {
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

pub(super) fn configure_runtime_limits(connection: &Connection) -> Result<(), StorageError> {
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
pub(super) fn migrate(connection: &mut Connection, version: u32) -> Result<(), StorageError> {
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

pub(super) fn verify_database(connection: &Connection) -> Result<(), StorageError> {
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
