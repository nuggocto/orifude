use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use orifude::domain::paper::{BrushRule, Fold, FoldDirection, LineStroke, PaperAction, StrokeAxis};
use orifude::domain::puzzle::{Puzzle, PuzzleIdentity, PuzzleSpec};
use orifude::domain::replay::Replay;
use orifude::packs::validate_directory;
use orifude::storage::{
    AppPaths, ColorMode, GlyphMode, InstallOutcome, Settings, Storage, StorageError,
    decode_replay_bytes,
};
use rusqlite::{Connection, params};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "orifude-storage-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("test directory should be unique");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn paths(&self) -> AppPaths {
        AppPaths::injected(
            self.path().join("data"),
            self.path().join("config"),
            self.path().join("cache"),
        )
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

fn solved_replay(pack_id: &str) -> (Puzzle, Replay) {
    let identity = PuzzleIdentity::new(pack_id, "berry").unwrap();
    let dimensions = orifude::domain::paper::Dimensions::new(4, 4).unwrap();
    let coordinate = dimensions.coordinate(0, 0).unwrap();
    let target = dimensions.cell_id(coordinate).unwrap();
    let puzzle = Puzzle::new(
        PuzzleSpec::new(identity, 4, 4)
            .with_target_cells(vec![target])
            .with_allowed_brushes(vec![BrushRule::Dot])
            .with_budgets(0, 1),
    )
    .unwrap();
    let mut attempt = puzzle.start();
    attempt.apply(PaperAction::Dot(coordinate)).unwrap();
    let replay = Replay::from_attempt(&attempt);
    (puzzle, replay)
}

