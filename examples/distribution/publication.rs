use super::{
    archive,
    support::{self, MAX_BYTES, REPOSITORY, Result, gh, require},
};
use serde_json::Value;
use std::{fs, path::Path};

pub fn validate_candidate(candidate: &Value, checks: &Value, commit: &str) -> Result<()> {
    require(
        candidate["head_sha"] == commit
            && candidate["conclusion"] == "success"
            && candidate["path"] == ".github/workflows/release-candidate.yml"
            && candidate["head_repository"]["full_name"] == REPOSITORY
            && matches!(
                candidate["event"].as_str(),
                Some("push" | "workflow_dispatch")
            ),
        "candidate run did not verify this repository and commit",
    )?;
    require(
        checks
            .as_array()
            .ok_or("invalid CI run list")?
            .iter()
            .any(|item| {
                item["conclusion"] == "success"
                    && item["headSha"] == commit
                    && matches!(item["event"].as_str(), Some("push" | "workflow_dispatch"))
            }),
        "ordinary and native CI did not pass this exact commit",
    )
}
pub fn validate_tag(
    settings: &Value,
    reference: &Value,
    annotated: &Value,
    commit: &str,
) -> Result<()> {
    require(
        settings["enabled"] == true,
        "release immutability must be enabled",
    )?;
    require(
        reference["object"]["type"] == "tag",
        "publication requires a signed annotated release tag",
    )?;
    require(
        annotated["object"]["type"] == "commit"
            && annotated["object"]["sha"] == commit
            && annotated["verification"]["verified"] == true,
        "release tag signature or commit is invalid",
    )
}
fn api(path: &str) -> Result<Value> {
    Ok(serde_json::from_str(&gh(&[
        "api",
        &format!("repos/{REPOSITORY}/{path}"),
    ])?)?)
}
fn preflight(tag: &str, commit: &str, run: &str, publishing: bool) -> Result<()> {
    require(
        commit.len() == 40
            && commit
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && !run.is_empty()
            && run.bytes().all(|b| b.is_ascii_digit()),
        "expected a full commit hash and numeric candidate run ID",
    )?;
    require(
        tag == format!("v{}", archive::version()?),
        "tag does not match package version",
    )?;
    require(
        !publishing || !archive::version()?.starts_with("0."),
        "development versions must not be published",
    )?;
    require(
        support::run(support::command("git").args(["status", "--porcelain"]))?.is_empty(),
        "release checkout must be clean",
    )?;
    require(
        support::run(support::command("git").args(["rev-parse", "HEAD"]))? == commit,
        "checkout differs from approved commit",
    )?;
    require(
        api("git/ref/heads/shrek")?["object"]["sha"] == commit,
        "shrek moved away from approved commit",
    )?;
    let checks = serde_json::from_str(&gh(&[
        "run",
        "list",
        "--repo",
        REPOSITORY,
        "--workflow",
        "ci.yml",
        "--commit",
        commit,
        "--limit",
        "20",
        "--json",
        "conclusion,event,headSha",
    ])?)?;
    validate_candidate(&api(&format!("actions/runs/{run}"))?, &checks, commit)?;
    if publishing {
        let settings = api("immutable-releases")?;
        let reference = api(&format!("git/ref/tags/{tag}"))?;
        let sha = reference["object"]["sha"]
            .as_str()
            .ok_or("missing tag object")?;
        validate_tag(
            &settings,
            &reference,
            &api(&format!("git/tags/{sha}"))?,
            commit,
        )?;
    }
    Ok(())
}
pub fn publish(tag: &str, commit: &str, run: &str, write: bool) -> Result<()> {
    preflight(tag, commit, run, write)?;
    let temporary = tempfile::tempdir()?;
    let directory = temporary.path();
    gh(&[
        "run",
        "download",
        run,
        "--repo",
        REPOSITORY,
        "--name",
        "release-set",
        "--dir",
        &directory.to_string_lossy(),
    ])?;
    archive::check(directory)?;
    let assets = archive::public_assets()?;
    for name in &assets {
        println!(
            "asset={name} sha256={}",
            archive::digest(&support::read(&directory.join(name), MAX_BYTES)?)
        );
    }
    if !write {
        println!("publication=dry-run; no release, tag, or package repository changed");
        return Ok(());
    }
    preflight(tag, commit, run, true)?;
    check_release_absent(tag)?;
    let notes = directory.join("release-notes.txt");
    fs::write(
        &notes,
        format!(
            "Orifude {}\n\nA quiet, offline folding and ink puzzle game for the terminal.\n\nChangelog: https://github.com/{REPOSITORY}/blob/{commit}/CHANGELOG.md\n",
            archive::version()?
        ),
    )?;
    let mut create = support::command("gh");
    create
        .args([
            "release",
            "create",
            tag,
            "--repo",
            REPOSITORY,
            "--verify-tag",
            "--draft",
            "--title",
            &format!("Orifude {}", archive::version()?),
            "--notes-file",
        ])
        .arg(notes);
    for name in &assets {
        create.arg(directory.join(name));
    }
    support::run(&mut create)?;
    let downloaded = directory.join("downloaded");
    gh(&[
        "release",
        "download",
        tag,
        "--repo",
        REPOSITORY,
        "--dir",
        &downloaded.to_string_lossy(),
    ])?;
    verify_draft(directory, &downloaded, &assets)?;
    preflight(tag, commit, run, true)?;
    gh(&[
        "release",
        "edit",
        tag,
        "--repo",
        REPOSITORY,
        "--draft=false",
    ])?;
    gh(&["release", "verify", tag, "--repo", REPOSITORY])?;
    for name in &assets {
        gh(&[
            "release",
            "verify-asset",
            tag,
            &downloaded.join(name).to_string_lossy(),
            "--repo",
            REPOSITORY,
        ])?;
    }
    println!("publication=verified; package updates can now consume these immutable assets");
    Ok(())
}
fn check_release_absent(tag: &str) -> Result<()> {
    let releases = api("releases?per_page=100")?;
    let releases = releases.as_array().ok_or("invalid release list")?;
    require(
        releases.iter().any(|item| item["draft"] == false) || archive::version()? == "1.0.0",
        "the first public puzzle-game release must be v1.0.0",
    )?;
    require(
        !releases.iter().any(|item| item["tag_name"] == tag),
        "release already exists; inspect its state before resuming publication",
    )
}
fn verify_draft(directory: &Path, downloaded: &Path, assets: &[String]) -> Result<()> {
    let mut names = fs::read_dir(downloaded)?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<Vec<_>>>()?;
    names.sort();
    require(
        names == assets,
        "draft asset list differs from verified candidate",
    )?;
    for name in assets {
        require(
            support::read(&downloaded.join(name), MAX_BYTES)?
                == support::read(&directory.join(name), MAX_BYTES)?,
            "draft asset bytes differ from verified candidate",
        )?;
    }
    Ok(())
}

