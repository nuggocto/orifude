# Security overview

Orifude runs offline and treats local packs, archives, saved data, command-line
arguments, and terminal input as untrusted. The controls below protect saved
progress, terminal state, and files outside its managed directories. This is a
description of the current design, not a guarantee that every defect is absent.

## Local input and storage

| Boundary | Controls and source |
| --- | --- |
| Commands and errors | [CLI](../src/cli.rs) accepts a fixed grammar. [Error output](../src/main.rs) replaces control characters and stops after eight causes or 16 KiB. |
| TOML and replay data | [Pack parsing](../src/packs/format.rs) and [replay decoding](../src/storage/replay.rs) check byte limits, reject undeclared fields, validate domain values, and execute supplied solutions through the production engine. |
| Archives and paths | [Pack loading](../src/packs/mod.rs) bounds input, file count, and expanded bytes. [ZIP preflight](../src/packs/archive.rs) rejects ambiguous catalogs, duplicate paths, traversal, links, special files, and unsupported archive forms before installation. |
| SQLite | [Storage](../src/storage/mod.rs) checks paths, takes an exclusive process lock, validates schemas and rows, and uses parameterized statements and transactions. Capacity limits preserve space for essential progress writes. |
| Installed packs | Private staging, a pending-operation record, and registry reconciliation make installation recoverable. Fingerprints detect changed content before play. Source directories are never removal targets. |
| Terminal | [Input](../src/tui/event.rs) is read on the main thread, so no queue can grow. External text cannot emit terminal controls. [Terminal ownership](../src/tui/terminal.rs) tracks acquired capabilities and retries failed restoration steps. |

The [authoring guide](puzzle-authoring.md#limits) gives pack limits, and the
[contributor guide](../CONTRIBUTING.md#resource-limits-and-verification) records
storage and runtime bounds. Application code denies unsafe Rust, and release
builds retain checked arithmetic and panic unwinding.

Valid SPDX whitespace in license metadata becomes ordinary spaces before display
or storage. Startup repairs legacy registry licenses in one bounded transaction;
it preserves pack source bytes, fingerprints, progress, and replays. Invalid SPDX
expressions and other control characters remain errors.

## Installers and publication

The README's install commands fetch `install.sh` and `install.ps1` from a fixed
GitHub release download URL, so users trust GitHub and this repository's release
process. Script inspection and GitHub release attestations remain available for
anyone who wants to check first.

From version 1.1.2 the scripts tolerate a truncated download. The POSIX body is
one `main` function called on the last line, and the PowerShell body is one
script block that PowerShell parses before running. A partial transfer defines
nothing runnable or fails to parse. The installer check pipes truncated copies
of the real scripts and requires the previous executable to survive.

The [installer templates](../scripts/release/) embed archive checksums, validate
the archive layout and binary version, and replace the executable only after
verification. Version 1.0.1 creates a user-owned default directory. Windows
updates only user PATH, preserves expandable entries and registry type, and
offers `-NoPath`. Its execution-policy option applies to the child process;
saved policy, machine PATH, and shell profiles remain unchanged.

[Game publication](../examples/distribution/publication.rs) binds the version,
clean source commit, signed tag, passing CI, and verified candidate before
publishing immutable assets. Package updates verify release and asset
attestations, use fixed remotes, and never force-push. Publication credentials
are separate from ordinary checks. The [distribution guide](distribution.md)
documents inspection, credential scopes, and recovery.

[Pack CI](../.github/workflows/packs.yml) builds trusted base code before reading
submitted data. Validation runs in a bounded container without network access
or credentials. [Pack publication](../.github/workflows/pack-release.yml) requires
a committed review and maintainer attestation for the selected source commit.
Its write job compares prepared asset bytes and verifies release attestations.
Maintainers remain responsible for authorship, license rights, and puzzle quality.

## Verification and limits

Focused regressions cover invalid input, archive escape attempts, transactions,
recovery, and terminal restoration. `mise run audit` checks the separate product
and fuzz dependency policies; `mise run secret-scan` checks local content and
Git history. Sanitizer campaigns and native artifact journeys complement these
checks. Release-specific evidence belongs in [release QA](release-qa.md).

A process already running as the same user can change that user's files;
directory validation is not isolation from that account. Installers assume
user-controlled temporary and destination directories. Dependency defects,
compromised maintainers, and compromised build or hosting providers remain
outside what parser checks and archive hashes alone can prevent.
