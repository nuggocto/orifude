use super::{
    archive, publication, published,
    support::{self, MAX_BYTES},
};
use flate2::{Compression, GzBuilder};
use serde_json::json;
use std::{
    fs,
    io::{Cursor, Write},
};

#[test]
fn public_downloads_require_complete_bounded_immutable_assets() {
    let names = vec!["archive.tar.gz".to_owned(), "install.sh".to_owned()];
    let valid = json!({"immutable": true, "draft": false, "assets": [
        {"name": "install.sh", "state": "uploaded", "size": 1},
        {"name": "archive.tar.gz", "state": "uploaded", "size": MAX_BYTES},
    ]});
    published::validate_assets(&valid, &names).unwrap();
    let cases = [
        ("/immutable", json!(false), "immutable"),
        ("/draft", json!(true), "immutable"),
        ("/assets/0/size", json!(0), "download limit"),
        ("/assets/0/size", json!(MAX_BYTES + 1), "download limit"),
        ("/assets/0/state", json!("new"), "incomplete"),
        ("/assets/0/name", json!("archive.tar.gz"), "asset names"),
        ("/assets", json!([]), "asset count"),
    ];
    for (pointer, value, reason) in cases {
        let mut changed = valid.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert!(
            published::validate_assets(&changed, &names)
                .unwrap_err()
                .to_string()
                .contains(reason),
            "{pointer}"
        );
    }
}

