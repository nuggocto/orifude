"""Drive the existing native player journey through the extracted release executable."""

import argparse
import os
from pathlib import Path
import subprocess
import tempfile

import release


def verify(directory: Path, target: str) -> None:
    release.check(directory, target)
    with tempfile.TemporaryDirectory(prefix="orifude-artifact-journey-") as temporary:
        binary = Path(temporary) / release.binary_name(target)
        files = release.archive_files(directory / release.archive_name(target), target)
        binary.write_bytes(files[release.binary_name(target)])
        binary.chmod(0o755)
        environment = os.environ.copy()
        environment["ORIFUDE_ARTIFACT_BINARY"] = str(binary)
        subprocess.run(
            [
                "cargo",
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
            ],
            cwd=release.ROOT,
            env=environment,
            check=True,
            timeout=1200,
        )
    print(f"artifact_journey=pass target={target}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("target", choices=release.targets())
    args = parser.parse_args()
    verify(args.directory.resolve(), args.target)
