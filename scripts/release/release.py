"""Build and validate the bounded Orifude release set. Uses only Python's stdlib."""

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]
REPOSITORY = "nuggocto/orifude"
MAX_BYTES = 64 * 1024 * 1024
EPOCH = 315532800  # ZIP's first representable date, shared by all archive formats.


def run(*args: str, cwd: Path = ROOT, timeout: int = 1200) -> str:
    environment = os.environ.copy()
    if args[0].lower() == "powershell.exe":
        # A Python child of pwsh otherwise passes incompatible PowerShell 7 modules to 5.1.
        environment = {
            k: v for k, v in environment.items() if k.upper() != "PSMODULEPATH"
        }
    return subprocess.run(
        args,
        cwd=cwd,
        env=environment,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        timeout=timeout,
    ).stdout.strip()


def project() -> dict:
    return tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]


def targets() -> list[str]:
    platforms = project()["metadata"]["orifude"]["platforms"]
    return sorted(
        target for platform in platforms.values() for target in platform["targets"]
    )


def version() -> str:
    value = project()["version"]
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", value):
        raise ValueError("release version must be a complete numeric semantic version")
    return value


def checked_target(target: str) -> str:
    if target not in targets():
        raise ValueError("unsupported release target")
    return target


def archive_name(target: str) -> str:
    suffix = "zip" if "windows" in checked_target(target) else "tar.gz"
    return f"orifude-{version()}-{target}.{suffix}"


def binary_name(target: str) -> str:
    return "orifude.exe" if "windows" in target else "orifude"


def bounded_read(path: Path, limit: int = MAX_BYTES) -> bytes:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > limit:
        raise ValueError(f"not a bounded regular file: {path.name}")
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError("file exceeds release byte limit")
    return data


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def check_binary(data: bytes, target: str) -> None:
    """Reject wrong-format or wrong-architecture executables before distribution."""
    arm = target.startswith("aarch64")
    if "linux" in target:
        valid = (
            len(data) >= 64
            and data[:6] == b"\x7fELF\x02\x01"
            and int.from_bytes(data[18:20], "little") == (183 if arm else 62)
        )
        if valid:
            offset = int.from_bytes(data[32:40], "little")
            size = int.from_bytes(data[54:56], "little")
            count = int.from_bytes(data[56:58], "little")
            valid = count <= 128 and offset + size * count <= len(data)
            if valid and any(
                int.from_bytes(
                    data[offset + size * i : offset + size * i + 4], "little"
                )
                == 3
                for i in range(count)
            ):
                raise ValueError("Linux release binary requires a dynamic interpreter")
    elif "darwin" in target:
        valid = (
            len(data) >= 32
            and data[:4] == b"\xcf\xfa\xed\xfe"
            and int.from_bytes(data[4:8], "little") == (0x100000C if arm else 0x1000007)
        )
    else:
        offset = int.from_bytes(data[60:64], "little")
        valid = (
            len(data) >= 64
            and data[:2] == b"MZ"
            and offset <= len(data) - 24
            and data[offset : offset + 6] == b"PE\0\0\x64\x86"
        )
    if not valid:
        raise ValueError(f"executable does not match {target}")


def readme() -> bytes:
    return (
        f"Orifude {version()}\n\nA quiet, offline folding and ink puzzle game.\n\n"
        "Place the executable in a directory on PATH, then run orifude.\n"
        "Use --help for commands. Enter begins play; ? opens help; q quits.\n"
        "Removing the executable preserves local saved progress.\n\n"
        "License: Apache-2.0. See LICENSE.\n"
        "https://github.com/nuggocto/orifude\n"
    ).encode()


