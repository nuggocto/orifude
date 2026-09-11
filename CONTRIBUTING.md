# Contributing to Orifude

Orifude is a native Rust terminal game that works fully offline. The permanent
default branch is `shrek`. The separate `orifude-front` repository contains the
static website; the game has no accounts, telemetry, or hosted application service.

## Build and check

Install mise, a native C compiler, and the platform SDK needed by that compiler.
SQLite is bundled and compiled locally. On Windows, use the MSVC build tools and
Windows SDK. Tool versions come from [the Rust toolchain](rust-toolchain.toml),
[mise.toml](mise.toml), and their lockfiles.

```sh
mise install --locked rust github:EmbarkStudios/cargo-deny shellcheck@0.11.0
mise run run
mise run check
```

The full check runs formatting, shell analysis, both dependency policies,
warning-denied Clippy, tests, doctests, and a release build. On Windows, shell
analysis also needs Bash. For a focused change, use `mise run test`,
`mise run lint`, or `mise run test-native`; task definitions are in
[mise.toml](mise.toml).

Tests that launch the game use `isolated-test-paths` to keep player data separate.
The mise test tasks enable it, and Cargo refuses the CLI test target without it.
The production binary ignores `ORIFUDE_TEST_ROOT`. Packaged-player and
package-manager checks need disposable accounts; follow the
[distribution guide](docs/distribution.md) before running them.

On Linux with Nix and flakes enabled, `nix build` builds the game and
`nix flake check --no-update-lock-file` also verifies the installed player journey.
The [Nix guide](docs/distribution.md#nix-and-nixos) describes supported systems
and the pinned build inputs.

## Code layout

| Location | Responsibility |
| --- | --- |
| [main.rs](src/main.rs), [cli.rs](src/cli.rs), [author.rs](src/author.rs) | Process lifecycle, command grammar, and local author commands |
| [domain/](src/domain) | Paper, folds, ink, puzzle rules, attempts, and replays |
| [solver/](src/solver), [generator/](src/generator) | Bounded search and deterministic puzzle generation |
| [storage/](src/storage) | SQLite, migrations, progress, replays, and managed pack installation |
| [packs/](src/packs) | TOML and ZIP validation, portable paths, and fingerprints |
| [tui/](src/tui) | Navigation, play sessions, rendering, input, and terminal restoration |
| [content/](src/content), [puzzles/journey/](puzzles/journey) | Embedded official papers |
| [examples/](examples), [scripts/](scripts) | Authoring examples, measurements, release tooling, and checks |

```text
App -> PlaySession -> Attempt -> Paper
Solver / Generator -> production domain actions -> verified Replay
Storage -> saved completion, progress, and replay in one transaction
```

Paper keeps a dense vector of at most 144 physical cells. Its row-major index
is the stable cell identity; folds change position, layer, face, and orientation.
Complete snapshots make undo straightforward within the 64-entry history limit.
Failed actions must leave state unchanged.

The solver retains exact state keys and parent records instead of full attempts.
It replays production actions to restore candidates and verify solutions.
Generation has a versioned deterministic seed and a fixed attempt budget.
Preserve daily-puzzle output across supported platforms.

The event worker owns terminal polling. Its 256-entry queue preserves keys and
coalesces ticks and resizes. One work manager owns and joins at most one
cancellable generation job. Terminal capabilities restore in reverse acquisition
order, including failure paths. Release builds retain overflow checks and panic
unwinding; expected I/O and validation failures return errors.

## Saved data and compatibility

[AppPaths](src/storage/paths.rs) resolves these per-user locations:

| OS | Data | Configuration | Cache |
| --- | --- | --- | --- |
| Linux | `$XDG_DATA_HOME/orifude` | `$XDG_CONFIG_HOME/orifude` | `$XDG_CACHE_HOME/orifude` |
| macOS | `~/Library/Application Support/orifude` | Same as data | `~/Library/Caches/orifude` |
| Windows | `%APPDATA%\orifude\data` | `%APPDATA%\orifude\config` | `%LOCALAPPDATA%\orifude\cache` |

Linux falls back to `~/.local/share`, `~/.config`, and `~/.cache`. The data
directory holds `orifude.sqlite3`, `orifude.lock`, managed packs, and staging.
Settings currently live in SQLite. One process lock protects one connection;
schema checks and migrations run before terminal entry. Corrupt or unsupported
data must produce a recovery error rather than silently resetting progress.

A completion commits its replay, best result, history, and progress together.
Replays carry the exact gameplay revision; cosmetic text changes keep them
compatible, while changed rules or targets can invalidate them. Removing a pack
or executable preserves saved progress. Keep migrations transactional and test
recovery before changing stored formats.

Pack installation validates and stages content before recording one pending
operation, renaming it, and committing the registry entry. Startup reconciliation
must leave a registered pack or no managed copy. Source directories are never
cleanup targets. Selecting an installed pack verifies its fingerprint.

## Resource limits and verification

| Resource | Current ceiling |
| --- | --- |
| Solver | 250,000 visited states, 128 MiB retained-memory budget, 20 actions deep |
| Generator | 512 candidate attempts |
| SQLite | 128 MiB main file, 16 MiB reserved for essential writes, 132 MiB rollback-journal budget |
| Saved history | 20 recent replays per puzzle; best solutions are protected |
| Managed packs | 32 installed, 512 MiB total, one loaded for play |
| Terminal | 160 × 60 rendered viewport, 30 Hz animation; minimum size 60 × 20 |

[Pack format limits](docs/puzzle-authoring.md#limits) belong in the authoring guide.
Raise a limit only with a resource estimate and checks of the affected boundary.
Tests should detect broken behavior: use independent replay or solver models,
malformed-input cases, failure recovery, and real terminal journeys where relevant.
Avoid assertions on decorative layout or facts guaranteed by Rust's type system.

Use `mise run property-check` for deterministic independent models,
`mise run release-check` for optimized checks, and `mise run fuzz-campaign` for
bounded sanitizer campaigns. `mise run release-measure` measures release startup,
input, memory, and storage against the budgets in [release QA](docs/release-qa.md).
Record the tested commit, artifact, environment, and meaningful limitations.

For packs, follow the [authoring guide](docs/puzzle-authoring.md). For release
work, use the [distribution guide](docs/distribution.md), [security
overview](docs/security-review.md), and [release evidence](docs/release-qa.md).
Keep these documents current; change explanations belong in commit messages.
