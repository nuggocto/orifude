"""Prepare one package repository update; pushing requires an explicit flag."""

import argparse
import difflib
from pathlib import Path
import tempfile

import release
from publish import gh

CHANNELS = {
    "homebrew": (
        "https://github.com/nuggocto/homebrew-tap.git",
        "shrek",
        "orifude.rb",
        "Formula/orifude.rb",
    ),
    "scoop": (
        "https://github.com/nuggocto/scoop-bucket.git",
        "shrek",
        "orifude.json",
        "bucket/orifude.json",
    ),
    "aur": (
        "ssh://aur@aur.archlinux.org/orifude-bin.git",
        "master",
        "PKGBUILD",
        "PKGBUILD",
    ),
}


def update(directory: Path, channel: str, push: bool) -> None:
    release.check(directory)
    tag = f"v{release.version()}"
    remote, branch, source, destination = CHANNELS[channel]
    if push:
        gh("release", "verify", tag, "--repo", release.REPOSITORY)
        for target in release.targets():
            gh(
                "release",
                "verify-asset",
                tag,
                str(directory / release.archive_name(target)),
                "--repo",
                release.REPOSITORY,
            )
    else:
        print("release_attestation=not_checked; required before push")
    with tempfile.TemporaryDirectory(prefix="orifude-channel-") as temporary:
        checkout = Path(temporary) / "repository"
        release.run(
            "git",
            "clone",
            "--depth",
            "1",
            "--branch",
            branch,
            remote,
            str(checkout),
            timeout=120,
        )
        original = release.run("git", "rev-parse", "HEAD", cwd=checkout)
        changes = {destination: release.bounded_read(directory / source, 1024 * 1024)}
        names = [destination, ".SRCINFO"] if channel == "aur" else [destination]
        before = {}
        for name in names:
            path = checkout / name
            if path.is_symlink() or not path.resolve().is_relative_to(
                checkout.resolve()
            ):
                raise ValueError("package destination must stay inside its checkout")
            before[name] = (
                release.bounded_read(path, 1024 * 1024).decode()
                if path.exists()
                else ""
            )
        if channel == "aur":
            (checkout / "PKGBUILD").write_bytes(changes["PKGBUILD"])
            changes[".SRCINFO"] = (
                release.run("makepkg", "--printsrcinfo", cwd=checkout, timeout=30)
                + "\n"
            ).encode()
        for name, data in changes.items():
            path = checkout / name
            print(
                "".join(
                    difflib.unified_diff(
                        before[name].splitlines(keepends=True),
                        data.decode().splitlines(keepends=True),
                        fromfile=name,
                        tofile=name,
                    )
                )
            )
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        if not push:
            print(f"channel={channel} mode=dry-run")
            return
        if (
            release.run(
                "git", "ls-remote", remote, f"refs/heads/{branch}", timeout=60
            ).split()[0]
            != original
        ):
            raise ValueError("package repository changed during preparation")
        release.run("git", "add", "--", *changes, cwd=checkout)
        if not release.run("git", "diff", "--cached", "--name-only", cwd=checkout):
            print("channel=already-current")
            return
        release.run(
            "git",
            "commit",
            "-m",
            f"Update Orifude to {release.version()}",
            "-m",
            "Use the verified immutable puzzle-game release archives and their exact checksums.",
            cwd=checkout,
        )
        # A normal push refuses concurrent updates; no force and no overwrite retries.
        release.run(
            "git", "push", "origin", f"HEAD:{branch}", cwd=checkout, timeout=120
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("channel", choices=CHANNELS)
    parser.add_argument("--push", action="store_true")
    args = parser.parse_args()
    update(args.directory.resolve(), args.channel, args.push)
