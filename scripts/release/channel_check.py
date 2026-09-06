"""Install release archives through real package managers in disposable fixtures."""

import argparse
import functools
import http.server
import json
from pathlib import Path
import platform
import subprocess
import tempfile
import threading

import release
from install_check import QuietFiles

ARCH_IMAGE = "archlinux:base@sha256:82b1b08faae9d61e3e7e13d562f4d09114d939105b0d59ff34140f3bd418593a"
SCOOP_COMMIT = "b588a06e41d920d2123ec70aee682bae14935939"


def homebrew(directory: Path, root: Path, base: str) -> None:
    name = "orifude-fixture/candidate"
    tap = (
        Path(release.run("brew", "--repository"))
        / "Library/Taps/orifude-fixture/homebrew-candidate"
    )
    if tap.exists():
        raise ValueError("fixture tap already exists")
    release.run("brew", "tap-new", "--no-git", name)
    try:
        template = (directory / "orifude.rb").read_text(encoding="utf-8")
        url = f"https://github.com/{release.REPOSITORY}/releases/download/v{release.version()}"
        formula = tap / "Formula/orifude.rb"
        formula.write_text(template.replace(url, base), encoding="utf-8")
        release.run("brew", "install", "--formula", f"{name}/orifude", timeout=600)
        release.run("brew", "test", f"{name}/orifude", timeout=60)
        # A package revision upgrades metadata while preserving the tested upstream bytes.
        formula.write_text(
            template.replace(url, base).replace(
                '  license "Apache-2.0"', '  revision 1\n  license "Apache-2.0"'
            ),
            encoding="utf-8",
        )
        release.run("brew", "upgrade", f"{name}/orifude", timeout=600)
        release.run("brew", "test", f"{name}/orifude", timeout=60)
    finally:
        subprocess.run(
            ["brew", "uninstall", "--force", f"{name}/orifude"], timeout=120, check=True
        )
        release.run("brew", "untap", name, timeout=60)
    print("homebrew_install_upgrade_test_uninstall=pass")


def scoop(directory: Path, root: Path, base: str) -> None:
    scoop_root = root / "scoop"
    checkout = scoop_root / "apps/scoop/current"
    release.run(
        "git",
        "clone",
        "https://github.com/ScoopInstaller/Scoop.git",
        str(checkout),
        timeout=120,
    )
    release.run("git", "checkout", "--detach", SCOOP_COMMIT, cwd=checkout)
    bucket = root / "bucket-source"
    bucket.mkdir()
    manifest = json.loads((directory / "orifude.json").read_text(encoding="utf-8"))
    manifest["architecture"]["64bit"]["url"] = (
        f"{base}/{release.archive_name('x86_64-pc-windows-msvc')}"
    )
    path = bucket / "orifude.json"
    path.write_text(json.dumps(manifest), encoding="utf-8")
    release.run("git", "init", "--initial-branch=main", cwd=bucket)
    release.run(
        "git",
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "add",
        ".",
        cwd=bucket,
    )
    release.run(
        "git",
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-m",
        "Create package fixture",
        cwd=bucket,
    )
    script = root / "check.ps1"
    script.write_text(
        r"""param($Root, $Bucket)
$ErrorActionPreference = 'Stop'
$env:SCOOP = $Root
$env:SCOOP_GLOBAL = Join-Path $Root 'global'
$env:SCOOP_CONFIG = Join-Path $Root 'config.json'
$env:PATH = (Join-Path $Root 'shims') + ';' + $env:PATH
$Scoop = Join-Path $Root 'apps/scoop/current/bin/scoop.ps1'
& $Scoop config aria2-enabled false
& $Scoop bucket add fixture $Bucket
& $Scoop install fixture/orifude
if ($LASTEXITCODE -ne 0) { throw 'Scoop installation failed.' }
$Binary = Join-Path $Root 'shims/orifude.exe'
& $Binary --version
if ($LASTEXITCODE -ne 0) { throw 'Scoop shim failed.' }
$ManifestPath = Join-Path $Root 'buckets/fixture/orifude.json'
$Manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
$Manifest.version = $Manifest.version + '.1'
$Manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $ManifestPath -Encoding UTF8
& $Scoop update orifude
if ($LASTEXITCODE -ne 0) { throw 'Scoop update failed.' }
$Installed = Get-Content -Raw -LiteralPath (Join-Path $Root 'apps/orifude/current/manifest.json') | ConvertFrom-Json
if ($Installed.version -ne $Manifest.version) { throw 'Scoop did not install the package revision.' }
& $Binary --version
if ($LASTEXITCODE -ne 0) { throw 'Updated Scoop shim failed.' }
& $Scoop uninstall orifude
if (Test-Path -LiteralPath $Binary) { throw 'Scoop uninstall left the shim.' }
""",
        encoding="utf-8",
    )
    release.run(
        "powershell.exe",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        str(script),
        str(scoop_root),
        str(bucket),
        timeout=600,
    )
    print("scoop_install_upgrade_uninstall=pass")


