"""Behavior checks for release integrity; no network or native binaries required."""

import gzip
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

import release


def fixture_binary(target: str) -> bytes:
    data = bytearray(128)
    if "linux" in target:
        data[:6] = b"\x7fELF\x02\x01"
        data[18:20] = (183 if target.startswith("aarch64") else 62).to_bytes(
            2, "little"
        )
    elif "darwin" in target:
        data[:4] = b"\xcf\xfa\xed\xfe"
        data[4:8] = (0x100000C if target.startswith("aarch64") else 0x1000007).to_bytes(
            4, "little"
        )
    else:
        data[:2] = b"MZ"
        data[60:64] = (64).to_bytes(4, "little")
        data[64:70] = b"PE\0\0\x64\x86"
    return bytes(data)


def fixture_set(root: Path) -> None:
    for target in release.targets():
        binary = root / "fixture-binary"
        binary.write_bytes(fixture_binary(target))
        release.pack(target, binary, root)
    binary.unlink()
    release.assemble(root)


class ReleaseIntegrity(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="orifude-release-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        fixture_set(self.root)

    def test_repacking_preserves_bytes_and_validates_complete_set(self) -> None:
        before = {p.name: p.read_bytes() for p in self.root.iterdir()}
        fixture_set(self.root)
        self.assertEqual(before, {p.name: p.read_bytes() for p in self.root.iterdir()})
        release.check(self.root)

    def test_missing_or_extra_target_stops_manifest_generation(self) -> None:
        (self.root / release.archive_name(release.targets()[0])).unlink()
        with self.assertRaisesRegex(ValueError, "matrix"):
            release.assemble(self.root)
        (self.root / "unexpected.zip").write_bytes(b"wrong")
        with self.assertRaisesRegex(ValueError, "matrix"):
            release.assemble(self.root)

    def test_manifest_and_installer_tampering_are_rejected(self) -> None:
        for name in (
            "SHA256SUMS",
            "install.sh",
            "install.ps1",
            "orifude.json",
            "orifude.rb",
            "PKGBUILD",
        ):
            with self.subTest(name=name):
                path = self.root / name
                original = path.read_bytes()
                path.write_bytes(original + b"tampered")
                with self.assertRaisesRegex(
                    ValueError, "generated release file differs"
                ):
                    release.check(self.root)
                path.write_bytes(original)

    def test_wrong_architecture_is_rejected_before_packaging(self) -> None:
        binary = self.root / "wrong-binary"
        binary.write_bytes(fixture_binary("aarch64-unknown-linux-musl"))
        with self.assertRaisesRegex(ValueError, "executable does not match"):
            release.pack("x86_64-unknown-linux-musl", binary, self.root)

    def test_traversal_links_and_duplicate_members_are_rejected(self) -> None:
        target = "x86_64-unknown-linux-musl"
        path = self.root / release.archive_name(target)
        name = f"orifude-{release.version()}-{target}/orifude"
        for kind in ("traversal", "symlink", "duplicate"):
            with self.subTest(kind=kind):
                buffer = io.BytesIO()
                with tarfile.open(fileobj=buffer, mode="w") as archive:
                    entry = tarfile.TarInfo(
                        "../outside" if kind == "traversal" else name
                    )
                    if kind == "symlink":
                        entry.type = tarfile.SYMTYPE
                        entry.linkname = "../../outside"
                    archive.addfile(entry, io.BytesIO())
                    if kind == "duplicate":
                        archive.addfile(entry, io.BytesIO())
                path.write_bytes(gzip.compress(buffer.getvalue()))
                with self.assertRaisesRegex(ValueError, "invalid archive member"):
                    release.archive_files(path, target)
                self.assertFalse((self.root.parent / "outside").exists())


if __name__ == "__main__":
    unittest.main()
