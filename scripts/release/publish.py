"""Publish an approved release set, or inspect the exact proposal with no writes."""

import argparse
import json
from pathlib import Path
import re
import tempfile

import release


def gh(*args: str) -> str:
    return release.run("gh", *args, timeout=120)


def preflight(tag: str, commit: str, run_id: str, *, publishing: bool) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or not run_id.isdecimal():
        raise ValueError("expected a full commit hash and numeric candidate run ID")
    if tag != f"v{release.version()}":
        raise ValueError("tag does not match package version")
    if publishing and int(release.version().split(".")[0]) < 1:
        raise ValueError("development versions must not be published")
    if release.run("git", "status", "--porcelain"):
        raise ValueError("release checkout must be clean")
    if release.run("git", "rev-parse", "HEAD") != commit:
        raise ValueError("checkout differs from approved commit")
    remote = json.loads(gh("api", f"repos/{release.REPOSITORY}/git/ref/heads/shrek"))
    if remote["object"]["sha"] != commit:
        raise ValueError("shrek moved away from the approved commit")
    candidate = json.loads(
        gh("api", f"repos/{release.REPOSITORY}/actions/runs/{run_id}")
    )
    if (
        candidate["head_sha"] != commit
        or candidate["conclusion"] != "success"
        or candidate["path"] != ".github/workflows/release-candidate.yml"
        or candidate["head_repository"]["full_name"] != release.REPOSITORY
        or candidate["event"] not in ("push", "workflow_dispatch")
    ):
        raise ValueError("candidate run did not verify this repository and commit")
    checks = json.loads(
        gh(
            "run",
            "list",
            "--repo",
            release.REPOSITORY,
            "--workflow",
            "ci.yml",
            "--commit",
            commit,
            "--limit",
            "20",
            "--json",
            "conclusion,event,headSha",
        )
    )
    if not any(
        item["conclusion"] == "success"
        and item["event"] in ("push", "workflow_dispatch")
        and item["headSha"] == commit
        for item in checks
    ):
        raise ValueError("ordinary and native CI did not pass this exact commit")
    if publishing:
        settings = json.loads(
            gh("api", f"repos/{release.REPOSITORY}/immutable-releases")
        )
        if not settings["enabled"]:
            raise ValueError("release immutability must be enabled")
        reference = json.loads(
            gh("api", f"repos/{release.REPOSITORY}/git/ref/tags/{tag}")
        )
        if reference["object"]["type"] != "tag":
            raise ValueError("publication requires a signed annotated release tag")
        annotated = json.loads(
            gh(
                "api",
                f"repos/{release.REPOSITORY}/git/tags/{reference['object']['sha']}",
            )
        )
        if (
            annotated["object"]["type"] != "commit"
            or annotated["object"]["sha"] != commit
            or not annotated["verification"]["verified"]
        ):
            raise ValueError("release tag signature or commit is invalid")


def publish(tag: str, commit: str, run_id: str, write: bool) -> None:
    preflight(tag, commit, run_id, publishing=write)
    with tempfile.TemporaryDirectory(prefix="orifude-publication-") as temporary:
        directory = Path(temporary)
        gh(
            "run",
            "download",
            run_id,
            "--repo",
            release.REPOSITORY,
            "--name",
            "release-set",
            "--dir",
            str(directory),
        )
        release.check(directory)
        assets = sorted(
            [
                *(release.archive_name(target) for target in release.targets()),
                "SHA256SUMS",
                "install.sh",
                "install.ps1",
            ]
        )
        for name in assets:
            print(
                f"asset={name} sha256={release.digest(release.bounded_read(directory / name))}"
            )
        if not write:
            print("publication=dry-run; no release, tag, or package repository changed")
            return
        # Repeat identity checks after download and validation, immediately before writes.
        preflight(tag, commit, run_id, publishing=True)
        releases = json.loads(
            gh("api", f"repos/{release.REPOSITORY}/releases?per_page=100")
        )
        if (
            not any(not item["draft"] for item in releases)
            and release.version() != "1.0.0"
        ):
            raise ValueError("the first public puzzle-game release must be v1.0.0")
        if any(item["tag_name"] == tag for item in releases):
            raise ValueError(
                "release already exists; inspect its state before resuming publication"
            )
        notes = (
            f"Orifude {release.version()}\n\n"
            "A quiet, offline folding and ink puzzle game for the terminal.\n\n"
            f"Changelog: https://github.com/{release.REPOSITORY}/blob/{commit}/CHANGELOG.md\n"
        )
        notes_path = directory / "release-notes.txt"
        notes_path.write_text(notes, encoding="utf-8")
        gh(
            "release",
            "create",
            tag,
            "--repo",
            release.REPOSITORY,
            "--verify-tag",
            "--draft",
            "--title",
            f"Orifude {release.version()}",
            "--notes-file",
            str(notes_path),
            *(str(directory / name) for name in assets),
        )
        downloaded = directory / "downloaded"
        gh(
            "release",
            "download",
            tag,
            "--repo",
            release.REPOSITORY,
            "--dir",
            str(downloaded),
        )
        if sorted(path.name for path in downloaded.iterdir()) != assets:
            raise ValueError("draft asset list differs from verified candidate")
        for name in assets:
            if release.bounded_read(downloaded / name) != release.bounded_read(
                directory / name
            ):
                raise ValueError("draft asset bytes differ from verified candidate")
        preflight(tag, commit, run_id, publishing=True)
        gh("release", "edit", tag, "--repo", release.REPOSITORY, "--draft=false")
        gh("release", "verify", tag, "--repo", release.REPOSITORY)
        for name in assets:
            gh(
                "release",
                "verify-asset",
                tag,
                str(downloaded / name),
                "--repo",
                release.REPOSITORY,
            )
        print(
            "publication=verified; package updates can now consume these immutable assets"
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("commit")
    parser.add_argument("run_id")
    parser.add_argument(
        "--publish", action="store_true", help="create and publish the verified draft"
    )
    args = parser.parse_args()
    publish(args.tag, args.commit, args.run_id, args.publish)
