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
| Terminal | [Input](../src/tui/event.rs) has a bounded queue. External text cannot emit terminal controls. [Terminal ownership](../src/tui/terminal.rs) tracks acquired capabilities and retries failed restoration steps. |

The [authoring guide](puzzle-authoring.md#limits) gives pack limits, and the
[contributor guide](../CONTRIBUTING.md#resource-limits-and-verification) records
storage and runtime bounds. Application code denies unsafe Rust, and release
builds retain checked arithmetic and panic unwinding.

## Installers and publication

The website's default command trusts an exact immutable GitHub release over
HTTPS. It downloads the complete script into a fresh temporary directory before
execution, with transfer size and time limits and cleanup. It does not independently
authenticate the script with a second hash. Script inspection, the reviewed
PowerShell hash, and GitHub release attestations are available separately.

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

## Static website

The separate frontend validates bounded release records and hash-checked
changelog snapshots, escapes contributor text, and derives download links from
the fixed repository. Its only application script is the optional installation
clipboard helper, permitted by its exact hash through CSP and script integrity.
It has no accounts, analytics, or browser game.

Cloudflare's injected bot-detection markup on 404 responses was observed blocked
by CSP. Removing the injection requires zone permissions unavailable during
release verification; the owner accepted this limitation and waived the
`www` redirect. [Release QA](release-qa.md#coverage-and-limitations) records the
tested scope.

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
