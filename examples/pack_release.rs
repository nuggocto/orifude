//! Offline pack checks and deterministic publication archives. This tool is not shipped.

use std::error::Error;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use orifude::packs::{MAX_ARCHIVE_BYTES, MAX_METADATA_BYTES, ValidatedPack, validate_source};
use orifude::solver::{NeverCancel, SolveOutcome, Solver, SolverLimits};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

const MAX_VERSIONS: usize = 128;
type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() {
    if let Err(error) = run() {
        // Paths and input bytes never enter the diagnostic emitted to CI logs.
        eprintln!("Pack release failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).take(5).collect();
    match arguments.as_slice() {
        [command, root] if command == "check" => check_tree(Path::new(root)),
        [command, source, version, output] if command == "build" => build(
            Path::new(source),
            version.to_str().ok_or("invalid version")?,
            Path::new(output),
        ),
        _ => {
            Err("usage: pack_release check ROOT | build SOURCE VERSION NEW_OUTPUT_DIRECTORY".into())
        }
    }
}

fn directory_entries(path: &Path) -> Result<Vec<PathBuf>> {
    if !fs::symlink_metadata(path)?.file_type().is_dir() {
        return Err("catalog entries must be directories, without links".into());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        if entries.len() == MAX_VERSIONS {
            return Err("catalog exceeds 128 entries".into());
        }
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            return Err("catalog entries must be directories, without links".into());
        }
        entries.push(entry.path());
    }
    entries.sort();
    Ok(entries)
}

fn check_tree(root: &Path) -> Result<()> {
    let mut count = 0;
    for pack_directory in directory_entries(root)? {
        let versions = directory_entries(&pack_directory)?;
        if versions.is_empty() {
            return Err("a pack needs at least one version".into());
        }
        for source in versions {
            count += 1;
            if count > MAX_VERSIONS {
                return Err("catalog exceeds 128 pack versions".into());
            }
            let version = source
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("invalid version")?;
            validate_version(version)?;
            let pack = validate_source(&source)?;
            if pack_directory.file_name().and_then(|name| name.to_str())
                != Some(pack.metadata().id())
            {
                return Err("catalog folder must match the pack ID".into());
            }
            solve(&pack)?;
            println!(
                "Verified {} {version}: {} puzzles [{}]",
                pack.metadata().id(),
                pack.puzzles().len(),
                pack.fingerprint_hex()
            );
        }
    }
    if count == 0 {
        return Err("catalog has no pack versions".into());
    }
    Ok(())
}

fn validate_version(version: &str) -> Result<()> {
    let components: Vec<_> = version.split('.').take(4).collect();
    if version.len() > 32
        || components.len() != 3
        || components.iter().any(|part| {
            part.is_empty()
                || (part.len() > 1 && part.starts_with('0'))
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || part.parse::<u32>().is_err()
        })
    {
        return Err("pack version must be three canonical unsigned numbers".into());
    }
    Ok(())
}

fn solve(pack: &ValidatedPack) -> Result<()> {
    for content in pack.puzzles() {
        if !matches!(
            Solver::solve(content.puzzle(), SolverLimits::default(), &NeverCancel),
            SolveOutcome::Solved(_)
        ) {
            return Err(format!(
                "solver could not prove puzzle {} within its limits",
                content.puzzle().identity().puzzle_id()
            )
            .into());
        }
    }
    Ok(())
}

fn build(source: &Path, version: &str, output: &Path) -> Result<()> {
    validate_version(version)?;
    let pack = validate_source(source)?;
    solve(&pack)?;
    let archive = archive(source, &pack)?;
    let digest = sha256(&archive);
    let name = format!("{}-{version}.zip", pack.metadata().id());
    // A new private directory owns the complete proposal; failures remove partial files.
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let staging = tempfile::tempdir_in(parent)?;
    fs::write(staging.path().join(&name), &archive)?;
    fs::write(
        staging.path().join("SHA256SUMS"),
        format!("{digest}  {name}\n"),
    )?;
    let record = serde_json::json!({
        "id": pack.metadata().id(), "version": version,
        "title": pack.metadata().title(), "description": pack.metadata().description(),
        "authors": pack.metadata().authors(), "license": pack.metadata().license(),
        "puzzles": pack.puzzles().len(), "sha256": digest,
        "fingerprint": pack.fingerprint_hex(), "requires": "1.0.0",
    });
    fs::write(
        staging.path().join("pack.json"),
        format!("{}\n", serde_json::to_string_pretty(&record)?),
    )?;
    // Refuse an existing destination, including empty directories and dangling links.
    match fs::symlink_metadata(output) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        _ => return Err("output already exists or cannot be inspected".into()),
    }
    fs::rename(staging.path(), output)?;
    println!("Built {name} [{digest}]");
    Ok(())
}

