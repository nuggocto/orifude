use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use orifude::storage::{AppPaths, Settings, Storage};
use rusqlite::Connection;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "orifude-storage-hot-journal-{}",
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

#[test]
fn startup_recovers_a_hot_rollback_journal_before_schema_checks() {
    if let Some(database) = std::env::var_os("ORIFUDE_TEST_HOT_JOURNAL_DATABASE") {
        let connection = Connection::open(database).unwrap();
        connection
            .execute_batch(
                "PRAGMA journal_mode = DELETE;
                 PRAGMA synchronous = FULL;
                 PRAGMA cache_size = 10;
                 PRAGMA cache_spill = ON;
                 BEGIN IMMEDIATE;
                 UPDATE settings SET color_mode = 'monochrome' WHERE singleton = 1;
                 INSERT INTO crash_filler VALUES (zeroblob(1048576));",
            )
            .unwrap();
        std::process::exit(86);
    }

    let root = TestDirectory::new();
    let paths = root.paths();
    drop(Storage::open(paths.clone()).unwrap());
    let connection = Connection::open(paths.database()).unwrap();
    connection
        .execute("CREATE TABLE crash_filler(payload BLOB NOT NULL)", [])
        .unwrap();
    drop(connection);

    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "startup_recovers_a_hot_rollback_journal_before_schema_checks",
            "--nocapture",
        ])
        .env("ORIFUDE_TEST_HOT_JOURNAL_DATABASE", paths.database())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86));
    assert!(paths.database().with_extension("sqlite3-journal").exists());

    let storage = Storage::open(paths).unwrap();
    assert_eq!(storage.settings().unwrap(), Settings::default());
}
