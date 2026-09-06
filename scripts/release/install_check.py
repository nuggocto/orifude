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
    def do_GET(self) -> None:
        if self.path.startswith("/partial-installer"):
            data = (
                b"Set-Content -LiteralPath $env:ORIFUDE_DOWNLOAD_SENTINEL -Value executed\n"
                if self.path.endswith(".ps1")
                else b'touch "$ORIFUDE_DOWNLOAD_SENTINEL"\n'
            )
            self.send_response(200)
            self.send_header("Content-Length", str(len(data) + 100))
            self.end_headers()
            self.wfile.write(data)
            self.close_connection = True
            return
        super().do_GET()

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
        saved = root / "data/orifude/orifude.sqlite3"
        saved.parent.mkdir(parents=True)
        saved.write_bytes(b"existing player database")
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
        environment["ORIFUDE_FIXTURE_CA"] = str(certificate)
        environment["XDG_DATA_HOME"] = str(root / "data")
        environment["XDG_CONFIG_HOME"] = str(root / "config")
        environment["XDG_CACHE_HOME"] = str(root / "cache")
        environment["LOCALAPPDATA"] = str(root / "data")
        environment["APPDATA"] = str(root / "config")
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
        if windows:
            # Schannel ignores CURL_CA_BUNDLE; pass the private CA explicitly.
            fixture_script = fixture_script.replace(
                "--connect-timeout",
                "--cacert $env:ORIFUDE_FIXTURE_CA --connect-timeout",
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
        try:
            sentinel = root / "executed-partial-installer"
            environment["ORIFUDE_DOWNLOAD_SENTINEL"] = str(sentinel)
            partial = root / name.replace("install", "partial")
            partial_url = (
                f"https://localhost:{server.server_port}/partial-installer"
                + (".ps1" if windows else ".sh")
            )
            if windows:
                wrapper = root / "download.ps1"
                wrapper.write_text(
                    "param($Destination, $Url)\n$ErrorActionPreference = 'Stop'\n"
                    "curl.exe --fail --location --proto '=https' --proto-redir '=https' "
                    "--tlsv1.2 --cacert $env:ORIFUDE_FIXTURE_CA --max-time 10 --output $Destination $Url\n"
                    "if ($LASTEXITCODE -ne 0) { throw 'Installer download failed.' }\n"
                    "& $Destination\n",
                    encoding="utf-8",
                )
                download = [
                    "powershell.exe",
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(wrapper),
                    str(partial),
                    partial_url,
                ]
            else:
                download = [
                    "sh",
                    "-c",
                    "curl --fail --location --proto '=https' "
                    "--proto-redir '=https' --tlsv1.2 --max-time 10 "
                    '--output "$1" "$2" && sh "$1"',
                    "download",
                    str(partial),
                    partial_url,
                ]
            execute(download, environment, success=False)
            if sentinel.exists() or existing.read_bytes() != marker:
                raise RuntimeError("a partial installer was executed")
            if not windows:
                if os.geteuid() == 0:
                    raise RuntimeError(
                        "installer permission checks require an unprivileged account"
                    )
                unsupported = root / "unsupported"
                unsupported.mkdir()
                uname = unsupported / "uname"
                uname.write_text(
                    "#!/bin/sh\nprintf 'Unsupported\\n'\n", encoding="utf-8"
                )
                uname.chmod(0o755)
                unsupported_environment = {
                    **environment,
                    "PATH": str(unsupported) + os.pathsep + environment["PATH"],
                }
                output = execute(command, unsupported_environment, success=False)
                if (
                    "unsupported operating system" not in output
                    or existing.read_bytes() != marker
                ):
                    raise RuntimeError("unsupported platform changed the installation")
                destination.chmod(0o500)
                try:
                    execute(command, environment, success=False)
                    if existing.read_bytes() != marker:
                        raise RuntimeError(
                            "read-only installation changed the executable"
                        )
                finally:
                    destination.chmod(0o700)
            archive.write_bytes(b"tampered archive")
            output = execute(command, environment, success=False)
            if (
                "checksum mismatch" not in output.lower()
                or existing.read_bytes() != marker
            ):
                raise RuntimeError(
                    f"tampered archive did not preserve the previous executable: {output}"
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
            if saved.read_bytes() != b"existing player database":
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


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("target", choices=release.targets())
    args = parser.parse_args()
    verify(args.directory.resolve(), args.target)