fn fixture_binary(target: &str) -> Vec<u8> {
    let mut data = vec![0; 128];
    if target.contains("linux") {
        data[..6].copy_from_slice(b"\x7fELF\x02\x01");
        data[18] = if target.starts_with("aarch64") {
            183
        } else {
            62
        };
    } else if target.contains("darwin") {
        data[..4].copy_from_slice(b"\xcf\xfa\xed\xfe");
        data[4..8].copy_from_slice(
            &(if target.starts_with("aarch64") {
                0x0100_000c_u32
            } else {
                0x0100_0007_u32
            })
            .to_le_bytes(),
        );
    } else {
        data[..2].copy_from_slice(b"MZ");
        data[60] = 64;
        data[64..70].copy_from_slice(b"PE\0\0\x64\x86");
    }
    data
}
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let binary = root.path().join("input");
    for target in archive::targets().unwrap() {
        fs::write(&binary, fixture_binary(&target)).unwrap();
        archive::pack(&target, &binary, root.path()).unwrap();
    }
    archive::assemble(root.path()).unwrap();
    root
}
#[test]
fn identical_inputs_repack_identically_and_validate() {
    let root = fixture();
    for target in archive::targets().unwrap() {
        let name = archive::name(&target).unwrap();
        let before = fs::read(root.path().join(&name)).unwrap();
        let binary = root.path().join("input");
        fs::write(&binary, fixture_binary(&target)).unwrap();
        archive::pack(&target, &binary, root.path()).unwrap();
        assert_eq!(
            fs::read(root.path().join(name)).unwrap(),
            before,
            "{target}"
        );
    }
    archive::check(root.path()).unwrap();
}
#[test]
fn missing_and_extra_archives_each_fail_a_complete_matrix() {
    let root = fixture();
    let name = archive::name("x86_64-unknown-linux-musl").unwrap();
    let path = root.path().join(name);
    let bytes = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(
        archive::manifest(root.path())
            .unwrap_err()
            .to_string()
            .contains("matrix")
    );
    fs::write(path, bytes).unwrap();
    archive::check(root.path()).unwrap();
    fs::write(root.path().join("unexpected.zip"), b"unexpected").unwrap();
    assert!(
        archive::manifest(root.path())
            .unwrap_err()
            .to_string()
            .contains("matrix")
    );
}
#[test]
fn altered_manifest_installers_and_package_files_are_rejected() {
    let root = fixture();
    for name in [
        "SHA256SUMS",
        "install.sh",
        "install.ps1",
        "orifude.rb",
        "orifude.json",
        "PKGBUILD",
    ] {
        let path = root.path().join(name);
        let before = fs::read(&path).unwrap();
        fs::write(&path, b"tampered").unwrap();
        assert!(
            archive::check(root.path())
                .unwrap_err()
                .to_string()
                .contains(name)
        );
        fs::write(path, before).unwrap();
    }
}
#[test]
fn wrong_architecture_and_dynamic_linux_interpreters_are_rejected() {
    assert!(
        archive::check_binary(
            &fixture_binary("aarch64-apple-darwin"),
            "x86_64-apple-darwin"
        )
        .unwrap_err()
        .to_string()
        .contains("does not match")
    );
    let mut elf = fixture_binary("x86_64-unknown-linux-musl");
    elf[32] = 64;
    elf[54] = 56;
    elf[56] = 1;
    elf[64] = 3;
    assert!(
        archive::check_binary(&elf, "x86_64-unknown-linux-musl")
            .unwrap_err()
            .to_string()
            .contains("dynamic interpreter")
    );
    elf[54] = 1;
    assert!(
        archive::check_binary(&elf, "x86_64-unknown-linux-musl")
            .unwrap_err()
            .to_string()
            .contains("program headers")
    );
}
#[test]
fn tar_links_duplicates_and_traversal_never_extract() {
    let root = fixture();
    let target = "x86_64-unknown-linux-musl";
    let path = root.path().join(archive::name(target).unwrap());
    for (name, kind, count) in [
        ("../escape".to_owned(), tar::EntryType::Regular, 1),
        (
            format!("{}/orifude", archive::folder(target).unwrap()),
            tar::EntryType::Symlink,
            1,
        ),
        (
            format!("{}/orifude", archive::folder(target).unwrap()),
            tar::EntryType::Regular,
            2,
        ),
    ] {
        let compressed = GzBuilder::new().write(Vec::new(), Compression::fast());
        let mut builder = tar::Builder::new(compressed);
        for _ in 0..count {
            let mut header = tar::Header::new_ustar();
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_size(0);
            header.set_mode(0o755);
            header.set_entry_type(kind);
            header.set_cksum();
            builder.append(&header, Cursor::new([])).unwrap();
        }
        fs::write(&path, builder.into_inner().unwrap().finish().unwrap()).unwrap();
        assert!(
            archive::files(&path, target)
                .unwrap_err()
                .to_string()
                .contains("invalid archive member")
        );
        assert!(!root.path().join("escape").exists());
    }
}
#[test]
fn zip_traversal_and_links_are_rejected() {
    let root = fixture();
    let target = "x86_64-pc-windows-msvc";
    let path = root.path().join(archive::name(target).unwrap());
    for name in [
        "../escape".to_owned(),
        format!("{}/orifude.exe", archive::folder(target).unwrap()),
    ] {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.add_symlink(name, "outside", zip::write::SimpleFileOptions::default())
            .unwrap();
        for member in ["LICENSE", "README.txt"] {
            zip.start_file(
                format!("{}/{member}", archive::folder(target).unwrap()),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        }
        fs::write(&path, zip.finish().unwrap().into_inner()).unwrap();
        assert!(
            archive::files(&path, target)
                .unwrap_err()
                .to_string()
                .contains("invalid archive member")
        );
    }
}
#[test]
fn expansion_is_bounded_before_tar_metadata_parsing() {
    let root = tempfile::tempdir().unwrap();
    let target = "x86_64-unknown-linux-musl";
    let path = root.path().join(archive::name(target).unwrap());
    let mut compressed =
        GzBuilder::new().write(fs::File::create(&path).unwrap(), Compression::fast());
    let zeros = [0; 8192];
    for _ in 0..(MAX_BYTES + 10240) / 8192 + 1 {
        compressed.write_all(&zeros).unwrap();
    }
    compressed.finish().unwrap();
    assert!(
        archive::files(&path, target)
            .unwrap_err()
            .to_string()
            .contains("byte limit")
    );
}
fn candidate() -> serde_json::Value {
    json!({"head_sha": "approved", "conclusion": "success", "path": ".github/workflows/release-candidate.yml", "head_repository": {"full_name": support::REPOSITORY}, "event": "push"})
}
#[test]
fn publication_requires_successful_canonical_candidate_and_ci_at_the_approved_commit() {
    let checks = json!([{"headSha": "approved", "conclusion": "success", "event": "push"}]);
    publication::validate_candidate(&candidate(), &checks, "approved").unwrap();
    for (key, value) in [
        ("head_sha", json!("other")),
        ("conclusion", json!("failure")),
        ("path", json!(".github/workflows/ci.yml")),
        ("head_repository", json!({"full_name": "someone/fork"})),
        ("event", json!("pull_request")),
    ] {
        let mut candidate = candidate();
        candidate[key] = value;
        assert!(
            publication::validate_candidate(&candidate, &checks, "approved")
                .unwrap_err()
                .to_string()
                .contains("candidate run")
        );
    }
    for (key, value) in [
        ("headSha", "other"),
        ("conclusion", "failure"),
        ("event", "pull_request"),
    ] {
        let mut checks = checks.clone();
        checks[0][key] = value.into();
        assert!(
            publication::validate_candidate(&candidate(), &checks, "approved")
                .unwrap_err()
                .to_string()
                .contains("CI did not pass")
        );
    }
}
#[test]
fn publication_requires_immutability_and_a_verified_annotated_tag() {
    let settings = json!({"enabled": true});
    let reference = json!({"object": {"type": "tag"}});
    let annotated = json!({"object": {"type": "commit", "sha": "approved"}, "verification": {"verified": true}});
    publication::validate_tag(&settings, &reference, &annotated, "approved").unwrap();
    assert!(
        publication::validate_tag(
            &json!({"enabled": false}),
            &reference,
            &annotated,
            "approved"
        )
        .unwrap_err()
        .to_string()
        .contains("immutability")
    );
    assert!(
        publication::validate_tag(
            &settings,
            &json!({"object": {"type": "commit"}}),
            &annotated,
            "approved"
        )
        .unwrap_err()
        .to_string()
        .contains("annotated")
    );
    for value in [
        json!({"object": {"type": "commit", "sha": "other"}, "verification": {"verified": true}}),
        json!({"object": {"type": "commit", "sha": "approved"}, "verification": {"verified": false}}),
    ] {
        assert!(
            publication::validate_tag(&settings, &reference, &value, "approved")
                .unwrap_err()
                .to_string()
                .contains("signature or commit")
        );
    }
}

#[test]
fn package_fixture_serves_complete_bytes_after_reading_request_headers() {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    let root = tempfile::tempdir().unwrap();
    let payload = vec![123; 4 * 1024 * 1024];
    fs::write(root.path().join("archive.zip"), &payload).unwrap();
    let server = super::fixture::Http::start(root.path()).unwrap();
    let mut stream = TcpStream::connect(server.base.strip_prefix("http://").unwrap()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .write_all(
            b"GET /archive.zip HTTP/1.1\r\nHost: localhost\r\nUser-Agent: candidate-check\r\n\r\n",
        )
        .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let header_end = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap()
        + 4;
    assert!(response.starts_with(b"HTTP/1.0 200 OK\r\n"));
    assert_eq!(&response[header_end..], payload);
}

#[test]
fn duplicate_zip_catalog_records_are_rejected_before_library_normalization() {
    let root = fixture();
    let target = "x86_64-pc-windows-msvc";
    let path = root.path().join(archive::name(target).unwrap());
    let original = fs::read(&path).unwrap();
    let footer = original.len() - 22;
    let catalog =
        u32::from_le_bytes(original[footer + 16..footer + 20].try_into().unwrap()) as usize;
    let record_size = 46
        + usize::from(u16::from_le_bytes(
            original[catalog + 28..catalog + 30].try_into().unwrap(),
        ));
    let mut bytes = original[..footer].to_vec();
    bytes.extend_from_slice(&original[catalog..catalog + record_size]);
    let new_footer = bytes.len();
    bytes.extend_from_slice(&original[footer..]);
    bytes[new_footer + 8..new_footer + 10].copy_from_slice(&4_u16.to_le_bytes());
    bytes[new_footer + 10..new_footer + 12].copy_from_slice(&4_u16.to_le_bytes());
    bytes[new_footer + 12..new_footer + 16]
        .copy_from_slice(&u32::try_from(new_footer - catalog).unwrap().to_le_bytes());
    for count in [4_u16, 3] {
        bytes[new_footer + 8..new_footer + 10].copy_from_slice(&count.to_le_bytes());
        bytes[new_footer + 10..new_footer + 12].copy_from_slice(&count.to_le_bytes());
        fs::write(&path, &bytes).unwrap();
        let error = archive::files(&path, target)
            .err()
            .expect("duplicate catalog was accepted");
        assert!(error.to_string().contains("ZIP catalog"));
    }
}