def pack(target: str, binary: Path, output: Path) -> Path:
    checked_target(target)
    data = bounded_read(binary)
    check_binary(data, target)
    files = {
        binary_name(target): data,
        "LICENSE": bounded_read(ROOT / "LICENSE"),
        "README.txt": readme(),
    }
    output.mkdir(parents=True, exist_ok=True)
    destination = output / archive_name(target)
    folder = f"orifude-{version()}-{target}"
    with tempfile.TemporaryDirectory(dir=output) as temporary:
        staged = Path(temporary) / destination.name
        if destination.suffix == ".zip":
            with zipfile.ZipFile(
                staged, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
            ) as archive:
                for name, content in sorted(files.items()):
                    entry = zipfile.ZipInfo(f"{folder}/{name}", (1980, 1, 1, 0, 0, 0))
                    entry.create_system = 3
                    entry.external_attr = (
                        0o100755 if name == binary_name(target) else 0o100644
                    ) << 16
                    archive.writestr(
                        entry,
                        content,
                        compress_type=zipfile.ZIP_DEFLATED,
                        compresslevel=9,
                    )
        else:
            with (
                staged.open("wb") as raw,
                gzip.GzipFile(
                    filename="", mode="wb", fileobj=raw, mtime=EPOCH, compresslevel=9
                ) as compressed,
            ):
                with tarfile.open(
                    fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT
                ) as archive:
                    for name, content in sorted(files.items()):
                        entry = tarfile.TarInfo(f"{folder}/{name}")
                        entry.size = len(content)
                        entry.mode = 0o755 if name == binary_name(target) else 0o644
                        entry.mtime = EPOCH
                        archive.addfile(entry, io.BytesIO(content))
        os.replace(staged, destination)
    return destination


def archive_files(path: Path, target: str) -> dict[str, bytes]:
    data = bounded_read(path)
    expected = {
        f"orifude-{version()}-{target}/{name}"
        for name in (binary_name(target), "LICENSE", "README.txt")
    }
    result = {}
    if path.suffix == ".zip":
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            entries = archive.infolist()
            if len(entries) != 3:
                raise ValueError("archive must contain exactly three regular files")
            for entry in entries:
                if (
                    entry.filename not in expected
                    or entry.filename in result
                    or entry.file_size > MAX_BYTES
                    or entry.is_dir()
                    or entry.external_attr >> 16 & 0o170000 != 0o100000
                ):
                    raise ValueError("invalid archive member")
                with archive.open(entry) as stream:
                    result[entry.filename] = stream.read(MAX_BYTES + 1)
    else:
        with gzip.GzipFile(fileobj=io.BytesIO(data)) as stream:
            expanded = stream.read(MAX_BYTES + 10240 + 1)
        if len(expanded) > MAX_BYTES + 10240:
            raise ValueError("expanded archive exceeds release byte limit")
        with tarfile.open(fileobj=io.BytesIO(expanded), mode="r:") as archive:
            for entry in archive:
                if (
                    entry.name not in expected
                    or entry.name in result
                    or not entry.isfile()
                    or entry.size > MAX_BYTES
                ):
                    raise ValueError("invalid archive member")
                stream = archive.extractfile(entry)
                if stream is None:
                    raise ValueError("missing archive payload")
                with stream:
                    result[entry.name] = stream.read(MAX_BYTES + 1)
    if set(result) != expected or sum(map(len, result.values())) > MAX_BYTES:
        raise ValueError("archive contents exceed the expected layout or byte limit")
    files = {Path(name).name: content for name, content in result.items()}
    check_binary(files[binary_name(target)], target)
    if (
        files["LICENSE"] != bounded_read(ROOT / "LICENSE")
        or files["README.txt"] != readme()
    ):
        raise ValueError("archive documentation differs from the candidate")
    return files


def manifest(directory: Path) -> dict[str, str]:
    expected = {archive_name(target) for target in targets()}
    actual = {
        p.name for p in directory.iterdir() if p.name.endswith((".tar.gz", ".zip"))
    }
    if actual != expected:
        raise ValueError("release archive matrix is incomplete or has unexpected files")
    hashes = {}
    for target in targets():
        path = directory / archive_name(target)
        archive_files(path, target)
        hashes[path.name] = digest(bounded_read(path))
    return dict(sorted(hashes.items()))


