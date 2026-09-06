use super::support::{self, MAX_BYTES, REPOSITORY, Result, read, read_stream, require, root};
use flate2::{Compression, GzBuilder, read::GzDecoder};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Write},
    path::Path,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

type Files = BTreeMap<String, Vec<u8>>;
const EPOCH: u32 = 315_532_800;
pub fn project() -> Result<toml::Value> {
    Ok(toml::from_str(&support::text(&root().join("Cargo.toml"))?)?)
}
pub fn version() -> Result<String> {
    let project = project()?;
    let value = project["package"]["version"]
        .as_str()
        .ok_or("missing package version")?;
    require(
        value.split('.').count() == 3
            && value.split('.').all(|part| {
                !part.is_empty()
                    && part.bytes().all(|b| b.is_ascii_digit())
                    && (part.len() == 1 || !part.starts_with('0'))
            }),
        "release version must be a complete numeric semantic version",
    )?;
    Ok(value.to_owned())
}
pub fn targets() -> Result<Vec<String>> {
    let project = project()?;
    let platforms = project["package"]["metadata"]["orifude"]["platforms"]
        .as_table()
        .ok_or("missing release platforms")?;
    let mut targets = Vec::new();
    for platform in platforms.values() {
        for target in platform["targets"]
            .as_array()
            .ok_or("missing platform targets")?
        {
            targets.push(target.as_str().ok_or("invalid release target")?.to_owned());
        }
    }
    targets.sort();
    require(
        targets.len() == 5 && targets.windows(2).all(|pair| pair[0] != pair[1]),
        "expected five unique release targets",
    )?;
    Ok(targets)
}
pub fn checked_target(target: &str) -> Result<()> {
    require(
        targets()?.iter().any(|t| t == target),
        "unsupported release target",
    )
}
pub fn folder(target: &str) -> Result<String> {
    checked_target(target)?;
    Ok(format!("orifude-{}-{target}", version()?))
}
pub fn name(target: &str) -> Result<String> {
    Ok(format!(
        "{}.{}",
        folder(target)?,
        if target.contains("windows") {
            "zip"
        } else {
            "tar.gz"
        }
    ))
}
pub fn binary_name(target: &str) -> &str {
    if target.contains("windows") {
        "orifude.exe"
    } else {
        "orifude"
    }
}
pub fn digest(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(data) {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 15)]));
    }
    output
}
pub fn base_url() -> Result<String> {
    Ok(format!(
        "https://github.com/{REPOSITORY}/releases/download/v{}",
        version()?
    ))
}
fn little(data: &[u8], offset: usize, size: usize) -> Result<u64> {
    let bytes = data
        .get(
            offset
                ..offset
                    .checked_add(size)
                    .ok_or("invalid executable offset")?,
        )
        .ok_or("truncated executable header")?;
    Ok(bytes
        .iter()
        .enumerate()
        .fold(0, |value, (i, byte)| value | (u64::from(*byte) << (i * 8))))
}
pub fn check_binary(data: &[u8], target: &str) -> Result<()> {
    checked_target(target)?;
    let arm = target.starts_with("aarch64");
    let valid = if target.contains("linux") {
        require(
            data.len() >= 64 && data.starts_with(b"\x7fELF\x02\x01"),
            "invalid ELF executable",
        )?;
        let offset = usize::try_from(little(data, 32, 8)?)?;
        let size = usize::try_from(little(data, 54, 2)?)?;
        let count = usize::try_from(little(data, 56, 2)?)?;
        require(
            count <= 128
                && (count == 0 || size >= 56)
                && offset
                    .checked_add(size * count)
                    .is_some_and(|end| end <= data.len()),
            "invalid ELF program headers",
        )?;
        for index in 0..count {
            require(
                little(data, offset + size * index, 4)? != 3,
                "Linux release binary requires a dynamic interpreter",
            )?;
        }
        little(data, 18, 2)? == if arm { 183 } else { 62 }
    } else if target.contains("darwin") {
        data.len() >= 32
            && data.starts_with(b"\xcf\xfa\xed\xfe")
            && little(data, 4, 4)? == if arm { 0x0100_000c } else { 0x0100_0007 }
    } else {
        let offset = usize::try_from(little(data, 60, 4)?)?;
        data.starts_with(b"MZ")
            && data.get(offset..offset.saturating_add(6)) == Some(b"PE\0\0\x64\x86")
    };
    require(valid, &format!("executable does not match {target}"))
}
fn readme() -> Result<Vec<u8>> {
    Ok(format!("Orifude {}\n\nA quiet, offline folding and ink puzzle game.\n\nPlace the executable in a directory on PATH, then run orifude.\nUse --help for commands. Enter begins play; ? opens help; q quits.\nRemoving the executable preserves local saved progress.\n\nLicense: Apache-2.0. See LICENSE.\nhttps://github.com/nuggocto/orifude\n", version()?).into_bytes())
}
pub fn pack(target: &str, binary: &Path, output: &Path) -> Result<()> {
    let data = read(binary, MAX_BYTES)?;
    check_binary(&data, target)?;
    let files = BTreeMap::from([
        (binary_name(target), data),
        ("LICENSE", read(&root().join("LICENSE"), MAX_BYTES)?),
        ("README.txt", readme()?),
    ]);
    require(
        files.values().map(Vec::len).sum::<usize>() as u64 <= MAX_BYTES,
        "archive contents exceed release byte limit",
    )?;
    fs::create_dir_all(output)?;
    let mut staged = tempfile::NamedTempFile::new_in(output)?;
    if target.contains("windows") {
        let mut archive = ZipWriter::new(staged.as_file_mut());
        for (name, data) in files {
            let options = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .compression_level(Some(9))
                .last_modified_time(zip::DateTime::default())
                .unix_permissions(if name == binary_name(target) {
                    0o755
                } else {
                    0o644
                });
            archive.start_file(format!("{}/{name}", folder(target)?), options)?;
            archive.write_all(&data)?;
        }
        archive.finish()?;
    } else {
        let compressed = GzBuilder::new()
            .mtime(EPOCH)
            .operating_system(255)
            .write(staged.as_file_mut(), Compression::best());
        let mut archive = tar::Builder::new(compressed);
        for (name, data) in files {
            let mut entry = tar::Header::new_ustar();
            entry.set_size(data.len() as u64);
            entry.set_mode(if name == binary_name(target) {
                0o755
            } else {
                0o644
            });
            entry.set_mtime(u64::from(EPOCH));
            entry.set_uid(0);
            entry.set_gid(0);
            entry.set_cksum();
            archive.append_data(
                &mut entry,
                format!("{}/{name}", folder(target)?),
                data.as_slice(),
            )?;
        }
        archive.into_inner()?.finish()?;
    }
    require(
        staged.as_file().metadata()?.len() <= MAX_BYTES,
        "compressed archive exceeds release byte limit",
    )?;
    staged.persist(output.join(name(target)?))?;
    Ok(())
}
pub fn files(path: &Path, target: &str) -> Result<Files> {
    let data = read(path, MAX_BYTES)?;
    let expected: BTreeSet<_> = [binary_name(target), "LICENSE", "README.txt"]
        .iter()
        .map(|name| Ok(format!("{}/{name}", folder(target)?)))
        .collect::<Result<_>>()?;
    let mut result = Files::new();
    let mut remaining = MAX_BYTES;
    let mut insert = |name: String, data: Vec<u8>, regular: bool| -> Result<()> {
        require(
            regular && expected.contains(&name) && !result.contains_key(&name),
            "invalid archive member",
        )?;
        remaining = remaining
            .checked_sub(data.len() as u64)
            .ok_or("archive contents exceed release byte limit")?;
        result.insert(name, data);
        Ok(())
    };
    if target.contains("windows") {
        let catalog = zip_catalog(&data, &expected)?;
        let mut archive = ZipArchive::with_config(
            zip::read::Config {
                archive_offset: zip::read::ArchiveOffset::Known(0),
            },
            Cursor::new(data),
        )?;
        require(
            archive.offset() == 0 && archive.central_directory_start() == catalog as u64,
            "invalid ZIP catalog offset",
        )?;
        require(
            archive.len() == 3,
            "archive must contain exactly three regular files",
        )?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_owned();
            let regular = entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170_000 == 0o100_000);
            require(
                expected.contains(&name) && regular && entry.size() <= MAX_BYTES,
                "invalid archive member",
            )?;
            insert(name, read_stream(&mut entry, MAX_BYTES)?, regular)?;
        }
    } else {
        // Bound expansion before tar metadata is interpreted, including extension records.
        let expanded = read_stream(GzDecoder::new(data.as_slice()), MAX_BYTES + 10240)?;
        let mut archive = tar::Archive::new(expanded.as_slice());
        for entry in archive.entries()?.raw(true) {
            let mut entry = entry?;
            let name = std::str::from_utf8(&entry.path_bytes())?.to_owned();
            let regular = entry.header().entry_type().is_file();
            require(
                expected.contains(&name) && regular && entry.size() <= MAX_BYTES,
                "invalid archive member",
            )?;
            insert(name, read_stream(&mut entry, MAX_BYTES)?, regular)?;
        }
    }
    require(
        result.keys().cloned().collect::<BTreeSet<_>>() == expected,
        "archive contents differ from expected layout",
    )?;
    let files: Files = result
        .into_iter()
        .map(|(name, data)| (name.rsplit('/').next().unwrap_or("").to_owned(), data))
        .collect();
    check_binary(&files[binary_name(target)], target)?;
    require(
        files["LICENSE"] == read(&root().join("LICENSE"), MAX_BYTES)?
            && files["README.txt"] == readme()?,
        "archive documentation differs from candidate",
    )?;
    Ok(files)
}

