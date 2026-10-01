//! Writes a compact structured starting corpus for each fuzz target.
//!
//! Valid seeds come from the repository's packs and the production replay
//! encoder, so they follow format changes. Each valid seed is checked against
//! its parser before it is written; malformed seeds keep most of a valid
//! structure so mutations start near deep validation paths.

use std::error::Error;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use orifude::domain::paper::MAX_ACTIONS;
use orifude::packs::{
    validate_archive_bytes, validate_directory, validate_metadata_bytes, validate_puzzle_bytes,
};
use orifude::storage::{AppPaths, Storage, decode_replay_bytes};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

type Seeds = Vec<(String, Vec<u8>)>;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let (Some(output), None) = (arguments.next(), arguments.next()) else {
        return Err("usage: fuzz_seeds OUTPUT_DIRECTORY".into());
    };
    write_all(Path::new(&output))
}

fn write_all(output: &Path) -> Result<(), Box<dyn Error>> {
    let catalog = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("puzzles");
    for (target, seeds) in [
        ("domain_actions", domain_actions()),
        ("puzzle_parser", puzzles(&catalog)?),
        ("pack_metadata", metadata(&catalog)?),
        ("replay_parser", replays(&catalog)?),
        ("archive_parser", archives(&catalog.join("example-pack"))?),
    ] {
        let directory = output.join(target);
        fs::create_dir_all(&directory)?;
        for (name, bytes) in seeds {
            fs::write(directory.join(name), bytes)?;
        }
    }
    Ok(())
}

/// Four-byte records: opcodes 0-3 fold, 4 dots, 5-6 draw lines, 7 undoes,
/// and 8 resets.
fn domain_actions() -> Seeds {
    let every_opcode = (0..9_u8).flat_map(|op| [op, op, op + 1, op + 2]).collect();
    let full_length = (0..usize::from(MAX_ACTIONS) * 4)
        .map(|index| u8::try_from(index % 251).expect("the remainder fits a byte"))
        .collect();
    vec![
        ("fold-then-dot".into(), vec![0, 1, 0, 0, 4, 1, 1, 0]),
        ("every-opcode".into(), every_opcode),
        (
            "undo-and-reset".into(),
            vec![1, 0, 0, 0, 7, 0, 0, 0, 2, 2, 0, 0, 8, 0, 0, 0, 5, 0, 0, 3],
        ),
        ("full-length".into(), full_length),
    ]
}

fn puzzles(catalog: &Path) -> Result<Seeds, Box<dyn Error>> {
    let mut seeds = Seeds::new();
    for pack in ["journey", "example-pack"] {
        let mut paths = fs::read_dir(catalog.join(pack).join("puzzles"))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();
        for path in paths {
            let name = path
                .file_stem()
                .ok_or("puzzle file without a name")?
                .to_string_lossy()
                .into_owned();
            // The fuzz target declares this puzzle ID; a mismatch would stop
            // every seed at the identity check.
            let bytes = fs::read_to_string(&path)?
                .replacen(&format!("id = \"{name}\""), "id = \"fuzz-puzzle\"", 1)
                .into_bytes();
            validate_puzzle_bytes("fuzz-pack", "fuzz-puzzle", &bytes)?;
            seeds.push((format!("{pack}-{name}"), bytes));
        }
    }
    let valid = seeds.first().ok_or("the catalog has no puzzles")?.1.clone();
    seeds.extend(malformed_toml(&valid, "width = 4", "width = \"four\"")?);
    Ok(seeds)
}

fn metadata(catalog: &Path) -> Result<Seeds, Box<dyn Error>> {
    let mut seeds = Seeds::new();
    for pack in ["journey", "example-pack"] {
        let bytes = fs::read(catalog.join(pack).join("pack.toml"))?;
        validate_metadata_bytes(&bytes)?;
        seeds.push((pack.into(), bytes));
    }
    let valid = seeds[1].1.clone();
    seeds.extend(malformed_toml(
        &valid,
        "license = \"Apache-2.0\"",
        "license = \"NOT-A-LICENSE\"",
    )?);
    Ok(seeds)
}

