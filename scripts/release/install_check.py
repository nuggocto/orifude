"""Exercise generated installers against a private HTTPS fixture release."""

import argparse
import functools
import http.server
import os
from pathlib import Path
import shutil
import ssl
import subprocess
import tempfile
import threading

import release
from test_release import fixture_set


def execute(
    command: list[str], environment: dict[str, str], success: bool = True
) -> str:
    result = subprocess.run(
        command,
        env=environment,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=180,
    )
    if (result.returncode == 0) != success:
        raise RuntimeError(f"installer returned {result.returncode}: {result.stdout}")
    return result.stdout


class QuietFiles(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_args: object) -> None:
        pass


def verify(directory: Path, target: str) -> None:
    windows = "windows" in target
    with tempfile.TemporaryDirectory(prefix="orifude-installer-") as temporary:
        root = Path(temporary)
        fixture = root / "release"
        fixture.mkdir()
        fixture_set(fixture)
        archive = fixture / release.archive_name(target)
        shutil.copyfile(directory / archive.name, archive)
        release.assemble(fixture)
        original_archive = archive.read_bytes()
        destination = root / "bin with spaces"
        destination.mkdir()
        existing = destination / release.binary_name(target)
        marker = b"previous executable must survive failure"
        existing.write_bytes(marker)
        (root / "saved-progress").write_bytes(b"player state")
        certificate = root / "localhost.pem"
        key = root / "localhost.key"
        release.run(
            "openssl",
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost",
            "-keyout",
            str(key),
            "-out",
            str(certificate),
            timeout=30,
        )
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(certificate, key)
        server = http.server.HTTPServer(
            ("127.0.0.1", 0), functools.partial(QuietFiles, directory=str(fixture))
        )
        server.socket = context.wrap_socket(server.socket, server_side=True)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        environment = os.environ.copy()
        environment["CURL_CA_BUNDLE"] = str(certificate)
        environment["TMPDIR"] = str(root)
        environment["TEMP"] = str(root)
        environment["TMP"] = str(root)
        name = "install.ps1" if windows else "install.sh"
        original_script = (fixture / name).read_text(encoding="utf-8")
        url = f"https://github.com/{release.REPOSITORY}/releases/download/v{release.version()}"
        # Only the private copy points at the fixture. Published scripts have no URL override.
        script = root / name
        fixture_script = original_script.replace(
            url, f"https://localhost:{server.server_port}"
        )
        script.write_text(fixture_script, encoding="utf-8")
        command = (
            [
                "powershell.exe",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                str(script),
                "-BinDir",
                str(destination),
            ]
            if windows
            else ["sh", str(script), "--bin-dir", str(destination)]
        )
        trust_script = root / "trust.ps1"
        trust_script.write_text(
            "param($Certificate, $Thumbprint)\n$ErrorActionPreference = 'Stop'\n"
            "if ($Certificate) { (Import-Certificate -FilePath $Certificate "
            "-CertStoreLocation Cert:\\CurrentUser\\Root).Thumbprint }\n"
            "else { Remove-Item -LiteralPath ('Cert:\\CurrentUser\\Root\\' + $Thumbprint) }\n",
            encoding="utf-8",
        )
        thumbprint = None
        try:
            if windows:
                # Windows curl uses Schannel. Trust only this fixture certificate for this test.
                thumbprint = release.run(
                    "powershell.exe",
                    "-NoProfile",
                    "-File",
                    str(trust_script),
                    "-Certificate",
                    str(certificate),
                    timeout=30,
                )
            archive.write_bytes(b"tampered archive")
            output = execute(command, environment, success=False)
            if (
                "checksum mismatch" not in output.lower()
                or existing.read_bytes() != marker
            ):
                raise RuntimeError(
                    "tampered archive did not preserve the previous executable"
                )
            archive.unlink()
            output = execute(command, environment, success=False)
            if (
                "download failed" not in output.lower()
                or existing.read_bytes() != marker
            ):
                raise RuntimeError("failed download changed the destination")
            archive.write_bytes(original_archive[:128])
            execute(command, environment, success=False)
            if existing.read_bytes() != marker:
                raise RuntimeError("truncated archive replaced the executable")
            archive.write_bytes(original_archive)
            expected = release.digest(original_archive)
            script.write_text(
                fixture_script.replace(expected, "0" * 64), encoding="utf-8"
            )
            execute(command, environment, success=False)
            if existing.read_bytes() != marker:
                raise RuntimeError("embedded hash mismatch replaced the executable")
            script.write_text(fixture_script, encoding="utf-8")
            existing.unlink()
            existing.mkdir()
            execute(command, environment, success=False)
            existing.rmdir()
            execute(command, environment)
            if (
                release.run(str(existing), "--version", timeout=30)
                != f"orifude {release.version()}"
            ):
                raise RuntimeError("installed version differs")
            before = existing.read_bytes()
            execute(
                command, environment
            )  # Replacing a previously working installation.
            if existing.read_bytes() != before:
                raise RuntimeError("reinstall changed verified bytes")
            existing.unlink()
            execute(command, environment)
            if (root / "saved-progress").read_bytes() != b"player state":
                raise RuntimeError("installation changed unrelated saved data")
            if list(destination.glob(".orifude-install-*")) or list(
                root.glob("orifude-install*")
            ):
                raise RuntimeError("installer left temporary state")
            print(f"installer_check=pass target={target}")
        finally:
            server.shutdown()
            thread.join(timeout=5)
            server.server_close()
            if thumbprint:
                release.run(
                    "powershell.exe",
                    "-NoProfile",
                    "-File",
                    str(trust_script),
                    "-Thumbprint",
                    thumbprint,
                    timeout=30,
                )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("target", choices=release.targets())
    args = parser.parse_args()
    verify(args.directory.resolve(), args.target)