// Our three-file, sub-64-MiB ZIPs need neither ZIP64 nor comments. Check the
// physical catalog before the library deduplicates names or allocates metadata.
fn zip_catalog(data: &[u8], expected: &BTreeSet<String>) -> Result<usize> {
    let footer = data.len().checked_sub(22).ok_or("truncated ZIP catalog")?;
    require(
        data[footer..].starts_with(b"PK\x05\x06")
            && little(data, footer + 4, 4)? == 0
            && little(data, footer + 8, 2)? == 3
            && little(data, footer + 10, 2)? == 3
            && little(data, footer + 20, 2)? == 0,
        "ZIP catalog must contain exactly three files without comments or disks",
    )?;
    let start = usize::try_from(little(data, footer + 16, 4)?)?;
    let size = usize::try_from(little(data, footer + 12, 4)?)?;
    require(
        start.checked_add(size) == Some(footer),
        "invalid ZIP catalog extent",
    )?;
    let mut offset = start;
    let mut seen = BTreeSet::new();
    for _ in 0..3 {
        let header = data
            .get(offset..offset.saturating_add(46))
            .ok_or("truncated ZIP catalog record")?;
        require(
            header.starts_with(b"PK\x01\x02")
                && little(header, 30, 2)? == 0
                && little(header, 32, 2)? == 0
                && little(header, 34, 2)? == 0,
            "invalid ZIP catalog extensions",
        )?;
        let name_length = usize::try_from(little(header, 28, 2)?)?;
        let name_start = offset + 46;
        offset = offset
            .checked_add(46 + name_length)
            .ok_or("invalid ZIP catalog record length")?;
        require(offset <= footer, "ZIP catalog record exceeds its extent")?;
        let name = std::str::from_utf8(&data[name_start..offset])?;
        require(
            expected.contains(name) && seen.insert(name),
            "invalid archive member in ZIP catalog",
        )?;
    }
    require(offset == footer, "ZIP catalog contains extra records")?;
    Ok(start)
}
pub fn manifest(directory: &Path) -> Result<BTreeMap<String, String>> {
    let targets = targets()?;
    let expected: BTreeSet<_> = targets.iter().map(|t| name(t)).collect::<Result<_>>()?;
    let actual: BTreeSet<_> = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|name| {
            let name = name.to_ascii_lowercase();
            name.ends_with(".tar.gz")
                || Path::new(&name).extension().is_some_and(|ext| ext == "zip")
        })
        .collect();
    require(
        actual == expected,
        "release archive matrix is incomplete or has unexpected files",
    )?;
    let mut hashes = BTreeMap::new();
    for target in targets {
        let name = name(&target)?;
        files(&directory.join(&name), &target)?;
        hashes.insert(
            name.clone(),
            digest(&read(&directory.join(name), MAX_BYTES)?),
        );
    }
    Ok(hashes)
}
pub fn generated(hashes: &BTreeMap<String, String>) -> Result<Files> {
    let mut values = BTreeMap::from([
        ("VERSION".to_owned(), version()?),
        ("BASE_URL".to_owned(), base_url()?),
    ]);
    for target in targets()? {
        values.insert(target.clone(), hashes[&name(&target)?].clone());
    }
    let mut output = Files::new();
    let mut checksums = Vec::new();
    for (name, sha) in hashes {
        writeln!(checksums, "{sha}  {name}")?;
    }
    output.insert("SHA256SUMS".to_owned(), checksums);
    for name in ["install.sh", "install.ps1", "orifude.rb", "PKGBUILD"] {
        let mut text = support::text(&root().join(format!("scripts/release/{name}.in")))?;
        for (key, value) in &values {
            text = text.replace(&format!("@{key}@"), value);
        }
        require(
            !text.split('@').skip(1).step_by(2).any(|s| {
                !s.is_empty()
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            }),
            "unresolved release template value",
        )?;
        output.insert(name.to_owned(), text.into_bytes());
    }
    let target = "x86_64-pc-windows-msvc";
    let project = project()?;
    let scoop = json!({"version": version()?, "description": project["package"]["description"].as_str(), "homepage": project["package"]["homepage"].as_str(), "license": "Apache-2.0", "architecture": {"64bit": {"url": format!("{}/{}", base_url()?, name(target)?), "hash": hashes[&name(target)?]}}, "extract_dir": folder(target)?, "bin": "orifude.exe"});
    output.insert(
        "orifude.json".to_owned(),
        format!("{}\n", serde_json::to_string_pretty(&scoop)?).into_bytes(),
    );
    Ok(output)
}
pub fn assemble(directory: &Path) -> Result<()> {
    for (name, data) in generated(&manifest(directory)?)? {
        fs::write(directory.join(name), data)?;
    }
    println!("release_set=verified version={}", version()?);
    Ok(())
}
pub fn check(directory: &Path) -> Result<()> {
    for (name, data) in generated(&manifest(directory)?)? {
        require(
            read(&directory.join(&name), 1024 * 1024)? == data,
            &format!("generated release file differs: {name}"),
        )?;
    }
    println!("release_check=pass");
    Ok(())
}
pub fn extract(directory: &Path, target: &str, destination: &Path) -> Result<()> {
    let files = files(&directory.join(name(target)?), target)?;
    fs::write(destination, &files[binary_name(target)])?;
    support::executable(destination)
}
pub fn smoke(directory: &Path, target: &str) -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let binary = temporary.path().join(binary_name(target));
    extract(directory, target, &binary)?;
    require(
        support::run(support::command(&binary).arg("--version"))?
            == format!("orifude {}", version()?),
        "packaged binary version does not match release",
    )?;
    require(
        support::run(support::command(&binary).arg("--help"))?
            .to_lowercase()
            .contains("orifude"),
        "packaged binary help is missing",
    )?;
    support::run(
        support::command(&binary)
            .arg("verify")
            .arg(root().join("puzzles/example-pack")),
    )?;
    println!("artifact_smoke=pass target={target}");
    Ok(())
}
pub fn build(target: &str, output: &Path) -> Result<()> {
    checked_target(target)?;
    require(
        !std::env::vars_os()
            .any(|(key, _)| key.to_string_lossy().starts_with("CARGO_PROFILE_RELEASE_")),
        "release profile environment overrides are not supported",
    )?;
    let mut command = support::command("cargo");
    command
        .args([
            "build",
            "--locked",
            "--release",
            "--bin",
            "orifude",
            "--target",
            target,
            "--target-dir",
        ])
        .arg(root().join("target"));
    command.env_remove("CARGO_ENCODED_RUSTFLAGS").env(
        "RUSTFLAGS",
        if target.contains("linux") || target.contains("windows") {
            "-C target-feature=+crt-static"
        } else {
            ""
        },
    );
    if target.contains("darwin") {
        command.env("MACOSX_DEPLOYMENT_TARGET", "13.0");
    }
    support::run(&mut command)?;
    pack(
        target,
        &root()
            .join("target")
            .join(target)
            .join("release")
            .join(binary_name(target)),
        output,
    )?;
    smoke(output, target)
}
pub fn journey(directory: &Path, target: &str) -> Result<()> {
    check(directory)?;
    smoke(directory, target)?;
    let temporary = tempfile::tempdir()?;
    let binary = temporary.path().join(binary_name(target));
    extract(directory, target, &binary)?;
    let out = support::run(
        support::command("cargo")
            .args([
                "test",
                "--locked",
                "--release",
                "--features",
                "isolated-test-paths",
                "--test",
                "terminal_pty",
                "packaged_binary_preserves_the_complete_player_journey",
                "--",
                "--ignored",
                "--exact",
            ])
            .env("ORIFUDE_ARTIFACT_BINARY", binary),
    )?;
    println!("{out}\nartifact_journey=pass target={target}");
    Ok(())
}