def generated(hashes: dict[str, str]) -> dict[str, bytes]:
    values = {
        "VERSION": version(),
        "BASE_URL": f"https://github.com/{REPOSITORY}/releases/download/v{version()}",
    }
    values.update({target: hashes[archive_name(target)] for target in targets()})
    output = {
        "SHA256SUMS": "".join(
            f"{sha}  {name}\n" for name, sha in hashes.items()
        ).encode()
    }
    for filename in ("install.sh", "install.ps1", "orifude.rb", "PKGBUILD"):
        text = (ROOT / "scripts/release" / (filename + ".in")).read_text(
            encoding="utf-8"
        )
        for key, value in values.items():
            text = text.replace(f"@{key}@", value)
        if re.search(r"@[A-Za-z0-9_-]+@", text):
            raise ValueError("unresolved release template value")
        output[filename] = text.encode()
    target = "x86_64-pc-windows-msvc"
    scoop = {
        "version": version(),
        "description": project()["description"],
        "homepage": project()["homepage"],
        "license": "Apache-2.0",
        "architecture": {
            "64bit": {
                "url": f"{values['BASE_URL']}/{archive_name(target)}",
                "hash": hashes[archive_name(target)],
            }
        },
        "extract_dir": f"orifude-{version()}-{target}",
        "bin": "orifude.exe",
    }
    output["orifude.json"] = (json.dumps(scoop, indent=2) + "\n").encode()
    return output


def assemble(directory: Path) -> None:
    for name, data in generated(manifest(directory)).items():
        (directory / name).write_bytes(data)
    print(f"release_set=verified version={version()}")


def check(directory: Path, target: str | None = None) -> None:
    for name, data in generated(manifest(directory)).items():
        if bounded_read(directory / name, 1024 * 1024) != data:
            raise ValueError(f"generated release file differs: {name}")
    if target:
        smoke(directory, target)
    print("release_check=pass")


def smoke(directory: Path, target: str) -> None:
    files = archive_files(directory / archive_name(target), checked_target(target))
    with tempfile.TemporaryDirectory(prefix="orifude-artifact-") as temporary:
        binary = Path(temporary) / binary_name(target)
        binary.write_bytes(files[binary_name(target)])
        binary.chmod(0o755)
        if run(str(binary), "--version", timeout=30) != f"orifude {version()}":
            raise ValueError("packaged binary version does not match release")
        if "orifude" not in run(str(binary), "--help", timeout=30).lower():
            raise ValueError("packaged binary help is missing")
        run(str(binary), "verify", str(ROOT / "puzzles/example-pack"), timeout=30)
    print(f"artifact_smoke=pass target={target}")


def build(target: str, output: Path) -> None:
    checked_target(target)
    environment = os.environ.copy()
    if any(key.startswith("CARGO_PROFILE_RELEASE_") for key in environment):
        raise ValueError("release profile environment overrides are not supported")
    environment.pop("CARGO_ENCODED_RUSTFLAGS", None)
    environment["RUSTFLAGS"] = (
        "-C target-feature=+crt-static"
        if "linux" in target or "windows" in target
        else ""
    )
    if "darwin" in target:
        environment["MACOSX_DEPLOYMENT_TARGET"] = "13.0"
    subprocess.run(
        [
            "cargo",
            "build",
            "--locked",
            "--release",
            "--target",
            target,
            "--target-dir",
            str(ROOT / "target"),
        ],
        cwd=ROOT,
        env=environment,
        check=True,
        timeout=1200,
    )
    pack(target, ROOT / "target" / target / "release" / binary_name(target), output)
    smoke(output, target)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("targets")
    for name in ("build", "pack", "smoke"):
        command = commands.add_parser(name)
        command.add_argument("target", choices=targets())
        if name == "pack":
            command.add_argument("binary", type=Path)
        command.add_argument("directory", type=Path)
    for name in ("assemble", "check"):
        command = commands.add_parser(name)
        command.add_argument("directory", type=Path)
        if name == "check":
            command.add_argument("--target", choices=targets())
    args = parser.parse_args()
    match args.command:
        case "targets":
            print(json.dumps(targets()))
        case "build":
            build(args.target, args.directory)
        case "pack":
            print(pack(args.target, args.binary, args.directory))
        case "assemble":
            assemble(args.directory)
        case "check":
            check(args.directory, args.target)
        case "smoke":
            smoke(args.directory, args.target)


if __name__ == "__main__":
    try:
        main()
    except (
        ValueError,
        OSError,
        subprocess.SubprocessError,
        tarfile.TarError,
        zipfile.BadZipFile,
    ) as error:
        sys.exit(f"error: {error}")