def arch(directory: Path) -> None:
    if platform.machine() not in ("x86_64", "AMD64"):
        # Arch's official container is x86_64. ARM payload execution is checked natively
        # by artifact-check; its package is also assembled in the x86_64 Arch job.
        print(
            "arch_arm_package=covered_by_arch_assembly; arm_payload=covered_by_native_journey"
        )
        return
    command = r"""set -eu
pacman -Sy --noconfirm --needed base-devel >/dev/null
useradd -m builder
mkdir /work
cp /candidate/PKGBUILD /candidate/*.tar.gz /work/
chown -R builder:builder /work
cd /work
runuser -u builder -- makepkg --printsrcinfo > .SRCINFO
runuser -u builder -- makepkg --verifysource --noconfirm
runuser -u builder -- makepkg --noconfirm
pacman -U --noconfirm ./orifude-bin-*-x86_64.pkg.tar.zst
orifude --version
orifude --help >/dev/null
sed -i 's/^pkgrel=1$/pkgrel=2/' PKGBUILD
runuser -u builder -- makepkg --noconfirm
pacman -U --noconfirm ./orifude-bin-*-2-x86_64.pkg.tar.zst
orifude --version
pacman -R --noconfirm orifude-bin
if test -e /usr/bin/orifude; then exit 1; fi
# Packaging ARM prebuilt bytes needs no cross compiler. Execute them in native ARM QA.
runuser -u builder -- env CARCH=aarch64 makepkg --config /etc/makepkg.conf --printsrcinfo > /tmp/arm.SRCINFO
cp /etc/makepkg.conf /work/arm.conf
printf '\nCARCH=aarch64\n' >> /work/arm.conf
runuser -u builder -- makepkg --config /work/arm.conf --noconfirm
printf 'arch_install_upgrade_uninstall=pass\n'
"""
    release.run(
        "docker",
        "run",
        "--rm",
        "--platform",
        "linux/amd64",
        "--cap-drop",
        "NET_RAW",
        "--mount",
        f"type=bind,src={directory},dst=/candidate,readonly",
        ARCH_IMAGE,
        "bash",
        "-c",
        command,
        timeout=900,
    )
    print("arch_install_upgrade_uninstall=pass")


def verify(directory: Path, target: str) -> None:
    release.check(directory, target)
    if "linux" in target:
        arch(directory)
        return
    with tempfile.TemporaryDirectory(prefix="orifude-package-") as temporary:
        root = Path(temporary)
        server = http.server.HTTPServer(
            ("127.0.0.1", 0), functools.partial(QuietFiles, directory=str(directory))
        )
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            base = f"http://127.0.0.1:{server.server_port}"
            if "darwin" in target:
                homebrew(directory, root, base)
            else:
                scoop(directory, root, base)
        finally:
            server.shutdown()
            thread.join(timeout=5)
            server.server_close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("target", choices=release.targets())
    args = parser.parse_args()
    verify(args.directory.resolve(), args.target)