fn write_pack(root: &Path, pack_id: &str) {
    fs::create_dir(root.join("puzzles")).unwrap();
    fs::write(
        root.join("pack.toml"),
        format!(
            "format_version = 1\nid = \"{pack_id}\"\ntitle = \"Quiet Grove\"\nauthors = [\"Ada\"]\nlicense = \"Apache-2.0\"\npuzzles = [\"berry\"]\n"
        ),
    )
    .unwrap();
    fs::write(
        root.join("puzzles/berry.toml"),
        "format_version = 1
id = \"berry\"
title = \"Berry\"
width = 4
height = 4
target = [\"#...\", \"....\", \"....\", \"....\"]
folds = []
brushes = [{ kind = \"dot\" }]
fold_budget = 0
stroke_budget = 1
",
    )
    .unwrap();
}

#[test]
fn settings_completion_and_best_replay_survive_restart() {
    let root = TestDirectory::new("restart");
    let paths = root.paths();
    let (puzzle, replay) = solved_replay("built-in");
    {
        let mut storage = Storage::open(paths.clone()).unwrap();
        assert_eq!(storage.settings().unwrap(), Settings::default());
        storage
            .save_settings(Settings {
                color_mode: ColorMode::Monochrome,
                glyph_mode: GlyphMode::Ascii,
                reduced_motion: true,
                instant_reveal: true,
            })
            .unwrap();
        storage
            .record_completion(&puzzle, &replay, 1_700_000_000, 3, true)
            .unwrap();
        storage
            .record_daily(
                orifude::generator::CalendarDate::new(2026, 9, 2).unwrap(),
                1,
                &puzzle,
                true,
            )
            .unwrap();
    }

    let storage = Storage::open(paths).unwrap();
    let settings = storage.settings().unwrap();
    assert_eq!(settings.color_mode, ColorMode::Monochrome);
    assert_eq!(settings.glyph_mode, GlyphMode::Ascii);
    assert!(settings.reduced_motion);
    assert!(settings.instant_reveal);
    let progress = storage.progress("built-in", "berry").unwrap().unwrap();
    assert_eq!(progress.attempt_count, 1);
    let saved = storage.best_replay("built-in", "berry").unwrap().unwrap();
    assert!(
        saved
            .replay()
            .execute(saved.puzzle())
            .unwrap()
            .result()
            .is_success()
    );
    let daily = storage
        .daily_history(
            orifude::generator::CalendarDate::new(2026, 9, 2).unwrap(),
            1,
        )
        .unwrap()
        .unwrap();
    assert!(daily.completed);
    assert_eq!(daily.puzzle_id.as_ref(), "berry");
    assert!(decode_replay_bytes(&vec![0_u8; 64 * 1024 + 1]).is_err());
}

#[test]
fn ownership_schema_corruption_and_database_pragmas_are_enforced() {
    let root = TestDirectory::new("ownership");
    let paths = root.paths();
    let storage = Storage::open(paths.clone()).unwrap();
    assert!(matches!(
        Storage::open(paths.clone()),
        Err(StorageError::Locked)
    ));
    let footprint = storage.footprint().unwrap();
    assert!(footprint.main <= Storage::main_file_limit());
    assert_eq!(footprint.wal, 0);
    assert_eq!(footprint.shared_memory, 0);
    let policy = storage.sqlite_policy().unwrap();
    assert_eq!(policy.page_size, 4096);
    assert_eq!(policy.max_page_count, 32_768);
    assert_eq!(policy.journal_mode.as_ref(), "delete");
    assert!(!policy.cache_spill);
    drop(storage);

    let connection = Connection::open(paths.database()).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode = WAL; PRAGMA user_version = 3")
        .unwrap();
    drop(connection);
    let database_before = fs::read(paths.database()).unwrap();
    assert!(matches!(
        Storage::open(paths.clone()),
        Err(StorageError::UnsupportedSchema {
            found: 3,
            supported: 2
        })
    ));
    assert_eq!(fs::read(paths.database()).unwrap(), database_before);

    let corrupt_root = TestDirectory::new("corrupt");
    let corrupt_paths = corrupt_root.paths();
    fs::create_dir_all(corrupt_paths.data()).unwrap();
    fs::write(corrupt_paths.database(), b"this is not sqlite").unwrap();
    assert!(matches!(
        Storage::open(corrupt_paths),
        Err(StorageError::Corrupt)
    ));
}

#[test]
fn a_database_claiming_the_schema_without_its_markers_is_rejected() {
    for version in [1, 2] {
        let root = TestDirectory::new("false-schema");
        let paths = root.paths();
        fs::create_dir_all(paths.data()).unwrap();
        let connection = Connection::open(paths.database()).unwrap();
        connection
            .pragma_update(None, "user_version", version)
            .unwrap();
        drop(connection);
        let database_before = fs::read(paths.database()).unwrap();
        assert!(matches!(
            Storage::open(paths.clone()),
            Err(StorageError::Corrupt)
        ));
        assert_eq!(fs::read(paths.database()).unwrap(), database_before);
    }
}

#[test]
fn rejected_settings_write_leaves_the_durable_value_unchanged() {
    let root = TestDirectory::new("settings-footprint");
    let paths = root.paths();
    let mut storage = Storage::open(paths.clone()).unwrap();
    let mut shared_memory_name = paths.database().into_os_string();
    shared_memory_name.push("-shm");
    fs::File::create(PathBuf::from(shared_memory_name))
        .unwrap()
        .set_len(Storage::transient_sidecar_limit() + 1)
        .unwrap();

    let changed = Settings {
        color_mode: ColorMode::Monochrome,
        ..Settings::default()
    };
    assert!(matches!(
        storage.save_settings(changed),
        Err(StorageError::Full)
    ));
    assert_eq!(storage.settings().unwrap(), Settings::default());
}

#[test]
fn schema_one_settings_gain_the_unicode_glyph_default() {
    let root = TestDirectory::new("settings-migration");
    let paths = root.paths();
    drop(Storage::open(paths.clone()).unwrap());
    let connection = Connection::open(paths.database()).unwrap();
    connection
        .execute_batch(
            "ALTER TABLE settings DROP COLUMN glyph_mode;
             PRAGMA user_version = 1;",
        )
        .unwrap();
    drop(connection);

    let storage = Storage::open(paths.clone()).unwrap();
    assert_eq!(storage.settings().unwrap().glyph_mode, GlyphMode::Unicode);
    drop(storage);

    let connection = Connection::open(paths.database()).unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 2);
}

#[test]
fn failed_initial_migration_rolls_back_and_retries_without_partial_schema() {
    let root = TestDirectory::new("migration-rollback");
    let paths = root.paths();
    fs::create_dir_all(paths.data()).unwrap();
    let connection = Connection::open(paths.database()).unwrap();
    connection
        .execute_batch("CREATE TABLE settings(existing INTEGER)")
        .unwrap();
    drop(connection);

    assert!(matches!(
        Storage::open(paths.clone()),
        Err(StorageError::Sqlite(_))
    ));
    let connection = Connection::open(paths.database()).unwrap();
    let metadata_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'schema_metadata'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(metadata_count, 0);
    assert_eq!(version, 0);
    drop(connection);
    assert!(matches!(Storage::open(paths), Err(StorageError::Sqlite(_))));
}

#[cfg(unix)]
#[test]
fn read_only_storage_path_has_a_recoverable_typed_error() {
    use std::os::unix::fs::PermissionsExt;

    let root = TestDirectory::new("read-only");
    let paths = root.paths();
    drop(Storage::open(paths.clone()).unwrap());
    fs::set_permissions(paths.lock(), fs::Permissions::from_mode(0o400)).unwrap();
    fs::set_permissions(paths.data(), fs::Permissions::from_mode(0o500)).unwrap();
    let result = Storage::open(paths.clone());
    fs::set_permissions(paths.data(), fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(paths.lock(), fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(result, Err(StorageError::ReadOnly)));
}

#[test]
fn replay_history_is_bounded_without_discarding_the_first_best() {
    let root = TestDirectory::new("pruning");
    let paths = root.paths();
    let (puzzle, replay) = solved_replay("built-in");
    let mut storage = Storage::open(paths.clone()).unwrap();
    for timestamp in 0..25 {
        storage
            .record_completion(&puzzle, &replay, timestamp, 0, false)
            .unwrap();
    }
    assert_eq!(
        storage
            .progress("built-in", "berry")
            .unwrap()
            .unwrap()
            .attempt_count,
        25
    );
    drop(storage);

    let connection = Connection::open(paths.database()).unwrap();
    let replay_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM replays", [], |row| row.get(0))
        .unwrap();
    let best_timestamp: i64 = connection
        .query_row(
            "SELECT created_at FROM replays WHERE is_best = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(replay_count, 20);
    assert_eq!(best_timestamp, 0);
}

#[test]
fn protected_progress_uses_the_reserve_without_retaining_nonessential_history() {
    const MAX_PAGES: i64 = 32_768;
    const RESERVE_PAGES: i64 = 4_096;

    let root = TestDirectory::new("protected-reserve");
    let paths = root.paths();
    drop(Storage::open(paths.clone()).unwrap());

    let mut connection = Connection::open(paths.database()).unwrap();
    connection
        .pragma_update(None, "max_page_count", MAX_PAGES)
        .unwrap();
    connection
        .execute("CREATE TABLE reserve_filler(payload BLOB NOT NULL)", [])
        .unwrap();
    let transaction = connection.transaction().unwrap();
    for _ in 0..128 {
        let page_count: i64 = transaction
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .unwrap();
        let free_pages: i64 = transaction
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))
            .unwrap();
        if MAX_PAGES - (page_count - free_pages) < RESERVE_PAGES {
            break;
        }
        transaction
            .execute("INSERT INTO reserve_filler VALUES (zeroblob(1048576))", [])
            .unwrap();
    }
    let page_count: i64 = transaction
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .unwrap();
    let free_pages: i64 = transaction
        .query_row("PRAGMA freelist_count", [], |row| row.get(0))
        .unwrap();
    assert!(MAX_PAGES - (page_count - free_pages) < RESERVE_PAGES);
    transaction.commit().unwrap();
    drop(connection);

    let (puzzle, replay) = solved_replay("built-in");
    let mut storage = Storage::open(paths.clone()).unwrap();
    storage
        .record_completion(&puzzle, &replay, 1, 0, false)
        .unwrap();
    let progress = storage
        .record_completion(&puzzle, &replay, 2, 0, false)
        .unwrap();
    assert_eq!(progress.attempt_count, 2);
    drop(storage);

    let connection = Connection::open(paths.database()).unwrap();
    let attempt_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM attempts", [], |row| row.get(0))
        .unwrap();
    let replay_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM replays", [], |row| row.get(0))
        .unwrap();
    assert_eq!(attempt_count, 1);
    assert_eq!(replay_count, 1);
}

#[test]
fn a_better_completion_replaces_the_best_atomically() {
    let root = TestDirectory::new("better-best");
    let dimensions = orifude::domain::paper::Dimensions::new(4, 4).unwrap();
    let first = dimensions.coordinate(0, 1).unwrap();
    let second = dimensions.coordinate(0, 2).unwrap();
    let puzzle = Puzzle::new(
        PuzzleSpec::new(
            PuzzleIdentity::new("built-in", "two-berries").unwrap(),
            4,
            4,
        )
        .with_target_cells(vec![
            dimensions.cell_id(first).unwrap(),
            dimensions.cell_id(second).unwrap(),
        ])
        .with_allowed_folds(vec![Fold::new(FoldDirection::Left, 2)])
        .with_allowed_brushes(vec![
            BrushRule::Dot,
            BrushRule::Line {
                axis: StrokeAxis::Horizontal,
                length: 2,
            },
        ])
        .with_budgets(1, 1),
    )
    .unwrap();
    let mut slower = puzzle.start();
    slower
        .apply(PaperAction::Fold(Fold::new(FoldDirection::Left, 2)))
        .unwrap();
    slower.apply(PaperAction::Dot(first)).unwrap();
    assert!(slower.result().is_success());
    let mut faster = puzzle.start();
    faster
        .apply(PaperAction::Line(LineStroke::new(first, second)))
        .unwrap();
    assert!(faster.result().is_success());

    let mut storage = Storage::open(root.paths()).unwrap();
    storage
        .record_completion(&puzzle, &Replay::from_attempt(&slower), 1, 0, false)
        .unwrap();
    let progress = storage
        .record_completion(&puzzle, &Replay::from_attempt(&faster), 2, 0, false)
        .unwrap();
    assert_eq!(progress.best_folds, 0);
    assert_eq!(progress.best_strokes, 1);
    assert_eq!(
        storage
            .best_replay("built-in", "two-berries")
            .unwrap()
            .unwrap()
            .replay()
            .actions(),
        [PaperAction::Line(LineStroke::new(first, second))]
    );
}

#[test]
fn a_failed_progress_write_rolls_back_attempt_replay_and_progress_together() {
    let root = TestDirectory::new("completion-rollback");
    let paths = root.paths();
    drop(Storage::open(paths.clone()).unwrap());
    let connection = Connection::open(paths.database()).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_progress BEFORE INSERT ON progress
             BEGIN SELECT RAISE(ABORT, 'forced progress failure'); END;",
        )
        .unwrap();
    drop(connection);

    let (puzzle, replay) = solved_replay("built-in");
    let mut storage = Storage::open(paths.clone()).unwrap();
    assert!(
        storage
            .record_completion(&puzzle, &replay, 1, 0, false)
            .is_err()
    );
    drop(storage);

    let connection = Connection::open(paths.database()).unwrap();
    for table in ["attempts", "replays", "progress"] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table} must roll back");
    }
}