fn archive(source: &Path, pack: &ValidatedPack) -> Result<Vec<u8>> {
    let mut paths = vec!["pack.toml".to_owned()];
    for id in pack.metadata().puzzle_ids() {
        paths.push(format!("puzzles/{id}.toml"));
        let note = format!("notes/{id}.txt");
        match fs::symlink_metadata(source.join(&note)) {
            Ok(_) => paths.push(note),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    paths.sort();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    for relative in paths {
        let path = source.join(&relative);
        if !fs::symlink_metadata(&path)?.file_type().is_file() {
            return Err("pack changed while preparing its archive".into());
        }
        let mut bytes = Vec::new();
        // The largest permitted individual file is a 64 KiB puzzle.
        File::open(path)?
            .take(MAX_METADATA_BYTES * 2 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_METADATA_BYTES * 2 {
            return Err("pack file exceeds its byte limit".into());
        }
        zip.start_file(relative, options)?;
        zip.write_all(&bytes)?;
    }
    let bytes = zip.finish()?.into_inner();
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err("ZIP exceeds 8 MiB".into());
    }
    let mut fixture = tempfile::NamedTempFile::new()?;
    fixture.write_all(&bytes)?;
    let archived = validate_source(fixture.path())?;
    if archived.fingerprint() != pack.fingerprint() {
        return Err("archive differs from the validated source".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_round_trip_preserves_content_and_is_repeatable() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let source = Path::new("puzzles/example-pack");
        let first = temporary.path().join("first");
        let second = temporary.path().join("second");
        build(source, "1.0.0", &first)?;
        build(source, "1.0.0", &second)?;
        let bytes = fs::read(first.join("paper-garden-1.0.0.zip"))?;
        assert_eq!(bytes, fs::read(second.join("paper-garden-1.0.0.zip"))?);
        assert_eq!(
            validate_source(&first.join("paper-garden-1.0.0.zip"))?.fingerprint(),
            validate_source(source)?.fingerprint()
        );
        assert_eq!(
            fs::read_to_string(first.join("SHA256SUMS"))?,
            format!("{}  paper-garden-1.0.0.zip\n", sha256(&bytes))
        );
        assert!(build(source, "1.0.0", &first).is_err());
        Ok(())
    }

    #[test]
    fn refuses_versions_that_could_change_paths_or_alias_a_release() {
        for version in [
            "",
            "1.0",
            "01.0.0",
            "1.0.0/../../other",
            "1.0.0\n",
            "1.0.0-rc",
            "4294967296.0.0",
        ] {
            assert!(validate_version(version).is_err(), "{version:?}");
        }
    }

    #[test]
    fn unsolvable_pack_cannot_produce_publication_files() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let source = temporary.path().join("pack");
        fs::create_dir(&source)?;
        fs::create_dir(source.join("puzzles"))?;
        fs::write(
            source.join("pack.toml"),
            "format_version = 1\nid = 'unsolved'\ntitle = 'Unsolved'\nlicense = 'Apache-2.0'\npuzzles = ['one']\n",
        )?;
        fs::write(
            source.join("puzzles/one.toml"),
            "format_version = 1\nid = 'one'\ntitle = 'One'\nwidth = 4\nheight = 4\ntarget = ['#...', '...#', '....', '....']\nfolds = []\nbrushes = [{kind = 'dot'}]\nfold_budget = 0\nstroke_budget = 1\n",
        )?;
        let output = temporary.path().join("output");
        let error = build(&source, "1.0.0", &output).unwrap_err();
        assert!(
            error.to_string().contains("solver could not prove"),
            "{error}"
        );
        assert!(!output.exists());
        Ok(())
    }
}

fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(output, "{byte:02x}").expect("string writes cannot fail");
    }
    output
}
