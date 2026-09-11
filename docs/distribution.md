# Building and distributing Orifude

Release tooling is the Rust [distribution example](../examples/distribution.rs),
built with the pinned compiler through mise. Start with the
[contributor setup](../CONTRIBUTING.md#build-and-check). Its archive and JSON
dependencies are development-only; installer QA uses OpenSSL for private loopback
HTTPS. The artifact matrix comes from [Cargo.toml](../Cargo.toml).

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

The candidate workflow tests every target without publication secrets and retains
`release-set` as an artifact. Archive checks reject altered checksums, unsafe
members, and wrong architectures. Repacking identical inputs is deterministic;
independent compiler output is not promised. Generate all hashes and package
metadata from one candidate set.

Installer fixtures use private script copies and local HTTPS. Published scripts
have fixed release URLs. Windows passes the fixture CA directly to curl without
changing the certificate store. Package fixtures use local archives; Homebrew and
Scoop exercise upgrades through a metadata revision. Arch installs x86_64 natively
and assembles ARM packages, whose binaries have separate native player checks.

## Installing an exact published version

Use the copyable commands on [the installation page](https://orifude.com/install/).
They download the complete exact-version script over HTTPS before executing it.
For separate inspection, download the release's `install.sh` or `install.ps1`,
read it, and verify it with `gh release verify-asset` as shown below. The
[security overview](security-review.md#installers-and-publication) explains the
bootstrap trust assumptions.

With version 1.0.1 and later, run the downloaded POSIX script with:

```sh
sh install.sh
```

It creates `$HOME/.local/bin`. Use `--bin-dir /absolute/directory` to choose another
location. Profiles stay unchanged. If the directory is absent from PATH, the
installer prints the executable's full path; add the directory through your usual
shell settings to run `orifude` by name.

On Windows, run the downloaded script with:

```powershell
powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File .\install.ps1
```

It creates `%LOCALAPPDATA%\Programs\Orifude` and adds that directory to the saved
user PATH. Close and reopen your terminal application, then run `orifude`.
Use `-BinDir C:\your\directory` for another destination, or `-NoPath` to leave
PATH unchanged. The command's execution-policy option applies only to its child
process; it changes no saved policy or profile and requires no administrator.

Version 1.0.0 keeps its original interface: create the destination first and pass
`--bin-dir` or `-BinDir` explicitly. Its installers do not update saved PATH.

Failed archive checks preserve the existing executable. Removing the executable
preserves saved progress.

## Publication

Changes only to `docs/`, `CONTRIBUTING.md`, `IDEAS.md`, or `AGENTS.md` skip ordinary
CI, candidate, and pack workflows. README and changelog changes still run them
because they affect release assets. For a release from a documentation-only
commit, dispatch CI and Release candidate for that exact commit first.

Before creating a public release, approve the exact clean `shrek` commit, its
successful ordinary CI run, and its successful candidate run. The version must
match the signed annotated release tag. Keep the tag and branch fixed until
publication finishes.

Inspect the exact proposed asset set without writing anything:

```sh
mise run release-publish -- vX.Y.Z APPROVED_COMMIT CANDIDATE_RUN_ID
```

Adding `--publish` creates a draft with the complete archive, checksum, and
installer set. The command downloads the draft again and compares every byte,
rechecks commit and tag identity, then publishes and verifies the immutable release.
It refuses a preexisting release so a partial attempt requires inspection instead
of silently replacing assets. Release immutability must already be enabled.

The publication workflow defaults to a dry run. Its `release` environment permits
only `shrek`. `RELEASE_TOKEN` needs access only to this repository, with contents
write, actions read, and administration read for the immutability check. Local
publication can use the operator's GitHub CLI session. Ordinary checks and
candidate builds receive no publisher credential.

After publication, users and automation can verify the release and a local asset:

```sh
gh release verify vX.Y.Z --repo nuggocto/orifude
gh release verify-asset vX.Y.Z ARCHIVE --repo nuggocto/orifude
```

Replace `vX.Y.Z` with the release tag and `ARCHIVE` with the downloaded file.
These checks require a published release attestation. Package updates stop if
release verification fails.

The `Published release verification` workflow runs in two parts. Dispatch
`installers` after GitHub publication and before updating package repositories;
dispatch `packages` after their updates. It checks the real public URLs and
repositories, then plays, saves, restarts, and replays using the installed binary.
Each job verifies release attestations, archive contents, generated metadata, and
the installed executable's bytes before running the player journey.

On a disposable native host, the same command is:

```sh
mise run published-check -- x86_64-unknown-linux-musl installer
```

Use `homebrew`, `scoop`, or `aur` for a supported package channel, or `archives`
for direct download. Homebrew checks refuse an existing Orifude installation or
tap. Scoop uses a private directory. AUR installation and removal run in an Arch
container; its installed executable then runs the player journey on the native
Linux host. These checks do not establish minimum-OS or terminal-GUI compatibility.

## Package repository updates

The generated formula, Scoop manifest, and `PKGBUILD` share the archive manifest.
The AUR publisher generates `.SRCINFO` with `makepkg --printsrcinfo`.

```sh
mise run release-channel -- target/release-dist homebrew
mise run release-channel -- target/release-dist scoop
mise run release-channel -- target/release-dist aur
```

These dry runs validate the candidate and show the proposed repository diff, even
before publication. Their output states that release attestation has not been
checked. `--push` first verifies the immutable release and each archive attestation;
only then can it write an external repository. Each update clones the exact approved
remote into a temporary checkout and uses a normal non-force push. A concurrent
change stops the update. Use credentials scoped separately to the tap and bucket;
AUR uses the operator's dedicated `aur@sshmoi.com` SSH identity. The tooling never
reads or stores private key material. No AUR package other than `orifude-bin` is
published.

## Nix and NixOS

The source flake supports Linux x86_64 and ARM64. It pins nixpkgs, rust-overlay,
the declared Rust toolchain, and Cargo dependencies. Nix downloads the build
inputs; the installed game works offline. This is a project flake, not a package
in the nixpkgs collection.

With Nix and flakes enabled, play an exact release or install it in your profile:

```sh
nix run github:nuggocto/orifude/v1.0.2
nix profile add github:nuggocto/orifude/v1.0.2
```

For a NixOS configuration that already uses flakes, add an input:

```nix
inputs.orifude.url = "github:nuggocto/orifude/v1.0.2";
```

Pass the input into your modules through `specialArgs`, or use it in an inline
module in the flake's outputs:

```nix
{ pkgs, ... }: {
  environment.systemPackages = [
    orifude.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
}
```

Here `orifude` is the input bound by the outputs function. For a separate module
receiving `specialArgs`, include `orifude` in that module's argument list.
Update your configuration's lockfile and rebuild through your usual NixOS
workflow. The package uses the same per-user data paths as the other Linux builds.

Maintainers run `nix flake check --no-update-lock-file` on each supported Linux
architecture. It builds the production package, runs the Rust tests, and exercises
the installed binary through the complete player journey. CI does this on native
x86_64 and ARM64 runners in a pinned Nix container. Test-only features are enabled
after installation and never enter the installed executable. Update `flake.lock`
deliberately when changing Nix inputs, and recheck both architectures before tagging.

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