#[test]
fn installation_load_conflict_removal_and_progress_are_consistent() {
    let root = TestDirectory::new("install");
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    write_pack(&source, "quiet-grove");
    let paths = root.paths();
    let (puzzle, replay) = solved_replay("quiet-grove");
    let mut storage = Storage::open(paths.clone()).unwrap();
    storage
        .record_completion(&puzzle, &replay, 10, 0, false)
        .unwrap();
    let outcome = storage.install_pack(&source, 11).unwrap();
    let installed_name = match outcome {
        InstallOutcome::Installed(pack) => orifude::packs::fingerprint_hex(pack.fingerprint),
        InstallOutcome::AlreadyPresent(_) => panic!("the first installation must be new"),
    };
    assert_eq!(storage.registered_packs().unwrap().len(), 1);
    assert_eq!(
        storage
            .load_pack("quiet-grove")
            .unwrap()
            .unwrap()
            .puzzles()
            .len(),
        1
    );
    assert!(matches!(
        storage.install_pack(&source, 12).unwrap(),
        InstallOutcome::AlreadyPresent(_)
    ));

    let managed_metadata = paths
        .managed_packs()
        .join(&installed_name)
        .join("pack.toml");
    let original_metadata = fs::read(&managed_metadata).unwrap();
    let mut changed = original_metadata.clone();
    changed.push(b'\n');
    fs::write(&managed_metadata, changed).unwrap();
    assert!(matches!(
        storage.install_pack(&source, 12),
        Err(StorageError::PackFingerprint)
    ));
    fs::write(&managed_metadata, original_metadata).unwrap();

    fs::write(
        source.join("pack.toml"),
        fs::read_to_string(source.join("pack.toml"))
            .unwrap()
            .replace("Quiet Grove", "Changed Grove"),
    )
    .unwrap();
    assert!(matches!(
        storage.install_pack(&source, 13),
        Err(StorageError::PackConflict)
    ));
    let mut changed = fs::read_to_string(&managed_metadata).unwrap();
    changed.push('\n');
    fs::write(managed_metadata, changed).unwrap();
    assert!(matches!(
        storage.load_pack("quiet-grove"),
        Err(StorageError::PackFingerprint)
    ));
    assert!(storage.remove_pack("quiet-grove").unwrap());
    assert!(storage.registered_packs().unwrap().is_empty());
    assert!(storage.progress("quiet-grove", "berry").unwrap().is_some());
    drop(storage);
    assert!(Storage::open(paths).is_ok());
}

