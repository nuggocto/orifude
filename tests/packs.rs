use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use orifude::packs::{
    MAX_ARCHIVE_BYTES, MAX_VALIDATION_ISSUES, PackError, validate_archive_bytes,
    validate_directory, validate_metadata_bytes, validate_puzzle_bytes,
};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "orifude-pack-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("test directory should be unique");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

fn metadata(pack_id: &str, title: &str) -> String {
    format!(
        "format_version = 1\nid = \"{pack_id}\"\ntitle = \"{title}\"\ndescription = \"A quiet test pack.\"\nauthors = [\"Ada\"]\nlicense = \"Apache-2.0\"\npuzzles = [\"berry\"]\n"
    )
}

fn puzzle() -> &'static str {
    "format_version = 1
id = \"berry\"
title = \"Winter Berry\"
description = \"Place one mark.\"
width = 4
height = 4
target = [\"#...\", \"....\", \"....\", \"....\"]
folds = []
brushes = [{ kind = \"dot\" }]
fold_budget = 0
stroke_budget = 1
par = { folds = 0, strokes = 1 }
tutorial_cues = [\"Touch the first square.\"]
author = \"Ada\"
license = \"Apache-2.0\"
"
}

fn write_pack(root: &Path, pack_id: &str, title: &str) {
    fs::create_dir(root.join("puzzles")).unwrap();
    fs::write(root.join("pack.toml"), metadata(pack_id, title)).unwrap();
    fs::write(root.join("puzzles/berry.toml"), puzzle()).unwrap();
}

fn zip_bytes(entries: &[(&str, &[u8])], reverse: bool) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    let indices: Box<dyn Iterator<Item = usize>> = if reverse {
        Box::new((0..entries.len()).rev())
    } else {
        Box::new(0..entries.len())
    };
    for index in indices {
        let (name, contents) = entries[index];
        writer.start_file(name, options).unwrap();
        writer.write_all(contents).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn directory_and_archive_preserve_validated_meaning_and_fingerprint() {
    let source = TestDirectory::new("stable-fingerprint");
    write_pack(source.path(), "quiet-grove", "Quiet Grove 🍂");
    let directory = validate_directory(source.path()).unwrap();
    let pack_metadata = metadata("quiet-grove", "Quiet Grove 🍂");
    let entries = [
        ("pack.toml", pack_metadata.as_bytes()),
        ("puzzles/berry.toml", puzzle().as_bytes()),
    ];
    let first = validate_archive_bytes(&zip_bytes(&entries, false)).unwrap();
    let reversed = validate_archive_bytes(&zip_bytes(&entries, true)).unwrap();

    assert_eq!(directory.fingerprint(), first.fingerprint());
    assert_eq!(first.fingerprint(), reversed.fingerprint());
    assert_eq!(first.metadata().title(), "Quiet Grove 🍂");
    assert_eq!(first.puzzles().len(), 1);
    assert_eq!(first.puzzles()[0].puzzle().identity().puzzle_id(), "berry");
}

#[test]
fn display_controls_and_invalid_licenses_are_rejected() {
    let source = TestDirectory::new("controls");
    write_pack(source.path(), "quiet-grove", "Quiet\\u001b[31mGrove");
    let control_error = validate_directory(source.path()).unwrap_err();
    assert!(
        control_error
            .issues()
            .iter()
            .any(|issue| issue.problem().contains("control character"))
    );

    fs::write(
        source.path().join("pack.toml"),
        metadata("quiet-grove", "Quiet Grove").replace("Apache-2.0", "made-up-license"),
    )
    .unwrap();
    let license_error = validate_directory(source.path()).unwrap_err();
    assert!(
        license_error
            .issues()
            .iter()
            .any(|issue| issue.location() == "pack.license")
    );
}

#[test]
fn schemas_reject_unknown_fields_bad_grids_and_unbounded_error_lists() {
    let bad_grid = puzzle().replace(
        "[\"#...\", \"....\", \"....\", \"....\"]",
        "[\"#..\", \"....\", \"....\", \"....\"]",
    );
    assert!(validate_puzzle_bytes("quiet-grove", "berry", bad_grid.as_bytes()).is_err());
    let unknown = puzzle().replace("stroke_budget = 1", "stroke_budget = 1\nsecret = true");
    assert!(validate_puzzle_bytes("quiet-grove", "berry", unknown.as_bytes()).is_err());
    let bad_budget = puzzle().replace("stroke_budget = 1", "stroke_budget = 20");
    assert!(validate_puzzle_bytes("quiet-grove", "berry", bad_budget.as_bytes()).is_err());

    let authors = (0..40).map(|_| "\"\"").collect::<Vec<_>>().join(", ");
    let many_errors = format!(
        "format_version = 1\nid = \"quiet-grove\"\ntitle = \"Quiet\"\nauthors = [{authors}]\nlicense = \"Apache-2.0\"\npuzzles = [\"berry\"]\n"
    );
    let error = validate_metadata_bytes(many_errors.as_bytes()).unwrap_err();
    assert_eq!(error.issues().len(), MAX_VALIDATION_ISSUES);
}

#[test]
fn optional_solution_is_replayed_and_invalid_witnesses_are_rejected() {
    let valid = format!(
        "{}solution = [{{ kind = \"dot\", row = 0, column = 0 }}]\n",
        puzzle()
    );
    let content = validate_puzzle_bytes("quiet-grove", "berry", valid.as_bytes()).unwrap();
    let replay = content.solution().expect("validated solution");
    assert!(
        replay
            .execute(content.puzzle())
            .is_ok_and(|attempt| attempt.result().is_success())
    );

    for invalid in [
        "solution = [{ kind = \"dot\", row = 4, column = 0 }]\n",
        "solution = [{ kind = \"dot\", row = 1, column = 1 }]\n",
    ] {
        let document = format!("{}{invalid}", puzzle());
        let error = validate_puzzle_bytes("quiet-grove", "berry", document.as_bytes()).unwrap_err();
        assert!(
            error
                .issues()
                .iter()
                .any(|issue| issue.location() == "puzzle.solution")
        );
    }
}

#[test]
fn nested_brush_and_solution_tables_reject_unknown_fields() {
    let unknown_solution = format!(
        "{}solution = [{{ kind = \"dot\", row = 0, column = 0, secret = true }}]\n",
        puzzle()
    );
    assert!(validate_puzzle_bytes("quiet-grove", "berry", unknown_solution.as_bytes()).is_err());

    let unknown_brush = puzzle().replace(
        "brushes = [{ kind = \"dot\" }]",
        "brushes = [{ kind = \"dot\", secret = true }]",
    );
    assert!(validate_puzzle_bytes("quiet-grove", "berry", unknown_brush.as_bytes()).is_err());
}

#[test]
fn puzzle_validation_keeps_independent_issues_in_one_report() {
    let invalid = puzzle()
        .replace("id = \"berry\"", "id = \"bad_id\"")
        .replace(
            "title = \"Winter Berry\"",
            "title = \"Winter\\u001b[31mBerry\"",
        )
        .replace(
            "target = [\"#...\", \"....\", \"....\", \"....\"]",
            "target = [\"#..\", \"....\", \"....\", \"....\"]",
        )
        .replace(
            "par = { folds = 0, strokes = 1 }",
            "par = { folds = 99, strokes = 99 }",
        );
    let error = validate_puzzle_bytes("quiet-grove", "berry", invalid.as_bytes()).unwrap_err();
    let locations = error
        .issues()
        .iter()
        .map(orifude::packs::PackIssue::location)
        .collect::<Vec<_>>();

    for expected in ["puzzle.id", "puzzle.title", "puzzle.target", "puzzle.par"] {
        assert!(
            locations.contains(&expected),
            "missing independent issue at {expected}: {locations:?}"
        );
    }
}

#[test]
fn archive_rejects_traversal_links_duplicates_and_resource_excess() {
    let traversal = zip_bytes(&[("../outside", b"bad")], false);
    assert!(validate_archive_bytes(&traversal).is_err());

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink(
            "puzzles/berry.toml",
            "../../outside",
            SimpleFileOptions::default(),
        )
        .unwrap();
    let link = writer.finish().unwrap().into_inner();
    assert!(validate_archive_bytes(&link).is_err());

    let pack_metadata = metadata("quiet-grove", "Quiet Grove");
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer.start_file("pack.toml", options).unwrap();
    writer.write_all(pack_metadata.as_bytes()).unwrap();
    writer.start_file("puzzles/berry.toml", options).unwrap();
    writer.write_all(puzzle().as_bytes()).unwrap();
    writer
        .add_symlink("notes/", "../../outside", options)
        .unwrap();
    let directory_link = writer.finish().unwrap().into_inner();
    assert!(validate_archive_bytes(&directory_link).is_err());

    let duplicate = zip_bytes(
        &[
            ("pack.toml", pack_metadata.as_bytes()),
            ("PACK.toml", pack_metadata.as_bytes()),
        ],
        false,
    );
    assert!(validate_archive_bytes(&duplicate).is_err());

    let oversized = vec![0_u8; MAX_ARCHIVE_BYTES + 1];
    assert!(matches!(
        validate_archive_bytes(&oversized),
        Err(PackError::Invalid { .. })
    ));
}

#[test]
fn archive_rejects_absolute_device_deep_large_and_excess_entry_inputs() {
    for path in [
        "/absolute.toml",
        "notes/con.txt",
        "one/two/three/four/five.toml",
    ] {
        assert!(validate_archive_bytes(&zip_bytes(&[(path, b"data")], false)).is_err());
    }

    let large_puzzle = vec![b' '; 64 * 1024 + 1];
    assert!(
        validate_archive_bytes(&zip_bytes(&[("puzzles/berry.toml", &large_puzzle)], false,))
            .is_err()
    );

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for index in 0..259 {
        writer
            .start_file(format!("notes/n{index}.txt"), options)
            .unwrap();
    }
    let too_many = writer.finish().unwrap().into_inner();
    assert!(validate_archive_bytes(&too_many).is_err());
}

#[test]
fn undeclared_directories_are_rejected_for_both_source_types() {
    let source = TestDirectory::new("undeclared-directory");
    write_pack(source.path(), "quiet-grove", "Quiet Grove");
    fs::create_dir(source.path().join("extras")).unwrap();
    assert!(validate_directory(source.path()).is_err());

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer.add_directory("extras/", options).unwrap();
    let undeclared_directory = writer.finish().unwrap().into_inner();
    assert!(validate_archive_bytes(&undeclared_directory).is_err());
}

#[cfg(unix)]
#[test]
fn directory_rejects_symbolic_links_without_following_them() {
    use std::os::unix::fs::symlink;

    let source = TestDirectory::new("link");
    write_pack(source.path(), "quiet-grove", "Quiet Grove");
    fs::remove_file(source.path().join("puzzles/berry.toml")).unwrap();
    symlink("../../outside", source.path().join("puzzles/berry.toml")).unwrap();
    assert!(validate_directory(source.path()).is_err());
}

#[cfg(unix)]
#[test]
fn directory_rejects_hard_links() {
    let source = TestDirectory::new("hard-link");
    write_pack(source.path(), "quiet-grove", "Quiet Grove");
    let puzzle_path = source.path().join("puzzles/berry.toml");
    fs::hard_link(&puzzle_path, source.path().join("puzzles/copy.toml")).unwrap();
    assert!(validate_directory(source.path()).is_err());
}