/// Encodes the example solutions through the production storage path and
/// reads back the stored replay documents.
fn replays(catalog: &Path) -> Result<Seeds, Box<dyn Error>> {
    let state = tempfile::tempdir()?;
    let paths = AppPaths::injected(
        state.path().join("data"),
        state.path().join("config"),
        state.path().join("cache"),
    );
    let pack = validate_directory(&catalog.join("example-pack"))?;
    let mut storage = Storage::open(paths.clone())?;
    for content in pack.puzzles() {
        let solution = content
            .solution()
            .ok_or("example paper without a solution")?;
        storage.record_completion(content.puzzle(), solution, 1, 0, false)?;
    }
    drop(storage);

    let connection = rusqlite::Connection::open(paths.database())?;
    let mut statement = connection.prepare("SELECT puzzle_id, payload FROM replays ORDER BY id")?;
    let mut seeds = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))?
        .collect::<Result<Seeds, _>>()?;
    for (_, bytes) in &seeds {
        decode_replay_bytes(bytes)?;
    }
    let valid = seeds.first().ok_or("no replay was stored")?.1.clone();
    seeds.extend(malformed_toml(&valid, "width = 4", "width = 99")?);
    Ok(seeds)
}

fn archives(pack: &Path) -> Result<Seeds, Box<dyn Error>> {
    let mut entries = vec![("pack.toml".to_owned(), fs::read(pack.join("pack.toml"))?)];
    let mut puzzles = fs::read_dir(pack.join("puzzles"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    puzzles.sort();
    for path in puzzles {
        let name = path.file_name().ok_or("puzzle file without a name")?;
        entries.push((
            format!("puzzles/{}", name.to_string_lossy()),
            fs::read(&path)?,
        ));
    }

    let stored = zip_bytes(&entries, CompressionMethod::Stored)?;
    let deflated = zip_bytes(&entries, CompressionMethod::Deflated)?;
    validate_archive_bytes(&stored)?;
    validate_archive_bytes(&deflated)?;
    let truncated = deflated[..deflated.len() - 22].to_vec();
    let mut escaping = entries.clone();
    escaping[1].0 = format!("../{}", escaping[1].0);
    let escaping = zip_bytes(&escaping, CompressionMethod::Deflated)?;
    Ok(vec![
        ("stored".into(), stored),
        ("deflated".into(), deflated),
        ("malformed-truncated-directory".into(), truncated),
        ("malformed-escaping-path".into(), escaping),
    ])
}

fn zip_bytes(
    entries: &[(String, Vec<u8>)],
    method: CompressionMethod,
) -> zip::result::ZipResult<Vec<u8>> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .unix_permissions(0o644);
    for (name, contents) in entries {
        writer.start_file(name.as_str(), options)?;
        writer.write_all(contents)?;
    }
    Ok(writer.finish()?.into_inner())
}

/// Returns a truncated copy, a copy with an unknown field, and a copy with one
/// field replaced by an invalid value.
fn malformed_toml(valid: &[u8], field: &str, invalid: &str) -> Result<Seeds, Box<dyn Error>> {
    let text = String::from_utf8_lossy(valid);
    if !text.contains(field) {
        return Err(format!("the valid seed has no `{field}` to replace").into());
    }
    let mut unknown = valid.to_vec();
    unknown.extend_from_slice(b"\nunexpected_field = true\n");
    Ok(vec![
        (
            "malformed-truncated".into(),
            valid[..valid.len() / 2].to_vec(),
        ),
        ("malformed-unknown-field".into(), unknown),
        (
            "malformed-invalid-value".into(),
            text.replacen(field, invalid, 1).into_bytes(),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsers_accept_valid_seeds_and_reject_malformed_ones() {
        let output = tempfile::tempdir().expect("seed directory");
        write_all(output.path()).expect("seeds are written");
        let parsers: [(&str, fn(&[u8]) -> bool); 4] = [
            ("puzzle_parser", |bytes| {
                validate_puzzle_bytes("fuzz-pack", "fuzz-puzzle", bytes).is_ok()
            }),
            ("pack_metadata", |bytes| {
                validate_metadata_bytes(bytes).is_ok()
            }),
            ("replay_parser", |bytes| decode_replay_bytes(bytes).is_ok()),
            ("archive_parser", |bytes| {
                validate_archive_bytes(bytes).is_ok()
            }),
        ];

        for (target, accepts) in parsers {
            let mut valid = 0;
            let mut malformed = 0;
            for entry in fs::read_dir(output.path().join(target)).expect("target seeds") {
                let path = entry.expect("seed entry").path();
                let name = path.file_name().expect("seed name").to_string_lossy();
                let expected = !name.starts_with("malformed-");
                assert_eq!(
                    accepts(&fs::read(&path).expect("seed bytes")),
                    expected,
                    "{target}/{name}"
                );
                if expected {
                    valid += 1;
                } else {
                    malformed += 1;
                }
            }
            assert!(valid >= 2 && malformed >= 2, "{target} lacks variety");
        }
    }
}