#[test]
fn replay_reads_reject_unsuccessful_or_mismatched_documents() {
    let root = TestDirectory::new("replay-validation");
    let paths = root.paths();
    let (first_puzzle, first_replay) = solved_replay("built-in");
    let (other_puzzle, other_replay) = solved_replay("other-pack");
    let mut storage = Storage::open(paths.clone()).unwrap();
    storage
        .record_completion(&first_puzzle, &first_replay, 1, 0, false)
        .unwrap();
    storage
        .record_completion(&other_puzzle, &other_replay, 2, 0, false)
        .unwrap();
    drop(storage);

    let connection = Connection::open(paths.database()).unwrap();
    let payload: Vec<u8> = connection
        .query_row(
            "SELECT payload FROM replays WHERE pack_id = 'built-in'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let unsuccessful = String::from_utf8(payload)
        .unwrap()
        .replace("target = [[0, 0]]", "target = [[0, 1]]");
    assert!(decode_replay_bytes(unsuccessful.as_bytes()).is_err());

    let other_replay_id: i64 = connection
        .query_row(
            "SELECT id FROM replays WHERE pack_id = 'other-pack'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    connection
        .execute(
            "UPDATE progress SET best_replay_id = ?1
             WHERE pack_id = 'built-in' AND puzzle_id = 'berry'",
            [other_replay_id],
        )
        .unwrap();
    drop(connection);
    let storage = Storage::open(paths).unwrap();
    assert!(matches!(
        storage.best_replay("built-in", "berry"),
        Err(StorageError::ReplayData)
    ));
}

#[test]
fn restart_reconciles_each_durable_install_state() {
    let root = TestDirectory::new("recovery");
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    write_pack(&source, "quiet-grove");
    let validated = validate_directory(&source).unwrap();
    let fingerprint = validated.fingerprint();
    let final_name = validated.fingerprint_hex();
    let paths = root.paths();
    drop(Storage::open(paths.clone()).unwrap());

    copy_directory(&source, &paths.pack_staging());
    insert_pending(&paths, "quiet-grove", fingerprint, &final_name, 41);
    drop(Storage::open(paths.clone()).unwrap());
    assert!(!paths.pack_staging().exists());
    assert_eq!(pending_count(&paths), 0);

    copy_directory(&source, &paths.pack_staging());
    insert_pending(&paths, "quiet-grove", fingerprint, &final_name, 73);
    fs::rename(
        paths.pack_staging(),
        paths.managed_packs().join(&final_name),
    )
    .unwrap();
    let storage = Storage::open(paths.clone()).unwrap();
    let registered = storage.registered_packs().unwrap();
    assert_eq!(registered.len(), 1);
    assert_eq!(registered[0].installed_at_unix_seconds, 73);
    drop(storage);

    let orphan = paths.pack_staging();
    fs::create_dir(&orphan).unwrap();
    fs::write(orphan.join("orphan"), b"temporary").unwrap();
    drop(Storage::open(paths.clone()).unwrap());
    assert!(!orphan.exists());

    let storage = Storage::open(paths).unwrap();
    assert_eq!(storage.registered_packs().unwrap().len(), 1);
}

#[test]
fn startup_removes_a_registry_row_when_its_managed_directory_is_missing() {
    let root = TestDirectory::new("missing-managed-pack");
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    write_pack(&source, "quiet-grove");
    let paths = root.paths();
    let mut storage = Storage::open(paths.clone()).unwrap();
    let outcome = storage.install_pack(&source, 91).unwrap();
    let fingerprint = match outcome {
        InstallOutcome::Installed(pack) => pack.fingerprint,
        InstallOutcome::AlreadyPresent(_) => panic!("the first installation must be new"),
    };
    drop(storage);
    fs::remove_dir_all(
        paths
            .managed_packs()
            .join(orifude::packs::fingerprint_hex(fingerprint)),
    )
    .unwrap();

    let storage = Storage::open(paths).unwrap();
    assert!(storage.registered_packs().unwrap().is_empty());
}

#[test]
fn startup_cleans_unknown_files_from_managed_pack_storage() {
    let root = TestDirectory::new("unknown-managed-file");
    let paths = root.paths();
    drop(Storage::open(paths.clone()).unwrap());
    let unknown = paths.managed_packs().join(".DS_Store");
    fs::write(&unknown, b"finder metadata").unwrap();

    drop(Storage::open(paths).unwrap());
    assert!(!unknown.exists());
}

#[cfg(unix)]
#[test]
fn startup_rejects_a_symlinked_managed_pack_root_without_following_it() {
    use std::os::unix::fs::symlink;

    let root = TestDirectory::new("managed-root-link");
    let paths = root.paths();
    fs::create_dir_all(paths.data()).unwrap();
    let outside = root.path().join("outside");
    let orphan = outside.join("a".repeat(64));
    fs::create_dir_all(&orphan).unwrap();
    let marker = orphan.join("keep");
    fs::write(&marker, b"outside data").unwrap();
    symlink(&outside, paths.managed_packs()).unwrap();

    let result = Storage::open(paths);
    assert_eq!(fs::read(marker).unwrap(), b"outside data");
    assert!(matches!(result, Err(StorageError::Corrupt)));
}

#[cfg(unix)]
#[test]
fn failed_orphan_cleanup_is_reported_and_retried_on_restart() {
    use std::os::unix::fs::PermissionsExt;

    let root = TestDirectory::new("cleanup-retry");
    let paths = root.paths();
    drop(Storage::open(paths.clone()).unwrap());
    let orphan = paths.managed_packs().join("a".repeat(64));
    fs::create_dir(&orphan).unwrap();
    fs::write(orphan.join("content"), b"orphan").unwrap();
    fs::set_permissions(&orphan, fs::Permissions::from_mode(0o000)).unwrap();
    assert!(matches!(
        Storage::open(paths.clone()),
        Err(StorageError::PackCleanup)
    ));
    fs::set_permissions(&orphan, fs::Permissions::from_mode(0o700)).unwrap();
    drop(Storage::open(paths).unwrap());
    assert!(!orphan.exists());
}

#[test]
fn corrupted_registry_text_is_never_returned_to_a_caller() {
    let root = TestDirectory::new("registry-controls");
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    write_pack(&source, "quiet-grove");
    let paths = root.paths();
    let mut storage = Storage::open(paths.clone()).unwrap();
    storage.install_pack(&source, 1).unwrap();
    drop(storage);

    let connection = Connection::open(paths.database()).unwrap();
    connection
        .execute(
            "UPDATE pack_registry SET title = ?1 WHERE pack_id = 'quiet-grove'",
            ["Quiet\u{1b}[31mGrove"],
        )
        .unwrap();
    drop(connection);
    assert!(matches!(Storage::open(paths), Err(StorageError::Corrupt)));
}

#[test]
fn malicious_archive_install_never_writes_outside_managed_storage() {
    let root = TestDirectory::new("archive-escape");
    let archive_path = root.path().join("untrusted-input");
    let archive = fs::File::create(&archive_path).unwrap();
    let mut writer = ZipWriter::new(archive);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer.start_file("../escaped", options).unwrap();
    writer.write_all(b"outside").unwrap();
    writer.finish().unwrap();

    let paths = root.paths();
    let mut storage = Storage::open(paths.clone()).unwrap();
    assert!(matches!(
        storage.install_pack(&archive_path, 1),
        Err(StorageError::Pack(_))
    ));
    assert!(!root.path().join("escaped").exists());
    assert_eq!(fs::read_dir(paths.managed_packs()).unwrap().count(), 0);
}

fn insert_pending(
    paths: &AppPaths,
    pack_id: &str,
    fingerprint: [u8; 32],
    final_name: &str,
    installed_at: i64,
) {
    let connection = Connection::open(paths.database()).unwrap();
    connection
        .execute(
            "INSERT INTO pending_install(
                 singleton, pack_id, fingerprint, final_name, installed_at
             ) VALUES (1, ?1, ?2, ?3, ?4)",
            params![pack_id, fingerprint.as_slice(), final_name, installed_at],
        )
        .unwrap();
}

fn pending_count(paths: &AppPaths) -> i64 {
    let connection = Connection::open(paths.database()).unwrap();
    connection
        .query_row("SELECT COUNT(*) FROM pending_install", [], |row| row.get(0))
        .unwrap()
}

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination.join("puzzles")).unwrap();
    fs::copy(source.join("pack.toml"), destination.join("pack.toml")).unwrap();
    fs::copy(
        source.join("puzzles/berry.toml"),
        destination.join("puzzles/berry.toml"),
    )
    .unwrap();
}
