# Code review on 2026-09-04

Reviewed baseline: [`027360f`](https://github.com/nuggocto/orifude/commit/027360f02d47b2975f7b461338b3272ced7f568c)
on `shrek`. This review covers the implemented native application, domain,
solver, generator, built-in content, persistence, pack handling, terminal UI,
tests, examples, fuzz entry points, scripts, dependency policy, and current CI.
The separate static frontend and unimplemented publication machinery are outside
the scope. Product requirements come from [PROJECT.md](../PROJECT.md).

The findings below describe the reviewed baseline. Their corrections and new
regression evidence are recorded in the
[notebook](../NOTEBOOK.md#review-corrections-on-2026-09-04).
The ZIP version label was corrected from the manifest minimum, 8.1.0, to the
locked version, 8.6.0. Regression tests reproduced the findings with that locked
dependency before the fixes.

Four findings survived code tracing and focused reproduction. Three are medium
severity and one is low severity. No critical or high-severity defect was
confirmed. Product source and tests were not changed during this review.

## Medium: revised puzzles compete against obsolete best scores

Location: [`insert_completion_rows`](../src/storage/mod.rs), lines 1405-1432,
and `update_progress`, lines 1461-1480.

The completion write looks up previous progress by pack and puzzle ID, then
compares only folds and strokes. It does not check whether the previous best
belongs to the same gameplay revision. A successful solution to a revised
puzzle with an equal or worse score leaves the obsolete replay as the best.

Meanwhile, `Storage::completion_matches` correctly requires the best replay's
embedded puzzle to match the current puzzle. Consequently, saving the new
solution can succeed while this method still returns false. The TUI marks the
paper complete in memory in [`App::completion_saved`](../src/tui/app.rs), but
[`play`](../src/tui/mod.rs) reloads completion from storage on startup.
The revised journey paper then appears incomplete again, and access to its next
paper can disappear after restart. Community keepsakes can likewise keep the
obsolete best solution.

Reproduction through the public storage API and the optimized library:

1. Create a valid 4-by-4 dot puzzle with identity
   `orifude-journey/first-drop`, target cell at row 0, column 0, zero folds,
   and one stroke. Save its successful one-dot replay in an isolated database.
2. Create another puzzle with the same identity and budgets, changing the target
   to row 0, column 1. Save its successful one-dot replay.
3. Check `completion_matches` for the new puzzle immediately and after reopening
   the database. Both return false. `best_replay` still embeds the old puzzle.

Both saves returned success. This reproduces persistence behavior directly;
the restart effect in the TUI follows from the callers above. The existing
`built_in_completion_requires_the_saved_gameplay_revision` test in
[`tests/storage.rs`](../tests/storage.rs) checks mismatched and matching saves
in separate databases, so it does not cover this transition.

Fix direction: compare scores only within the same gameplay revision. A first
completion of a different revision must establish that revision's current best
and completion state. Keep any older replay according to the bounded history
policy. Verify equal and worse new scores, restart, and same-revision ties.

## Medium: event shutdown can lose its wakeup

Location: [`SharedQueue::begin_shutdown`](../src/tui/event.rs), lines 194-198.
The affected waits are in `send`, lines 133-137, and `receive`, lines 161-169.

The shutdown flag is atomic, but its transition does not take the mutex used
by the condition-variable waits. A receiver can check that shutdown is false,
be descheduled, miss the shutdown notification, and then wait indefinitely.
The equivalent window exists for a sender waiting for a full queue to drain.
Acquire/release ordering does not close the gap between checking and waiting.

```mermaid
sequenceDiagram
    participant R as SharedQueue::receive
    participant W as EventPump worker
    Note over R: Holds state mutex; queue empty; shutdown false
    W->>W: begin_shutdown stores true
    W-->>R: not_empty.notify_all before waiter exists
    R->>R: wait releases state mutex and blocks
    Note over R: No worker remains to send another event
```

A terminal-input failure can therefore leave the main thread blocked instead
of reporting the error and restoring the terminal. Shutdown under queue
pressure can also block while joining the sender. Rust's
[condition-variable documentation](https://doc.rust-lang.org/std/sync/struct.Condvar.html#method.wait)
describes the atomic unlock-and-wait operation; a notification before that
operation is not retained for a future waiter.

Reproduction used a temporary copy of the actual event module with a two-barrier
scheduling hook immediately before `.wait(state)`. The receiver paused after
the predicate check while retaining the mutex. Another thread called the real
`begin_shutdown`, then released the receiver into the wait. The receiver did
not finish within 200 ms. An additional notification, sent while holding the
state mutex, let it return `None` and join successfully.

This is a controlled reproduction of a permitted thread interleaving, not an
observed hang in the shipped binary. Two uninstrumented runs of 20,000 races
did not reproduce it. The controlled probe changes scheduling only; repository
source was left intact.

Fix direction: synchronize the shutdown predicate transition with the same
state mutex used by both waits, then notify both condition variables. Preserve
recoverable handling of a poisoned mutex. Verify both empty-queue receivers
and full-queue senders with controlled scheduling.

## Medium: ZIP preflight can validate a different footer from the parser

Location: [`validate_archive_bytes`](../src/packs/archive.rs), lines 20-23,
and `preflight_entry_count`, lines 105-139.

The preflight scans backward for an end-of-central-directory record and checks
its declared entry count. `ZipArchive::new` then independently scans the same
bytes. The locked `zip` 8.6.0 reader can reject the last footer and fall back to
an earlier one. It allocates and parses that directory before Orifude checks
`archive.len()`.

Reproduction used a small archive with 300 one-byte files. Ordinarily, preflight
rejects it with `archive entry table exceeds its supported bounds`. Appending
a 22-byte decoy footer changes the result to the later
`entry count exceeds the limit`. The library has already parsed all 300 entries,
despite Orifude's limit of 258 entries including directories.

The decoy is an ordinary little-endian ZIP footer with signature `PK 05 06`,
both disk numbers zero, both entry counts one, central-directory size zero,
central-directory offset equal to the original archive length, and comment
length zero. Orifude accepts the offset-plus-size boundary. The ZIP reader
rejects that footer's directory and uses the preceding real footer. Calling
`ZipArchive::new` on the same input confirms a length of 300.

The 8 MiB input limit still applies, and this probe is ultimately rejected
before installation. The confirmed problem is that the catalog allocation and
parsing limit is bypassed. No memory-exhaustion stress test, path escape, or code
execution was demonstrated.

Fix direction: make the parser consume the same directory that passed bounded
preflight. Validate raw catalog records within the entry budget before allowing
library allocation, and reject ambiguous or fallback interpretations. Merely
tightening one offset comparison does not establish that agreement. Add a
regression that checks rejection before excessive catalog parsing.

## Low: exact duplicate ZIP filenames disappear before validation

Location: [`validate_archive_bytes`](../src/packs/archive.rs), lines 29-31
and 65-67.

The locked ZIP reader stores entries in a filename-keyed map. Exact duplicate
names overwrite earlier entries before Orifude enumerates them. Its
`folded_paths` set therefore catches case-only collisions, but cannot catch
exact duplicates that the dependency has already discarded.

Reproduction used three stored entries: invalid bytes in `pack.toml`, valid
metadata in `hack.toml`, and a valid `puzzles/dot.toml`. Replacing the two
occurrences of `hack.toml` in the local and central headers with the equal-length
name `pack.toml` creates the duplicate without changing the directory layout.
The public `validate_archive_bytes` function returns a valid pack and silently
ignores the first metadata entry. The metadata names pack `review-pack`, uses
license `Apache-2.0`, and declares the valid one-dot puzzle `dot`.

This violates the pack format's rejection of duplicate paths and accepts
ambiguous content. The current duplicate test in
[`tests/packs.rs`](../tests/packs.rs) uses `pack.toml` and `PACK.toml`, which
remain distinct in the dependency's map. There was no filesystem escape or
code execution in this reproduction.

Fix direction: reject duplicate raw entry names before the ZIP reader collapses
them. Include exact duplicates with different contents as well as case-only
collisions. This can share the bounded raw catalog validation needed above.

## Toolchain follow-up

[`rust-toolchain.toml`](../rust-toolchain.toml) pins Rust 1.98.0. On September 3,
Rust released [1.98.1](https://blog.rust-lang.org/2026/09/03/Rust-1.98.1/) to fix
a vtable miscompilation in 1.98.0 that can emit null function pointers. Updating
the reviewed toolchain pin and rebuilding before publication is warranted.
No Orifude-specific manifestation was established, so this is separate from
the four confirmed application findings.

## Verification and assessment

- `mise run release-check` passed formatting, shell syntax and ShellCheck,
  locked product and fuzz dependency policy, strict all-target Clippy, 227
  optimized unit/integration/example tests, the doctest, and the production
  release build. Seven tests exercise the executable in a native Linux PTY.
- `cargo test --locked --all-targets --features isolated-test-paths` passed the
  same 227 tests in the debug profile.
- `scripts/property-check.sh` passed exhaustive fold boundaries, fixed action
  sequences, the independent tiny solver, deterministic generation, and the
  complete official journey.
- `scripts/secret-scan.sh` found no high-confidence credential pattern in the
  local tree or reachable Git history.
- The production `target/release/orifude` binary verified and solved all three
  papers in `puzzles/example-pack` successfully.
- The focused revision, ZIP, and scheduled shutdown probes produced the
  outcomes described above. Pack probes used in-memory bytes, and storage used
  a private test database.

The sandbox initially prevented advisory-database locking and two CLI tests'
filesystem effects. Authorized native reruns passed; those failures are not
product findings. This review ran on x86_64 Linux with the pinned toolchain.
It did not repeat the sustained sanitizer campaigns, performance measurements,
hosted macOS/Windows runs, minimum supported OS checks, or GUI terminal checks
recorded in [release QA](release-qa.md).

The domain representation, validated constructors, atomic rejected actions,
replay verification, bounded search, storage transactions, and terminal cleanup
design provide a sound basis. The tests exercise meaningful behavior, including
independent models and real process boundaries. The findings concern specific
revision and synchronization transitions, plus assumptions at the ZIP library
boundary. They warrant focused corrections, not a rewrite. They also qualify
the broader ZIP and shutdown assurances in the earlier
[security review](security-review.md).
