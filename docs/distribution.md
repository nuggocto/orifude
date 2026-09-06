# Building and distributing Orifude

The game needs no network connection or interpreter. Release tooling uses the
pinned Rust compiler and Python 3.14.7 through mise. Python has no third-party
runtime dependencies here. The artifact matrix comes from the platform metadata
in [Cargo.toml](../Cargo.toml).

## Candidate archives

Run each target on its matching native host:

```sh
mise run release-build -- x86_64-unknown-linux-musl target/release-dist
```

Linux needs musl headers and a C compiler for bundled SQLite. Use `musl-gcc` for
`CC_x86_64_unknown_linux_musl` or `CC_aarch64_unknown_linux_musl` as appropriate.
Rust's final linker can remain the native `cc`; the build enables static CRT
linking. The archive checker rejects Linux executables with a dynamic interpreter.
Windows also links the CRT statically. macOS uses a 13.0 deployment target.
Release arithmetic checks and panic unwinding remain enabled.

Collect the five archives from the same commit, then run:

```sh
mise run release-assemble -- target/release-dist
mise run release-verify -- target/release-dist
mise run installer-check -- target/release-dist x86_64-unknown-linux-musl
mise run artifact-check -- target/release-dist x86_64-unknown-linux-musl
mise run package-channel-check -- target/release-dist x86_64-unknown-linux-musl
```

Use the native target in the last three commands. The artifact journey uses an
extracted default-feature binary. It refuses existing player directories and
removes only the new state it created. On macOS and Windows use a disposable
account because platform directory APIs are not reliably redirected by environment
variables. Do not run package-channel checks on an ordinary player account.

The candidate workflow builds and tests every target without publication secrets.
It preserves `release-set` as a workflow artifact. The archive tests cover order,
metadata, checksums, unexpected members, links, traversal, and architecture.
Repacking identical inputs is deterministic. This alone does not establish that
independent compilers or different operating-system images produce identical
executable bytes.

The installer tests render a private copy pointing at a local HTTPS fixture.
Published scripts have fixed GitHub release URLs and no fixture URL override.
Windows temporarily trusts the fixture certificate in the test account and removes
it afterward. Package-manager fixtures use local archive URLs. Homebrew and Scoop
upgrade tests use a package metadata revision with the same verified executable.
The Arch job assembles both architectures and installs x86_64 natively; the ARM
executable has its own native Linux player journey.

## Installing an exact published version

These commands apply after the named release exists. The first puzzle-game public
release is `v1.0.0`; development fixture versions are not public upgrade sources.

POSIX, with an existing destination directory:

```sh
curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
  --output install.sh \
  https://github.com/nuggocto/orifude/releases/download/v1.0.0/install.sh
```

After the download succeeds, inspect `install.sh`. Then run:

```sh
sh install.sh --bin-dir "$HOME/.local/bin"
```

PowerShell, with an existing user-owned destination directory:

```powershell
curl.exe --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 `
  --output install.ps1 `
  https://github.com/nuggocto/orifude/releases/download/v1.0.0/install.ps1
if ($LASTEXITCODE -ne 0) { throw 'Installer download failed.' }
```

Inspect the completed file before executing it:

```powershell
powershell.exe -NoProfile -File .\install.ps1 -BinDir "$env:LOCALAPPDATA\Programs\Orifude"
```

Create the chosen directory separately if it does not exist. Both installers are
noninteractive, never invoke sudo, and leave profiles and PATH unchanged. Add the
directory to your user PATH through your normal shell or operating-system settings.
A failed archive download or verification leaves the existing executable intact.
Removing the executable preserves saved progress.

## Publication

Before creating a public release, approve the exact clean `shrek` commit, its
successful ordinary CI run, and its successful candidate run. The version must
match the signed annotated release tag. Keep the tag and branch fixed until
publication finishes.

Inspect the exact proposed asset set without writing anything:

```sh
mise run release-publish -- v1.0.0 APPROVED_COMMIT CANDIDATE_RUN_ID
```

Adding `--publish` creates a draft with the complete archive, checksum, and
installer set. The command downloads the draft again and compares every byte,
rechecks commit and tag identity, then publishes and verifies the immutable release.
It refuses a preexisting release so a partial attempt requires inspection instead
of silently replacing assets. Release immutability must already be enabled.

The separate publication workflow defaults to this dry run. Actual publication
uses the `release` environment's `RELEASE_TOKEN`: a token restricted to this
repository, with contents write, actions read, and administration read for checking
immutability. Do not put a broad personal token in that secret. Local publication
can instead use the release operator's GitHub CLI session. No publisher credential
is passed to ordinary checks or candidate builds.

After publication, users and automation can verify the release and a local asset:

```sh
gh release verify v1.0.0 --repo nuggocto/orifude
gh release verify-asset v1.0.0 ARCHIVE --repo nuggocto/orifude
```

These checks need a real published release attestation. A fixture or draft cannot
establish that evidence. The publisher stops before any package update if release
verification fails.

## Package repository updates

The generated formula, Scoop manifest, and `PKGBUILD` share the archive manifest.
The AUR publisher generates `.SRCINFO` with `makepkg --printsrcinfo`.

```sh
mise run release-channel -- target/release-dist homebrew
mise run release-channel -- target/release-dist scoop
mise run release-channel -- target/release-dist aur
```

These commands verify the immutable release and show the proposed repository diff.
Only `--push` writes an external repository. Each update clones the exact approved
remote into a temporary checkout and uses a normal non-force push. A concurrent
change stops the update. Use credentials scoped separately to the tap and bucket;
AUR uses the operator's dedicated `aur@sshmoi.com` SSH identity. The tooling never
reads or stores private key material. No AUR package other than `orifude-bin` is
published.

## Recovery

If draft creation or upload fails, inspect the draft and candidate hashes before
removing the incomplete draft and repeating preparation. Never delete or recreate
a published immutable release to repair its binaries. Publish corrected canonical
assets under a new patch version.

If one package update fails, keep the successful canonical release and other
channels intact. Inspect the failed channel's current commit, then rerun its dry
run and normal push against the same verified assets. Correct package-only metadata
with a package revision where supported; do not change an upstream archive hash
without a different verified upstream artifact.

An executable rollback can use a previous compatible puzzle-game release. Preserve
a backup of player data before any rollback involving a schema change. The retired
letter-exchange application's `v0.2.0` is never a supported rollback target.