pub fn channel(directory: &Path, channel: &str, push: bool) -> Result<()> {
    let (remote, branch, source, destination) = match channel {
        "homebrew" => (
            "https://github.com/nuggocto/homebrew-tap.git",
            "shrek",
            "orifude.rb",
            "Formula/orifude.rb",
        ),
        "scoop" => (
            "https://github.com/nuggocto/scoop-bucket.git",
            "shrek",
            "orifude.json",
            "bucket/orifude.json",
        ),
        "aur" => (
            "ssh://aur@aur.archlinux.org/orifude-bin.git",
            "master",
            "PKGBUILD",
            "PKGBUILD",
        ),
        _ => return Err("unsupported package channel".into()),
    };
    archive::check(directory)?;
    if push {
        let tag = format!("v{}", archive::version()?);
        gh(&["release", "verify", &tag, "--repo", REPOSITORY])?;
        for target in archive::targets()? {
            gh(&[
                "release",
                "verify-asset",
                &tag,
                &directory.join(archive::name(&target)?).to_string_lossy(),
                "--repo",
                REPOSITORY,
            ])?;
        }
    } else {
        println!("release_attestation=not_checked; required before push");
    }
    let temporary = tempfile::tempdir()?;
    let checkout = temporary.path().join("repository");
    support::run(
        support::command("git")
            .args(["clone", "--depth", "1", "--branch", branch, remote])
            .arg(&checkout),
    )?;
    let original = support::run(
        support::command("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&checkout),
    )?;
    let names = if channel == "aur" {
        vec![destination, ".SRCINFO"]
    } else {
        vec![destination]
    };
    for name in &names {
        safe_destination(&checkout, name)?;
    }
    let path = checkout.join(destination);
    fs::create_dir_all(path.parent().ok_or("missing package parent")?)?;
    fs::write(path, support::read(&directory.join(source), 1024 * 1024)?)?;
    if channel == "aur" {
        let srcinfo = support::run(
            support::command("makepkg")
                .arg("--printsrcinfo")
                .current_dir(&checkout),
        )?;
        fs::write(checkout.join(".SRCINFO"), format!("{srcinfo}\n"))?;
    }
    support::run(
        support::command("git")
            .args(["add", "--"])
            .args(&names)
            .current_dir(&checkout),
    )?;
    println!(
        "{}",
        support::run(
            support::command("git")
                .args(["diff", "--cached", "--no-ext-diff"])
                .current_dir(&checkout)
        )?
    );
    if !push {
        println!("channel={channel} mode=dry-run");
        return Ok(());
    }
    push_channel(&checkout, remote, branch, &original)
}
fn push_channel(checkout: &Path, remote: &str, branch: &str, original: &str) -> Result<()> {
    let remote_head = support::run(support::command("git").args([
        "ls-remote",
        remote,
        &format!("refs/heads/{branch}"),
    ]))?;
    require(
        remote_head.split_whitespace().next() == Some(original),
        "package repository changed during preparation",
    )?;
    if support::run(
        support::command("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(checkout),
    )?
    .is_empty()
    {
        println!("channel=already-current");
        return Ok(());
    }
    support::run(support::command("git").args(["commit", "-m", &format!("Update Orifude to {}", archive::version()?), "-m", "Use the verified immutable puzzle-game release archives and their exact checksums."]).current_dir(checkout))?;
    support::run(
        support::command("git")
            .args(["push", "origin", &format!("HEAD:{branch}")])
            .current_dir(checkout),
    )?;
    Ok(())
}
fn safe_destination(checkout: &Path, name: &str) -> Result<()> {
    let mut path = checkout.to_owned();
    for part in Path::new(name).components() {
        require(
            matches!(part, std::path::Component::Normal(_)),
            "invalid package destination",
        )?;
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => require(
                !metadata.file_type().is_symlink(),
                "package destination must stay inside its checkout",
            )?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
