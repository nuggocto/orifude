# Orifude

A small sheet of paper rests in the terminal. You fold it and leave a little ink
between its layers. When you open it again, the marks show the path taken by the
folds.

Orifude is a quiet, keyboard-driven puzzle game for the terminal. The player
folds a sheet of paper, applies ink through the stacked layers, and unfolds the
sheet to reproduce a target pattern. It is a native Rust application, works
without a network connection, and stores progress locally.

This document is the product specification, architecture record, release
contract, and only phase tracker for Orifude v1.

## Current work

- Current phase: **Phase 6, terminal foundation**
- Current checklist item: **Add and pin the selected Ratatui and Crossterm
  versions.**
- Last updated: **2026-09-02**

Checkbox rules:

- `[ ]` means the work has not been verified.
- `[x]` means the behavior and its stated verification are complete.
- A file existing or code compiling does not make an item complete.
- Keep exactly one current checklist item in the `Current work` section.
- Phase names, numbers, and progress belong only in this document.

## Product identity

- Product name: `Orifude`
- Binary name: `orifude`
- Main repository: `https://github.com/nuggocto/orifude`
- Permanent default branch: `shrek`
- Frontend repository: `https://github.com/nuggocto/orifude-front`
- Public domain: `https://orifude.com`
- License: Apache License 2.0
- Primary form: native terminal user interface
- Primary input: keyboard
- Language: Rust
- Network requirement: none
- Persistent storage: local SQLite

Orifude is a coined name inspired by folding and brushwork. It must not be
presented as a Japanese dictionary word or as a claim of cultural authenticity.

The repository retains source history and the `v0.2.0` tag from the retired
letter-exchange application. Its GitHub Release and distribution entries are
gone, and it is not a supported version of this puzzle game. The puzzle product
starts at `0.1.0` and first publishes at `v1.0.0`. It does not read, migrate,
modify, or delete data from the retired application. Its changelog begins with
the puzzle-game reboot, and no installer or package channel offers an automatic
upgrade or rollback between the two products.

Running a puzzle-game installer is an explicit fresh installation. It may
replace the executable at the chosen destination after verification, but it
does not detect, migrate, or remove retired-product data. Package metadata must
not declare the retired release as an upgrade source or rollback target.

The artwork in `/home/nuggocto/Pictures/Orifude` is the identity source:

- `Orifude-logo.png` contains the squirrel courier, folded paper, branch,
  berries, and brush circle.
- `Orifude-icon.png` contains the brush circle and folded leaf.
- `Orifude-watermark.png` contains the wordmark and folded leaf.
- The monochrome variants guide the terminal mark and high-contrast output.

## The game

Orifude gives the player one small paper puzzle at a time. There is no timer,
energy system, login streak, public ranking, advertisement, or demand to keep
playing. Undo and restart are always available. The challenge comes from
understanding how folds transform ink, not from fighting the interface.

The product should be recognizable from a screenshot. Quiet spacing, irregular
ink, folded-paper geometry, and a restrained branch motif matter more than a
large feature list.

## Goals

- Make folding and unfolding understandable in a terminal grid.
- Build a deterministic puzzle engine shared by handcrafted and generated
  puzzles.
- Ship a polished native binary for Linux, macOS, and Windows.
- Remain fully useful offline after installation.
- Let contributors add and validate puzzle packs without changing Rust code.
- Keep resource use bounded and state transitions auditable.
- Preserve progress safely in one local SQLite database.
- Release reproducible, checksummed archives through GitHub.
- Distribute through Homebrew on macOS, Scoop on Windows, and one binary AUR
  package named `orifude-bin`.
- Maintain a small static website that explains the game, links to GitHub and
  releases, and presents the changelog after each release.

## Supported platforms

Orifude v1 supports this native platform matrix:

- Linux kernel 5.10 or newer on x86_64 and ARM64. Release archives use the
  corresponding musl Rust targets.
- macOS 13 or newer on Intel and Apple Silicon.
- Windows 10 22H2 or newer on x86_64.

Native Linux QA covers Ubuntu 24.04, Debian 12, Fedora, and Arch Linux. The
Fedora and Arch Linux versions are the versions current when the release
candidate is cut and are recorded in the QA evidence. Windows ARM64 is not a v1
release target.

A platform is advertised only after its native release binary completes the
required end-to-end journey. Cross-compilation proves that an artifact can be
built, not that the platform works.

## Non-goals for v1

- A browser version of the game
- Accounts, profiles, cloud saves, or cross-device progress
- Online multiplayer or cooperative play
- A server API or hosted game database
- Telemetry, advertising, tracking, or behavioral analytics
- A global leaderboard or anti-cheat system
- Downloading executable puzzle plugins
- Running scripts supplied by puzzle packs
- Automatic network access from the game
- Mouse-first interaction
- Mobile or graphical desktop clients
- Diagonal, curved, or freeform creases
- Realistic cloth, paper, or fluid physics
- User-created bitmap, audio, or video content
- AWS, GCP, Azure, Firebase, Supabase, or another application cloud
- Package variants such as `orifude-git` or a source-building AUR package

## Player experience

The player receives a sheet, studies the target, folds the paper, adds ink, and
opens it again. A finished paper becomes a keepsake on the branch.

The squirrel appears when a paper arrives and when a completed result joins the
branch. It should not sit in every menu.

### Home branch

Launching `orifude` opens the home branch. It shows current progress and the
available ways to play:

- Continue the handcrafted journey.
- Open today's deterministic paper.
- Enter the endless garden.
- Play an installed puzzle pack.
- Review keepsakes and best solutions.
- Open settings and help.

The first launch gives a short interactive lesson instead of a wall of text.
The player can replay the lesson later.

### How to play

The home branch and contextual help open a replayable `How to play` view. It
teaches the rules with one small paper instead of a page of instructions. The
player can step forward and backward through these views:

- The target beside a fresh unfolded paper.
- The chosen crease, moving side, and folded result.
- A side view that labels the top and bottom of the resulting stack.
- Ink passing through every layer under the brush.
- The unfolded result compared with the target, including extra and missing
  ink.

The TUI derives every teaching frame from production domain state. It does not
maintain a second hand-written version of fold behavior. Captions explain every
symbol, and the sequence remains clear in ASCII, monochrome, and reduced-motion
modes. Reduced motion replaces transitions with player-controlled steps rather
than removing information. The first-launch lesson reuses the same components
and example.

### Puzzle screen

The puzzle screen shows:

- The unfolded target pattern.
- The current folded paper.
- The active crease or brush cursor.
- The fold and stroke budget.
- A compact action history.
- Contextual keyboard help.
- A textual status that does not rely on color.

The player can undo, reset, preview the unfolded state, inspect the target, and
quit without losing completed progress.

### Reveal

Checking a solution unfolds the paper one crease at a time. The animation must
remain brief, bounded, interruptible, and optional. Reduced-motion mode replaces
it with the final state. Failure shows extra and missing ink separately. Success
records the solution and adds the result to the branch.

## Canonical game rules

### Paper

- A paper is a rectangular grid of physical cells.
- Every physical cell has one stable original coordinate.
- A cell may move, rotate by right angles, and change face orientation through
  folds, but it is never duplicated or destroyed.
- Overlapping cells form an ordered stack.
- The engine stores enough information to undo an action exactly.
- The v1 board uses axis-aligned creases on cell boundaries.
- Built-in boards use sizes between 4 by 4 and 12 by 12 cells.

The v1 working area is the paper's original width-by-height coordinate
rectangle. A fold may leave some positions empty, and a moved stack may land at
an empty position inside the rectangle. No physical cell may move outside that
rectangle. The puzzle format has no separate overhang field.

### Fold

A fold selects a vertical or horizontal crease and one side to move. The engine
reflects every cell on that side across the crease and reverses the moving layer
order as required. At each destination, the moved stack is placed above any
stationary stack. If the destination is empty, the moved stack becomes the only
stack at that position.

A fold is legal only when:

- The crease lies on an allowed cell boundary.
- The moving side contains paper.
- Every moved cell lands on an integer grid coordinate.
- The resulting position stays within the puzzle's bounded working area.
- The move respects any puzzle-specific crease restrictions.
- The action stays within the puzzle's fold budget.

Malformed puzzle data returns a validation error. An impossible internal cell
count or duplicate physical cell is a programmer-error invariant.

### Ink

- A dot targets one occupied position on the folded paper.
- A line is an inclusive horizontal or vertical segment between two endpoints.
  Endpoint order does not matter, and every position in the segment must
  contain paper.
- Ink passes through every physical cell in the selected stack.
- Ink records against physical cells, not screen positions.
- There is no direct erase action. Undo and reset may restore an earlier state
  and thereby remove ink added by the reversed actions.
- The v1 brush set contains a dot and bounded horizontal or vertical strokes.
- Every allowed brush and stroke length is declared by the puzzle.
- A brush action must stay within the puzzle's stroke budget.

The first built-in puzzles use only a dot. Line brushes appear after the player
understands stack behavior.

### Target and success

- A target declares the exact ink state expected at every original cell.
- A solution succeeds only when missing ink and extra ink are both empty.
- The engine reports missing and extra cells separately.
- Multiple action sequences may solve a puzzle.
- The game never requires a unique solution unless the puzzle metadata says so
  and the bounded solver proves it.

### Undo, reset, and replay

- Undo restores the complete state before the last action.
- Reset restores the initial puzzle without changing stored progress.
- A replay contains a puzzle identity, exact validated gameplay revision,
  format version, engine compatibility version, and bounded action list. The
  gameplay revision covers dimensions, target, allowed folds and brushes,
  budgets, and par. Display text does not invalidate a replay.
- A solution replay contains the current successful fold and brush sequence.
  Undo removes the reversed action, and reset clears the sequence. Personal
  undo and hint history remain result metadata rather than replay actions.
- Replaying the same valid actions must produce identical state on every
  supported platform.
- Invalid, incompatible, or oversized replays fail without partially mutating
  saved progress.

### Scoring

Completing a puzzle matters more than optimization. The result records:

- Fold count
- Brush stroke count
- Undo count for personal history only
- Whether hints were used
- Whether the solution meets the declared par

Best-solution comparison uses folds first, then strokes. Meeting par requires
both the fold and stroke counts to stay at or below their declared values. Time
is never part of the score. Daily results contain no hidden timing or streak
pressure.

## Play modes

### Journey

The journey contains at least 40 handcrafted puzzles grouped by mechanic. Each
group introduces one rule through play, then combines it with earlier rules.
Completing a group changes the home branch with a new leaf, berry, folded boat,
or small animal.

Journey progress is local. A player may replay any completed puzzle and keep a
better solution without losing the earlier replay until storage pruning applies.

### Daily paper

The daily paper is generated offline from a stable seed:

```text
orifude:<engine-compatibility>:<calendar-date>
```

The seed uses the player's local calendar date. Changing the system clock is not
treated as cheating. The generator compatibility value preserves old daily
puzzles when the algorithm changes.

The player may copy a spoiler-free text result. Orifude will not use terminal
clipboard escape sequences in v1. It renders the result in a selectable view or
writes it to an explicitly chosen file.

### Endless garden

The generator creates a bounded legal action sequence, derives its target, and
asks the solver whether the result is valid and sufficiently interesting. It
rejects trivial, duplicate, over-budget, or unsolved candidates.

The generator stops after a fixed number of attempts. If it cannot produce a
valid puzzle, it reports the seed and returns to the menu. It never loops until
luck arrives.

### Puzzle packs

Players can install local puzzle packs from a directory or supported archive.
The game does not fetch packs from the network. A pack contains metadata,
bounded puzzle files, optional plain-text notes, and no executable content.

Supporting commands may validate author content without entering the TUI:

```text
orifude verify PATH
orifude solve PATH
orifude pack install PATH
orifude pack list
orifude pack remove PACK_ID
```

The game remains the default when the binary runs without a subcommand.

## Terminal design

### Visual system

The starting palette comes from the supplied artwork:

| Token | Color | Use |
| --- | --- | --- |
| Ink | `#292823` | Primary text and painted cells |
| Washi | `#F1ECE1` | Light paper surfaces |
| Moss | `#858A72` | Active controls and living leaves |
| Clay | `#A48B68` | Secondary marks and berries |
| Branch | `#62594D` | Borders and structure |
| Ash | `#A59E91` | Muted text |
| Ember | `#A45B52` | Destructive actions and errors |

The TUI must support true color, ANSI 256, ANSI 16, and monochrome. Every state
has a word, symbol, or shape in addition to color.

### Layout

- The preferred terminal is at least 80 columns by 24 rows.
- The minimum interactive layout is 60 columns by 20 rows.
- Smaller terminals show a stable resize message and retain state.
- Wide layouts show target and folded paper side by side.
- Narrow layouts switch between target and paper without horizontal scrolling.
- Resizing during a fold, reveal, dialog, or error must not panic or lose input.

### Input

The default bindings are familiar but not modal-editor cosplay:

- Arrow keys and `h`, `j`, `k`, `l` move the active cursor.
- `f` enters fold selection.
- `b` selects or applies the brush according to context.
- `u` undoes one action.
- `r` asks before resetting an in-progress attempt.
- `Space` previews the unfolded paper.
- `Enter` confirms the focused action.
- `Esc` cancels the current transient action.
- `?` opens contextual help.
- `q` requests quit and explains whether unsaved work exists.

Bindings must remain discoverable in the UI. The player can remap them only if
the resulting configuration has no required-action conflicts.

The bounded event queue preserves key-event order. Repeated tick and resize
notifications coalesce instead of occupying one slot each. A full queue applies
backpressure rather than allocating more memory or discarding a key event.
Shutdown and cancellation remain independently observable so a full queue
cannot prevent exit.

### Accessibility

- Provide an ASCII-only mode.
- Provide a monochrome high-contrast mode.
- Provide reduced motion and instant reveal.
- Never encode extra and missing ink by color alone.
- Keep focus order and help predictable.
- Avoid rapid flashing and terminal bell output.
- Sanitize all external display text before rendering.
- Make errors readable after terminal restoration when startup fails.

## Architecture

Orifude begins as one Cargo application package. A workspace split requires a
measured build, ownership, or reuse benefit.

Suggested ownership layout:

```text
src/
  main.rs              process entrypoint and exit status
  cli.rs               argument parsing and authoring commands
  app.rs               high-level application state machine
  error.rs             stable application errors
  domain/
    paper.rs           physical cells, stacks, and coordinates
    fold.rs            legal fold descriptions and transitions
    ink.rs             brush definitions and ink application
    puzzle.rs          validated puzzle contracts
    replay.rs          deterministic action records
    score.rs           solution comparison
  solver/
    mod.rs             bounded search interface
    state.rs           canonical search representation
  generator/
    mod.rs             deterministic candidate generation
  storage/
    mod.rs             storage interface and lifecycle
    sqlite.rs          SQLite implementation
    migrations.rs      ordered schema changes
  packs/
    mod.rs             pack validation and installation
    archive.rs         bounded archive handling
  tui/
    mod.rs             terminal lifecycle
    event.rs           bounded input and tick events
    update.rs          state transitions
    view.rs            rendering
    style.rs           terminal capability styles
puzzles/               built-in puzzle sources
tests/                 boundary and shipped-binary integration tests
scripts/               release and package metadata tooling
```

### Dependency posture

Likely dependencies include:

- `ratatui` for rendering
- `crossterm` for portable terminal input and lifecycle
- `serde` and `toml` for explicit puzzle and configuration formats
- `rusqlite` with a deliberate SQLite linkage policy
- `thiserror` for typed boundary errors
- `anyhow` only at executable orchestration boundaries
- `clap` if its maintenance and binary cost beat a small manual CLI
- A deterministic random generator with stable seed behavior

Property testing, fuzzing, archive support, and release tooling may add
development dependencies after their value and bounds are documented. Do not
add an async runtime or network client for v1.

`deny.toml` began with only the project license. Each dependency license
requires review and an explicit allowlist entry. Do not add a skip or exception
merely to make the audit pass.

### State ownership

- `App` owns the current screen, loaded puzzle, attempt, dialogs, and terminal
  preferences.
- The domain engine owns paper validity and deterministic transitions.
- The TUI translates input into domain actions and renders resulting state. It
  does not reproduce fold logic.
- Storage persists completed operations. It does not decide game policy.
- The solver receives immutable validated puzzle state and a cancellation
  signal. It cannot mutate the live attempt.
- A generator owns its seed and attempt budget. It cannot read wall-clock time
  after construction.

### Core data representation

The paper model follows the operations the engine performs most often:
fold every physical cell, inspect one visible stack, apply ink by physical cell
ID, compare every original cell, copy bounded undo state, and produce a stable
solver key. A board contains at most 144 physical cells, so predictable dense
passes are preferable to pointer-heavy structures.

- A validated paper assigns each physical cell one stable dense `CellId` in
  original row-major order. The ID intentionally indexes dense storage, and
  array reordering never changes identity.
- The live paper has one canonical dense collection indexed by `CellId` for
  current placement, layer, face, and orientation. Ink and target membership use
  compact bit sets indexed by the same ID.
- Coordinates and layer positions have one mutable source of truth. Rendering,
  brush lookup, and fold planning derive bounded stack views into reusable
  scratch storage. They do not maintain a second mutable map of stacks.
- Undo begins with complete canonical snapshots in a bounded history. The
  prototype measures their worst-case size before considering action deltas.
  Exact snapshots are preferred while they fit the ordinary-play memory budget.
- The solver derives a compact immutable search key from canonical paper state.
  The key is not a second rules engine, and solver results are replayed through
  production transitions.
- Secondary indexes or caches require a named dominant lookup, an update or
  rebuild rule, and measurement showing that a dense scan is insufficient.

The prototype records the dominant operations and scale, the selected
representation, the strongest rejected alternative, and the invariants and
measurements that justify the choice. The likely rejected baseline is a map from
coordinates to separately allocated stacks: it makes lookup direct but makes
folds, snapshots, hashing, and solver copies more expensive and easier to
desynchronize. The production engine keeps this representation after the
prototype measurements and invariant tests accepted it.

## Local storage

The default database lives in the operating system's user data directory under
an `orifude` directory. Tests inject an isolated path. The application must not
write into the current working directory unless an explicit export command
names a destination.

The native path mapping follows the operating system conventions exposed by
`directories` 6.0. Linux uses `$XDG_DATA_HOME/orifude`,
`$XDG_CONFIG_HOME/orifude`, and `$XDG_CACHE_HOME/orifude`, with the standard
`~/.local/share`, `~/.config`, and `~/.cache` fallbacks. macOS uses
`~/Library/Application Support/orifude` for data and configuration and
`~/Library/Caches/orifude` for cache data. Windows uses
`%APPDATA%\orifude\data`, `%APPDATA%\orifude\config`, and
`%LOCALAPPDATA%\orifude\cache`. The database, ownership lock, managed packs,
and fixed staging directory live under the data path.

The database stores:

- Schema metadata
- Settings
- Journey progress
- Daily history
- Completed attempts
- Best solutions
- Bounded replay history
- Installed pack registry and content fingerprints
- One bounded pending pack-installation journal

Built-in puzzle content remains in version-controlled files and is embedded or
packaged with the binary. The database does not become the source of truth for
official puzzle definitions.

Startup loads bounded installed-pack catalog metadata from SQLite, not every
community puzzle file. Orifude verifies the selected pack against its recorded
fingerprint before loading it for play and keeps at most one community pack
loaded at a time. A mismatch disables that pack with a recovery message. The
loaded-pack cache is discarded on removal, replacement, or process exit.

### Storage behavior

- Open one database connection for the single-process application unless
  measurement proves a different need.
- Use parameterized SQL only.
- Apply migrations before entering the interactive terminal.
- Make migrations transactional where SQLite permits it.
- Back up the existing database before a destructive format change.
- Return a clear recovery path for corrupt or unsupported databases.
- Never silently reset progress after an open or migration failure.
- Prevent two Orifude processes from racing writes to the same database.
- Commit one logical progress update in one transaction.
- Flush durable progress before reporting a completed puzzle as saved.

## Puzzle and pack format

Puzzle files use a human-readable, versioned TOML format. A puzzle declares:

- Format version
- Stable pack-scoped ID
- Title and optional short description
- Width and height in cells
- Target ink grid
- Allowed creases and fold directions
- Allowed brushes and stroke lengths
- Fold and stroke budgets
- Optional par score
- Optional tutorial cues
- Optional author and license metadata

Pack and puzzle IDs contain 1 to 64 ASCII bytes. They use lowercase letters and
digits separated by single hyphens, and they begin and end with a letter or
digit. Unicode belongs in display text, not identity or filesystem keys. IDs
are logical keys and are never used directly as managed directory names.

Pack-relative directory names and file stems follow the same lowercase ASCII
segment grammar as IDs. The schema declares every allowed directory and file
extension. Validation rejects empty components, case-insensitive duplicates,
Windows device names, trailing dots or spaces, and any name outside the
component and relative-path bounds. Managed directories use an opaque
installation ID or content fingerprint generated by Orifude.

The content fingerprint is SHA-256 over the pack format version and every
validated, `/`-separated relative path and file byte sequence in sorted path
order. Lengths frame each value so different path and content pairs cannot
concatenate to the same input. Archive timestamps, permissions, entry order,
and container format do not affect the fingerprint.

Validation runs before domain construction and returns all safely collectable
errors up to a fixed error-count limit. A pack with one invalid required puzzle
does not partially install.

Pack installation follows this order:

1. Resolve the explicit local source.
2. Inspect source type without trusting the filename extension alone.
3. Enforce compressed size, extracted size, file count, path depth, and name
   limits.
4. Reject absolute paths, parent traversal, devices, sockets, symlinks, and hard
   links.
5. Copy or extract into the one fixed private staging directory on the same
   filesystem as managed pack storage.
6. Parse and validate metadata and every puzzle.
7. Calculate a content fingerprint and reject conflicting pack IDs.
8. Commit one non-playable pending-install record containing the generated final
   name, pack ID, fingerprint, and installation timestamp.
9. Atomically rename the accepted staging directory to its final managed name.
10. In one SQLite transaction, register the installed pack and remove its
    pending-install record.
11. Clean temporary content on every outcome that does not leave a recorded
    recovery operation.

The installed-pack registry is the source of truth for playable packs. A
pending pack never appears in play, author commands, or pack listings. Startup
reconciles the one fixed staging directory and single pending operation before
loading packs. A pending record with its final directory completes the registry
transaction. A pending record with only staging content rolls back. Staging
content without a pending record is also removed. A cleanup failure leaves the
same state for one bounded retry at the next startup. Recovery never uses an
unbounded retry loop and never removes a source directory supplied by the user.
If a completed registry transaction survives but its managed directory does
not, startup removes the unplayable registry row and preserves its progress.
Unregistered entries under the private managed-pack root are orphaned content
and are removed without following links.

## Explicit v1 bounds

These are initial hard limits. Work may lower them. Raising one requires a
resource estimate, tests, and a documented decision here.

| Resource | Limit |
| --- | --- |
| Board width | 4 to 12 cells |
| Board height | 4 to 12 cells |
| Physical cells | 144 |
| Fold actions in one attempt | 12 |
| Brush actions in one attempt | 8 |
| Line brush length | 2 to 12 cells |
| Allowed fold rules in one puzzle | 44 |
| Allowed brush rules in one puzzle | 23 |
| Total replay actions | 64 |
| Undo history states | 64 |
| Puzzle file size | 64 KiB |
| Pack metadata file size | 32 KiB |
| Plain-text note file size | 16 KiB |
| Pack ID | 1 to 64 ASCII bytes |
| Puzzle ID | 1 to 64 ASCII bytes |
| Puzzles in one pack | 128 |
| Files in one pack | 256 |
| Installed community packs | 32 |
| Community packs loaded for play | 1 |
| Pending pack installations | 1 |
| Managed installed-pack content | 512 MiB |
| Archive compressed size | 8 MiB |
| Archive extracted size | 16 MiB |
| Archive path depth | 4 components |
| Pack path component | 80 ASCII bytes |
| Pack relative path | 128 ASCII bytes |
| Display title | 80 Unicode scalar values |
| Display description | 512 Unicode scalar values |
| Validation errors returned | 32 |
| Solver visited states | 250,000 |
| Solver memory budget | 128 MiB |
| Generator candidate attempts | 512 |
| TUI event queue | 256 events |
| Animation refresh rate | 30 frames per second |
| One reveal animation | 1,200 milliseconds |
| Recent replays per puzzle | 20 |
| SQLite main database file | 128 MiB |

The solver must stop before breaching its visited-state or memory budget. The
SQLite limit applies to the main database file. Persistence fixes the page size
before schema creation and enforces the corresponding `max_page_count` on every
writable connection. A transaction that would grow the main file past 128 MiB
fails without exposing partial progress.

SQLite capacity is the configured maximum page count minus allocated non-free
pages, using `page_count` and `freelist_count`. A nonessential transaction must
leave at least 16 MiB of that capacity available. Before retaining a replay or
history write that would cross the reserve, one bounded pruning batch removes
superseded non-best replays and other nonessential history in the same
transaction. A standalone nonessential write rolls back and reports the
storage limit when pruning cannot restore the reserve. A completion still
commits its protected progress and best solution; it discards only the new
non-best replay when necessary. Settings, completion state, best solutions,
and migrations may use the reserve, but a protected write that would exceed
the hard maximum also leaves the prior state unchanged and reports the limit.
Protected data is not a promise of unbounded growth.

Physical storage accounting records the main file and any journal, WAL,
shared-memory, or temporary sidecar by actual file length. The 128 MiB limit is
for the main file, not a claim about transient SQLite space. Before persistence
work closes, the project chooses a journal mode, checkpoint policy, retained
journal limit, and a separate hard budget for worst-case transaction sidecars.
The selected `DELETE` journal mode disables cache spilling, retains no journal
after commit, and has a 132 MiB sidecar budget. With spilling disabled, one
transaction has one header of at most 64 KiB and at most one 4,104-byte journal
record for each of the 32,768 database pages.

The installed-pack content ceiling includes final and staging content.
Installation checks the candidate's validated extracted size before committing
the pending record. Removing an installed pack frees its managed content but
preserves the bounded progress records required to explain saved history.

## Required invariants

The engine design must make these properties explicit and testable:

- Every physical cell ID is unique.
- A valid paper always contains exactly its initial physical-cell count.
- Every physical cell appears in exactly one stack position.
- Layer order within each stack is total and contains no duplicate position.
- Fold input is validated before mutation.
- A failed action leaves the prior state unchanged.
- Undo restores byte-equivalent canonical domain state.
- Ink affects only physical cells in the selected brush footprint and stack.
- Puzzle comparison considers every original cell exactly once.
- Replay execution is deterministic across supported platforms.
- Generator output always passes the same validator used for external puzzles.
- A solver-reported solution succeeds when replayed by the production engine.
- Storage transactions never expose half-completed progress.
- Installing an invalid pack leaves no registered or playable partial pack.
- A pack is playable only when its complete final directory matches one
  installed registry record. Staging and pending packs are never playable.
- Once filesystem and SQLite operations succeed again, pack reconciliation
  converges to one complete registered pack or no managed copy for that
  operation.
- Terminal teardown runs after normal quit and recoverable failure.

Assertions may enforce internal programmer-error invariants. External files,
terminal behavior, filesystem failures, SQLite errors, cancellation, and
resource exhaustion use normal error handling.

## Performance budgets

The budgets apply to a release build on a documented reference machine:

- Show the first usable local screen within 250 milliseconds when no migration
  is required.
- Keep idle CPU below 1 percent after terminal activity settles.
- Process an ordinary key event and render within one 30 Hz frame budget.
- Keep ordinary play below 64 MiB resident memory.
- Keep bounded solver work below 128 MiB resident memory.
- Keep normal database writes below 50 milliseconds at the 95th percentile on
  local SSD storage.
- Keep the stripped compressed archive small enough to remain a practical
  command-line download. Record the measured size before release rather than
  inventing a marketing number.

Performance completion requires reproducible measurement. Estimates guide the
design, but they do not count as evidence.

The solver's two limits must hold independently. Dividing 128 MiB by 250,000
visited states leaves at most 536 bytes per state before accounting for the
frontier, collection metadata, allocator overhead, and cancellation state. The
solver therefore measures or conservatively accounts for total allocated bytes
and may reach its memory limit before its visited-state limit.

## Toolchain and automation

Orifude pins Rust `1.98.0`, the current stable release approved for the
repository foundation. `rust-toolchain.toml` is the authoritative exact
toolchain selection. It uses the minimal rustup profile and includes `rustfmt`
and `clippy`. `Cargo.toml` declares `rust-version = "1.98"` as the package's
minimum compiler version. Updating either value is a reviewed compatibility
change, not an automatic moving-stable update.

While the two versions match, ordinary CI is also the minimum-version build. If
the exact toolchain advances beyond `rust-version`, that same change adds a
separate CI build with the declared minimum. A newer compiler build is not MSRV
evidence.

`mise.toml` is the command interface for development, CI, QA, and release work.
It does not declare a second Rust version. Rustup reads `rust-toolchain.toml`,
and a job installs only the extra compilation targets it needs. Documented
commands and automation call mise tasks rather than copying Cargo command lines
into the README or workflows.

The repository foundation provides these tasks:

- `mise run run [ARGS]` runs the development binary and forwards its arguments.
- `mise run fmt` applies Rust formatting.
- `mise run fmt-check` checks formatting without changing files.
- `mise run lint` runs Clippy with warnings denied.
- `mise run test` runs the locked test suite.
- `mise run doctest` runs locked documentation tests.
- `mise run build` creates the locked release build.
- `mise run audit` runs bounded dependency advisory and license checks.
- `mise run check` composes the non-mutating format, lint, test, doctest,
  dependency-audit, and release-build tasks used by ordinary CI.

Bounded `test-native` and `release-check` tasks are added when their
corresponding behavior exists. Complex packaging logic may live in a reviewed
script, but mise remains its public entry point. Cargo commands that resolve or
build dependencies use `--locked` after `Cargo.lock` is committed.
Within `check`, Cargo tasks that share one target directory run in a deliberate
sequence so they reuse build output instead of contending for Cargo's target
lock. Independent work may run concurrently only when measurement shows a
benefit and the resulting failures remain clear.

CI grows with the behavior it can verify:

- The repository foundation creates ordinary CI. Every pull request and push
  to `shrek` runs `mise run check` once on Linux x86_64.
- The `shrek` branch has no branch-protection rule and accepts direct pushes.
  Ordinary CI reports the result after a push; it does not block the update.
  Pull requests remain available but are optional.
- The terminal foundation adds a small native smoke matrix on Linux x86_64,
  macOS Apple Silicon, and Windows x86_64. It exercises terminal startup and
  restoration on pull requests without duplicating the complete test suite.
- The complete playable loop adds `mise run test-native`. The full supported
  operating-system and architecture matrix runs on `shrek`, manual
  dispatch, and each release candidate rather than on every source edit.
- Release hardening runs the recorded Linux distribution QA and the complete
  native matrix against release binaries. Environments unavailable as hosted
  runners use the same mise task on a designated native host.
- Release publication is a separate workflow. It can use credentials only for
  an approved tag and commit whose ordinary and native checks passed.

Ordinary CI has read-only repository permissions, receives no publication
secrets, cancels superseded runs for the same branch or pull request, and gives
every job an explicit timeout. Jobs may run concurrently, but each result stays
independent and visible. Third-party setup actions use reviewed immutable
revision pins, and CI installs an exact reviewed mise release. Cross-run build
caching starts disabled; it is added only when timings show a useful reduction
and the key isolates the operating system, target, exact toolchain, and
lockfile. Release jobs never consume untrusted cached executables.

## Security model

Orifude has no remote service, yet it still processes untrusted local content
and interacts with terminals, filesystems, databases, archives, CI, and package
repositories.

### Assets to protect

- The user's files outside Orifude-managed directories
- Saved progress and best solutions
- Terminal integrity and restored terminal state
- Release credentials and package repository access
- Published artifact integrity
- Contributor machines running tests or content validation

### Expected attackers and failures

- A malicious or malformed puzzle pack
- A crafted archive attempting path escape or resource exhaustion
- Display text containing terminal escape sequences
- A compromised dependency or CI action
- A corrupted or incompatible SQLite database
- A partial write, process interruption, or full disk
- A release artifact replaced or mismatched after metadata generation

### Required controls

- Positive bounds and syntax validation at every content boundary
- No executable pack content or dynamic native plugins
- Terminal control-character rejection for external display text
- Safe archive extraction with path and type checks
- Parameterized SQL and transaction-owned progress updates
- Owner-scoped local files using platform-appropriate permissions where
  available
- No secrets in source, logs, artifacts, terminal recordings, or test fixtures
- Locked dependencies and dependency advisory review
- Immutable revision pins for third-party CI actions
- Least-privileged release credentials scoped to the required repositories
- SHA-256 checksums covering every release archive
- Clean-machine installer and package verification

Security reviews must state scope, evidence, exclusions, and residual risk. The
project will not claim that a scan made it secure.

## Test strategy

### Unit tests

Unit tests protect focused domain behavior:

- Coordinate and stack construction
- Each legal fold direction
- Illegal crease and out-of-area rejection
- Layer-order reversal
- Dot and line brush footprints
- Missing and extra ink comparison
- Score ordering
- Undo and reset
- Limit enforcement
- Stable typed errors

### Property tests

Property tests cover broad state spaces with deterministic recorded seeds:

- Legal actions preserve physical-cell count and identity.
- Fold followed by undo restores canonical state.
- A replay matches direct action execution.
- Serialization and parsing preserve accepted puzzle meaning.
- The solver never reports an invalid solution.
- Generated puzzles pass validation and replay.
- Failing actions do not mutate state.

Every property run has explicit case, shrink, input-size, operation, and time
budgets.

### Integration tests

Integration tests exercise real boundaries:

- SQLite creation, migration, rollback, corruption reporting, and pruning
- Atomic progress persistence across restart
- Isolated filesystem pack installation, interruption recovery, and removal
- Archive rejection for traversal, links, excessive size, and excessive count
- Stable pack fingerprints across entry order, timestamps, permissions, and
  supported source containers
- Puzzle and replay compatibility failures
- Terminal capability selection and size changes
- Release metadata against actual archives and checksums

### End-to-end tests

A small suite launches the shipped binary and checks:

- First launch and lesson
- One complete journey puzzle
- Undo, reset, preview, solve, save, quit, and restart
- Daily puzzle stability for an injected date
- Small-terminal resize recovery
- Malformed pack reporting without terminal corruption
- Author command exit status, stdout, and stderr
- Version and help output

Native end-to-end journeys run on every supported operating system. Tests own
their temporary files and processes and clean them on success, failure, timeout,
and cancellation.

### Fuzzing

Bounded fuzz targets cover:

- Puzzle TOML parsing
- Pack metadata parsing
- Replay parsing
- Archive entry validation
- Domain action sequences

Useful failures become small deterministic regression tests. CI runs a short
fixed-budget corpus pass. Longer campaigns run separately and never contact
external services.

## QA contract

Before a user-visible checklist item closes, QA records:

- Commit and dirty state
- Release or development profile
- Rust toolchain
- Operating system and architecture
- Terminal and color mode
- Exact commands and user actions
- Fixture or seed
- Expected and actual result
- Saved-state and filesystem effects
- Cleanup result
- Untested areas and residual risk

Release verdicts use `PASS`, `PASS WITH KNOWN ISSUES`, `FAIL`, `BLOCKED`, or
`INCONCLUSIVE`. The separate recommendation is `ship`, `hold`, or
`no recommendation`.

## Distribution contract

### Canonical artifacts

GitHub Releases is the canonical binary source. A complete release contains:

- Linux x86_64 musl archive
- Linux ARM64 musl archive
- macOS x86_64 archive
- macOS ARM64 archive
- Windows x86_64 archive
- One complete SHA-256 checksum file
- POSIX installer generated after the archives and pinned to the release
- PowerShell installer generated after the archives and pinned to the release
- Release notes and changelog link

Release tags use complete semantic versions such as `v1.0.0`.

The release workflow creates a draft, uploads every finished asset, verifies the
whole set, and then publishes it as an immutable GitHub release. A broken
published asset requires a new patch release. It must never be replaced under
the old version.

### Installer trust

The checksum file remains useful for manual downloads and package metadata, but
an installer must not treat a checksum downloaded beside the archive as its
only expected value. Anyone able to replace both files could make them agree.

Each release-specific installer embeds the expected SHA-256 checksum for every
archive it supports. The release process generates both installers from the
completed archive manifest after all archive hashes are known. A release check
then compares the embedded values with the checksum file and the actual bytes.

The embedded values protect against a corrupt download, the wrong archive, and
an archive changed independently of the installer. They do not help if an
attacker can replace the installer itself. For that reason, the website links to
the installer attached to an exact immutable GitHub release. The release
attestation gives users with GitHub CLI a separate way to verify the release and
downloaded asset.

The website presents separate commands to download the installer to a named
file, inspect it, and run it only after the transfer succeeds. It must not
publish pipe-to-shell or pipe-to-`Invoke-Expression` commands. A failed transfer
never invokes an interpreter or changes the installation destination.

### Classic POSIX installer

The POSIX installer must:

- Support the declared Linux and macOS architectures.
- Download the release-specific script and archive only from immutable GitHub
  release URLs.
- Select an exact version rather than silently changing during execution.
- Embed the expected checksum for each supported archive.
- Verify the selected archive against its embedded checksum before extraction.
- Optionally compare the checksum file for consistency, but never use that
  downloaded file as the sole expected value.
- Use `curl` in failure-reporting mode, allow HTTPS only for the initial request
  and redirects, and require TLS 1.2 or newer where the installed curl supports
  those controls.
- Finish downloading the installer before execution in the documented default
  command.
- Use a private temporary directory and clean it on every exit.
- Refuse unknown operating systems and architectures.
- Never run `sudo` without an explicit user choice.
- Install into an explicit destination and explain PATH changes.
- Preserve an existing binary until the new binary verifies.
- Provide a documented noninteractive mode with bounded inputs.

### PowerShell installer

The PowerShell installer must:

- Enable terminating error behavior for every required operation.
- Download only immutable GitHub release URLs over HTTPS.
- Finish downloading the installer before execution in the documented default
  command, and never pipe network output into `Invoke-Expression`.
- Embed the expected checksum for each supported Windows archive.
- Verify the selected archive against its embedded checksum before extraction.
- Optionally compare the complete checksum file for consistency, but never use
  that downloaded file as the sole expected value.
- Use a private temporary directory and remove it in `finally` cleanup.
- Refuse unsupported Windows architectures.
- Replace an existing binary only after verification and extraction succeed.
- Explain the user-scoped installation directory and PATH behavior.
- Avoid profile modification unless the user explicitly requests it.
- Support a documented noninteractive invocation.

### Homebrew

- Publish one formula named `orifude` to the existing GitHub repository
  `nuggocto/homebrew-tap`.
- Support macOS only. Do not advertise or test the formula as Linuxbrew.
- Include Intel and Apple Silicon release artifacts.
- Pin immutable release URLs and SHA-256 checksums.
- Include a `test do` block that runs the installed binary's version output.
- Update the tap only after the GitHub release exists and its archives pass
  clean installation checks.

### Scoop

- Publish one manifest named `orifude` to the existing GitHub repository
  `nuggocto/scoop-bucket`.
- Support verified Windows architectures only.
- Pin immutable release URLs and SHA-256 checksums.
- Expose the `orifude` binary without running install-time scripts from the
  archive.
- Update the bucket only after the GitHub release and clean PowerShell install
  pass.

### AUR

- Publish exactly one AUR package named `orifude-bin`.
- Do not publish `orifude`, `orifude-git`, or another AUR variant for v1.
- Download prebuilt Linux archives from the immutable GitHub release.
- Verify upstream SHA-256 checksums in `PKGBUILD`.
- Generate and review `.SRCINFO` from the final `PKGBUILD`.
- Use the release operator's dedicated AUR SSH identity `aur@sshmoi.com`.
- Never store the private SSH key in either Orifude repository, artifacts,
  workflow logs, or package metadata.
- Confirm the official AUR remote, supported architectures, and account access
  with a read-only or dry-run check before the first publication.

## Frontend contract

`orifude-front` is a separate Astro static site deployed on Cloudflare Pages.
It presents Orifude but never runs the puzzle engine.

The public site remains deliberately small:

- `/` is the landing page.
- `/changelog/` is the styled release changelog.
- Unknown paths return a real not-found response or static not-found page.

### Landing page

The landing page contains:

- The real Orifude wordmark and squirrel-courier artwork
- A short introduction written in the same quiet voice as the game
- A direct description of the terminal puzzle
- A short visual fold, ink, and unfold explanation
- One reviewed terminal recording or still image
- A visible project status
- A link to `https://github.com/nuggocto/orifude`
- Release and installation links only after verified artifacts exist
- A link to the changelog
- Apache-2.0 license and source attribution

It must not contain a browser game, account form, newsletter form, analytics,
tracking pixel, cookie banner, fake terminal interaction, or unavailable
download button.

The mechanic explanation uses one reviewed static sequence: fresh paper and
target, the chosen fold, ink passing through the resulting stack, and the final
unfolded comparison. Short captions explain the crease, moving side, stack
order, and exact-match rule. The sequence must remain understandable without
animation, JavaScript, color, or physical-cell IDs. It explains the native game
but does not imitate an interactive terminal.

### Changelog page

The changelog page presents one section per published release. Each release
contains:

- Semantic version
- Publication date
- Short human summary
- Added, changed, fixed, and security notes when present
- Direct GitHub release link
- Direct link to the corresponding source tag
- Supported installation channels for that release

The design may use folded-paper cards, ink strokes, branch growth, and the
project palette. Semantic headings and chronological navigation must remain
clear without CSS or animation. New releases appear through reviewed structured
data or generated content tied to the canonical main-repository changelog.

### Frontend constraints

- Build to static files with Astro.
- Keep JavaScript optional and minimal.
- Bundle fonts and assets. Do not depend on external font or script CDNs.
- Optimize the supplied PNG artwork into appropriate responsive web formats.
- Respect reduced motion.
- Meet keyboard, focus, reflow, contrast, and accessible-name checks.
- Set canonical metadata, social metadata, sitemap, robots policy, favicon, and
  a real 404 path.
- Set restrictive static security headers suitable for the final assets.
- Keep Cloudflare Pages as the only hosting dependency.
- Verify the production domain and `www` redirect after deployment.

## Build plan

### Phase 0, product contract

Goal: settle what Orifude is before building the Rust application.

- [x] Keep the product name `Orifude`.
- [x] Define Orifude as a TUI-only folding-and-ink puzzle.
- [x] Choose Rust for the native application.
- [x] Require complete offline play.
- [x] Choose local SQLite for progress and replays.
- [x] Exclude accounts, telemetry, multiplayer, and a web game.
- [x] Keep Cloudflare Pages only for the static frontend.
- [x] Record the supplied artwork as the visual source.
- [x] Record the coined-name language and cultural restraint.
- [x] Record the v1 product and engineering specification in `PROJECT.md`.
- [x] Record permanent agent and engineering rules in `AGENTS.md`.
- [x] Define GitHub archives, classic installers, Homebrew, Scoop, and
  `orifude-bin` as release channels.
- [x] Define the landing and changelog pages for `orifude-front`.
- [x] Review the complete specification with the project owner.
- [x] Resolve or record every requested product change from that review.
- [x] Mark the product contract accepted before foundational implementation.

Exit gate:

- [x] The owner accepts the v1 scope, non-goals, core rules, release channels,
  frontend boundary, and initial resource limits.

### Phase 1, repository foundation

Goal: create a reproducible Rust application base with strict local and CI
checks before domain code arrives.

- [x] Add `rust-toolchain.toml` pinning Rust `1.98.0` with the minimal profile,
  `rustfmt`, and `clippy`.
- [x] Declare `rust-version = "1.98"` in `Cargo.toml` and verify it remains the
  intended minimum compiler version.
- [x] Confirm Rust edition 2024 and the matching Cargo resolver behavior.
- [x] Encode the approved operating-system, architecture, minimum-version, and
  Rust target matrix in build and test configuration.
- [x] Add `.gitattributes` with explicit text, binary, and cross-platform
  line-ending rules.
- [x] Commit `Cargo.lock` and document the lockfile policy.
- [x] Add project metadata, license metadata, repository URL, and binary target
  to `Cargo.toml`.
- [x] Add strict but practical Rust and Clippy lint policy.
- [x] Deny unsafe code at the crate boundary.
- [x] Define debug, test, and release profile choices, including overflow and
  panic behavior.
- [x] Establish the single-package module layout without empty abstraction
  layers.
- [x] Add typed top-level error handling and stable exit statuses.
- [x] Add `--help` and `--version` behavior.
- [x] Add formatter, Clippy, test, doctest, and locked build commands.
- [x] Add `mise.toml` as the command interface and compose the full local check
  from small reusable tasks.
- [x] Add bounded dependency license and advisory checks through
  `mise run audit`.
- [x] Add ordinary CI that runs `mise run check` on Linux x86_64 for every pull
  request and push to `shrek`.
- [x] Give ordinary CI read-only permissions, no publication secrets, bounded
  job timeouts, and concurrency cancellation for superseded runs.
- [x] Pin third-party CI actions to reviewed immutable revisions.
- [x] Pin the mise release installed by CI and document the supported local
  mise version.
- [x] Confirm `shrek` remains the GitHub default branch, accepts direct pushes
  without branch protection, and runs ordinary CI after each push.
- [x] Measure ordinary CI before adding any cross-run build cache.
- [x] Document rustup, mise, supported local tools, and the mise commands.
- [x] Replace the placeholder README with the current product description and
  development commands.
- [x] Verify a clean checkout without relying on undeclared global state.

Verification record (2026-09-01): signed commit `cdc2b22` passed task
validation and `mise run check` locally and from a fresh clone with isolated
Rust, Cargo, mise, and cache directories. The first cache-free GitHub Actions
run completed its `ubuntu-24.04` `check` job in 15 seconds; tool installation
took about 10 seconds and the repository check took about 1.5 seconds. At that
verification point, `shrek` was the default branch, required the strict GitHub
Actions `check`, and disallowed force-pushes and deletion. The owner later
removed branch protection. A live GitHub API check on 2026-09-01 confirmed that
`shrek` remains the default branch, GitHub reports it as unprotected, and no
branch rules apply. Ordinary CI still runs after each push.

Exit gate:

- [x] A clean checkout formats, lints, tests, and builds with the declared
  toolchain and locked dependencies.

### Phase 2, paper model and rule prototype

Goal: prove that folding stacked terminal cells is understandable, deterministic,
and small enough before building the full application around it.

Prototype decision record (2026-09-01):

- The dominant operations are bounded passes over every physical cell, deriving
  one visible stack, applying ink by stable identity, comparing all original
  cells, copying undo state, and copying future solver candidates. The largest
  paper has 144 physical cells. Fold and solver work copy whole states more
  often than rendering asks for one stack.
- The selected model stores one `PhysicalCell` value per stable row-major
  `CellId` in a dense vector. Each value owns its current coordinate, layer,
  face, and orientation. A fixed three-word bit set stores ink by the same ID.
  Caller-owned fixed scratch storage derives bottom-to-top stack views. No
  coordinate map or stack cache can become a second mutable source of truth.
- The strongest rejected alternative is a `BTreeMap` from coordinates to
  separately allocated stack vectors. Its direct stack lookup is faster, but
  it scatters one maximum paper across 144 small allocations and makes every
  snapshot copy the tree and its vectors. The checked-in measurement example
  implements this alternative and compares its state with production folds in
  every direction and every valid two-fold direction pair before timing it.
- Complete canonical snapshots remain the undo model. The measured lower bound
  for 64 maximum-paper snapshots is about 49 KiB, so action deltas would add
  failure paths without solving a resource problem.

Coordinates start at row zero and column zero in the top-left corner. A crease
value counts the cells above or left of that boundary. For a vertical crease
`c`, a moved column becomes `2c - 1 - column`. For a horizontal crease, a moved
row becomes `2c - 1 - row`. Left folds move columns at or right of the crease,
right folds move columns left of it, up folds move rows at or below it, and down
folds move rows above it.

`Orientation` records the screen direction of the physical cell's original
north edge. Every fold flips the visible face. A vertical fold swaps east and
west orientations while leaving north and south unchanged. A horizontal fold
swaps north and south while leaving east and west unchanged. If a moving stack
has `m` layers, a cell at old layer `l` lands at layer
`stationary_layers + (m - 1 - l)`. Stationary layers remain below it. When the
destination is empty, `stationary_layers` is zero.

The verification plan combines release-active invariant assertions with tests
through the public domain API. Assertions check cell conservation, dense
identity, in-bounds coordinates, action-count agreement, and one complete
zero-based layer order per stack. Tests cover malformed dimensions, creases,
budgets, empty positions, empty moving sides, all four directions across every
supported even board size, two-fold reversal, moved-only stacks at empty
destinations, exact ink comparison, failed-action immutability, and exact undo.
The layer-reversal test was also run once against an intentionally disabled
reversal and failed with the expected `[5, 6, 9, 10]` versus
`[5, 6, 10, 9]` difference.

Release measurements ran on an AMD Ryzen AI Max+ 395, x86_64 Arch Linux, and
Rust 1.98.0. Five processes each collected 25 alternating sample blocks through
`mise run paper-measure`. These are directional microbenchmarks rather than a
portable latency promise:

| Maximum 12 by 12 paper | Dense state | Coordinate map |
| --- | ---: | ---: |
| Representation lower bound | 781 bytes | 4,240 bytes plus B-tree node overhead |
| Snapshot clone median across runs | 13 to 14 ns | 2,460 to 2,579 ns |
| One stack lookup median across runs | 111 to 115 ns | 5 ns |
| Clone and apply two folds median across runs | 2,173 to 2,232 ns | 13,742 to 14,217 ns |

On this target a physical-cell value is 5 bytes and a 144-cell dense payload is
720 bytes. A complete snapshot's named payloads total 779 bytes. The solver
state has a 776-byte lower bound. A 64-action replay with maximum-length IDs and
the maximum rule collections has a 710-byte lower bound. Sixty-four
snapshot-and-action history entries have a 50,176-byte named-payload lower
bound. These values include the named payloads and collection headers but not
allocator bookkeeping. Rust does not promise this layout on every target.

The initial board, cell, action, and history bounds remain unchanged. Even the
maximum complete-snapshot estimate and a full grid of derived stack lookups sit
well below the ordinary-play memory and frame budgets. No canonical game rule
changed. The prototype follows the existing rule that a moved stack may become
the only stack at an empty destination inside the bounded working area.
Its temporary fold API deliberately accepts only creases at the original
horizontal or vertical midpoint. This is narrower than the production crease
contract; arbitrary puzzle-allowed cell boundaries remain engine work.

- [x] Define newtypes for physical cell ID, row, column, width, height, layer,
  fold count, stroke count, and action count.
- [x] Define canonical paper, physical-cell, stack, coordinate, face, and
  orientation representations.
- [x] Record the dominant paper operations and scale, the selected canonical
  representation, the strongest rejected alternative, and the verification
  plan.
- [x] Prototype a dense `CellId`-indexed state with derived stack views and
  compare it with a coordinate-to-stack map.
- [x] Document how vertical and horizontal folds transform coordinates,
  orientation, and layer order.
- [x] Implement construction of a validated rectangular paper.
- [x] Implement one vertical half-fold in a temporary domain prototype.
- [x] Implement one horizontal half-fold in the same model.
- [x] Implement a dot stamp through an overlapping stack.
- [x] Implement exact unfolded target comparison.
- [x] Implement exact undo for the prototype actions.
- [x] Prototype complete canonical undo snapshots and use action deltas only if
  measured size or copy cost requires them.
- [x] Add assertions for cell conservation, unique identity, and total layer
  order.
- [x] Add validation errors for malformed dimensions, creases, and budgets.
- [x] Render the prototype as plain text without committing to the final TUI.
- [x] Add a model-driven visual walkthrough that explains the crease, layer
  order, ink path, and answer format before asking for a prediction.
- [x] Create at least six small example puzzles that expose different layer
  orders and fold directions.
- [x] Run a paper prototype with real keyboard input and observe whether players
  can predict one-fold and two-fold results.
- [x] Estimate memory per cell, per state, per replay, and per solver candidate.
- [x] Revisit the initial bounds using prototype measurements.
- [x] Record any rule changes in the canonical game rules above.

Verification record (2026-09-01): the working tree based on `aaa348c` passed
`mise run check`, optimized tests for every local Cargo target, rustdoc with
warnings denied, and the locked dependency audit on x86_64 Arch Linux with Rust
1.98.0. A PTY run through `mise run paper` completed a two-fold prediction,
exact dot comparison, undo, and clean exit. The security review covered bounded
keyboard input, terminal output, integer arithmetic, allocation limits, unsafe
code, and the unchanged dependency graph. It found no remaining security issue
in that scope.
The adversarial implementation review found and fixed non-square coverage,
input-limit, menu-limit, and measurement-accounting defects before the final
run. A follow-up review reproduced unread oversized-input tails at both keyboard
prompts. Regression tests now prove that oversized input ends the exercise
without changing paper state or interpreting trailing bytes as later commands.
The owner review found that raw cell-ID stacks were understandable after an
explanation but too mathematical on their own. The exercise now begins with a
two-row fold, top-and-bottom stack labels, and a dot passing through both layers.
It then says to enter only the predicted top cell ID and shows `> 6` as the
answer for `[05<06]`.
Follow-up verification on the uncommitted working tree based on `9419443`
passed `mise run check`, release-mode example tests, and piped release-mode
sessions on x86_64 Arch Linux with Rust 1.98.0. The sessions covered the visual
walkthrough and numeric answer example, a correct two-fold prediction, immediate
quit, and oversized input. The walkthrough used ASCII only, stayed within 80
columns, and wrote no saved state or other files. A PTY run through
`mise run paper` confirmed the same walkthrough and a clean exit.
Native macOS and Windows behavior remains untested because this prototype does
not use platform terminal APIs. On 2026-09-01, the owner accepted the revised
visual explanation and dense paper model as sufficient for this prototype gate.
No separate external-player session was claimed; broader player observation
remains part of content and playable-loop review.

Exit gate:

- [x] The production model is chosen only after the prototype preserves every
  invariant and the fold result can be explained without reading code.

### Phase 3, deterministic game engine

Goal: turn the accepted model into the complete bounded rules engine used by
play, authoring tools, tests, and the solver.

- [x] Implement validated puzzle construction.
- [x] Implement every allowed horizontal and vertical crease and direction.
- [x] Reject empty, overlapping-invalid, out-of-area, over-budget, and
  disallowed folds without mutating state.
- [x] Implement dot brush behavior.
- [x] Implement bounded horizontal and vertical line brushes.
- [x] Implement missing-ink and extra-ink result sets.
- [x] Implement success and par evaluation.
- [x] Implement bounded action history, undo, and reset.
- [x] Define canonical state equality and hashing.
- [x] Define stable replay actions and compatibility metadata.
- [x] Implement replay validation and deterministic execution.
- [x] Define typed operational errors and programmer-error invariants.
- [x] Add focused unit tests for every public domain transition.
- [x] Add boundary tests at zero, one, maximum, and maximum plus one.
- [x] Add property tests for conservation, undo, replay, and failed-action
  immutability.
- [x] Make each new regression test fail for the expected reason before the fix
  is accepted.
- [x] Add a bounded domain-action fuzz target.
- [x] Review the engine for integer conversion, overflow, allocation, recursion,
  and panic paths.
- [x] Verify identical replay results in debug and release builds.

Verification record (2026-09-01): `mise run check`, release tests for every
target, and warning-denied Rust documentation passed on Rust 1.98.0 for Linux
x86_64. The 26 engine integration tests cover validated construction, every
fold direction, nested folds to eight layers, dot and line brushes, result
comparison, scoring, history, keys, replay compatibility, and atomic failures.
The exhaustive fold property covered all 2,268 board, direction, and crease
cases from 4 by 4 through 12 by 12; it needs no shrink step because it visits
the complete bounded range. Eight recorded seeds covered 256 direct actions and
fresh replay execution. Both property runs completed inside a separate
30-second review budget. Stable state hashes and replay outcomes matched in
debug and release profiles.

The fixed fuzz corpus and a 256-byte, 64-operation release run preserved cell,
layer, atomicity, and replay invariants. A 257-byte input stopped with status 2
before domain work. The keyboard exercise ran through folds, prediction, exact
ink comparison, undo, and clean exit using the production puzzle API. It used a
piped non-TTY with `TERM=xterm-ghostty`, true color available, and `NO_COLOR=1`;
it wrote no saved state or user files.

The adversarial review covered integer conversions, checked arithmetic,
allocation retention, recursion, panic paths, error reflection, and replay
isolation. It changed attempts to own their bounded validated puzzle, normalized
retained rule, replay, and solver collections to exact-length boxed slices,
canonicalized order-independent line endpoints, and corrected the memory
measurement for action-bearing history. A follow-up review bound every replay
to the exact validated gameplay revision and corrected the snapshot and replay
size accounting. Dependency policy checks passed and no
unsafe code, network access, parsing, paths, archives, SQLite, or terminal
control handling entered this engine surface. Puzzle and replay parsing and
native macOS and Windows runtime behavior remain later-phase verification
surfaces.

Exit gate:

- [x] A validated puzzle can be played entirely through the public domain API,
  and every required invariant has focused or property coverage.

### Phase 4, solver and deterministic generator

Goal: validate authored puzzles, find bounded solutions, and generate daily and
endless puzzles without unbounded search.

Implementation record (2026-09-01):

- The solver uses deterministic uniform-cost search. Its priority queue compares
  the same typed `Score` as the game, so folds come first and strokes break ties
  inside one fold count. A serial insertion number resolves equal scores. This
  is Dijkstra search without a heuristic, chosen because the action depth is at
  most 20 and measurements did not justify a puzzle-specific heuristic.
- Each visited entry owns the exact `PaperStateKey`, including dimensions,
  physical-cell placement, layer, face, orientation, ink, fold count, and stroke
  count. A separate compact node stores only its parent, action, depth, and
  score cost. Search restores one reusable attempt by replaying that parent path
  through production actions. It never stores a complete `Attempt` in every
  frontier entry.
- A `HashSet` answers membership but is never traversed. Queue insertion order,
  the canonical puzzle rules, and row-major action enumeration therefore
  determine output even though the standard hash table chooses a private hash
  seed. Cancellation is checked before setup, at every frontier pop, and before
  every candidate transition. One restore between checks contains at most 20
  bounded production actions.
- Visited states stop at 250,000, retained solver work stops at 128 MiB, and
  depth stops at the smaller of the caller limit and puzzle budgets. The memory
  check includes fixed setup, canonical key payloads, parent nodes, frontier
  entries, collection overhead, and a 512-byte per-state safety margin. On the
  measured 64-bit target, a maximum-paper state is charged 1,312 bytes, so the
  memory limit normally arrives before the numerical visited-state limit.
- Version 1 generation uses SplitMix64 wrapping arithmetic and multiply-high
  bounded index mapping. Daily seed text is exactly
  `orifude:1:YYYY-MM-DD`, hashed with FNV-1a into the versioned seed. The caller
  supplies a validated Gregorian date; generation never reads a clock.
- Candidate construction starts with a legal fold, uses a bounded configured
  action count, and finishes with ink. Successful production actions guarantee
  that built candidates are non-empty, initially unsolved, and within budget;
  those conditions are programmer-error assertions rather than retryable
  rejection states. Duplicate and trivial targets are rejected normally. The
  source action sequence is a solution witness, so a solver `Unsolved` result is
  also an invariant failure.
- Solver exhaustion rejects only the current candidate. Generation records the
  reason and tries the next candidate until the generation attempt limit is
  reached. Every accepted target passes `Puzzle::new`, receives the solver score
  as par, and replays the returned solution against that final exact puzzle
  revision.
- Generation performs at most 512 candidate attempts. Exhausted and cancelled
  results retain the exact versioned seed and counters needed to reproduce the
  run. Compatibility dispatch keeps the version 1 date and random path intact
  when a later generator is introduced. A fixed daily golden covers the seed,
  target cell IDs, selected actions, puzzle ID, and candidate number.

Release measurements ran on an AMD Ryzen AI Max+ 395, x86_64 Arch Linux, and
Rust 1.98.0. Five release processes each collected 25 sample blocks through
`mise run solver-measure`. These are directional measurements, not portable
latency promises:

| Representative solver work | Visited states | Checked actions | Median range |
| --- | ---: | ---: | ---: |
| One dot | 2 | 16 | 4.61 to 4.90 microseconds |
| One fold and one dot | 7 | 68 | 14.25 to 14.65 microseconds |
| Small unsolved target | 3 | 16 | 5.01 to 5.19 microseconds |
| Two-axis four-layer target | 35 | 798 | 180.87 to 184.30 microseconds |

The maximum-paper canonical key has a 776-byte named-payload lower bound. A
256-key `HashSet` lookup took 1.495 to 1.502 microseconds per membership across
the five process medians; a linear vector lookup took 11.682 to 11.809
microseconds. Hash storage used 209,408 named bytes versus 198,656 for the
vector, excluding hash control bytes and allocator bookkeeping. The selected
1,312-byte conservative state charge is about one thirteenth of the 16,750-byte
named-payload lower bound for cloning a maximum-rule `Attempt` with 20 history
entries. Replaying a 20-action parent path took 18.726 to 18.989 microseconds
versus 0.339 to 0.347 microseconds for cloning that complete attempt. The
selected representation spends bounded CPU to avoid multiplying full puzzle,
paper, and history ownership across the frontier.

Generation remains unsuitable for fold-free teaching puzzles, authored visual
compositions, story pacing, puzzles that require a unique solution, and broad
or deep rule sets that exhaust the solver. Line-heavy folded layouts can also
reject many candidates because a valid line may not cross an empty position.
Those classes remain handcrafted or require a later measured generator policy;
the bounded generator does not keep trying until a pleasing result appears.

- [x] Define the solver input, result, cancellation, limit, and exhaustion
  contracts.
- [x] Choose breadth-first, A-star, or another iterative search using measured
  state behavior rather than fashion.
- [x] Define a canonical search key that does not merge distinct meaningful
  states.
- [x] Compare visited-set and frontier representations using total bytes per
  entry, lookup cost, deterministic traversal, and cancellation behavior.
- [x] Enforce visited-state, memory, depth, and cancellation bounds.
- [x] Return solved, unsolved, exhausted, cancelled, and invalid distinctly.
- [x] Verify solver results by replaying them through the production engine.
- [x] Build a tiny brute-force reference for differential tests on small boards.
- [x] Add deterministic solver tests for shortest known solutions.
- [x] Add tests for bound exhaustion and cancellation.
- [x] Define a versioned stable seed and random generator policy.
- [x] Generate candidates from bounded valid action sequences.
- [x] Reject empty, already-solved, trivial, duplicate, and over-budget targets.
- [x] Run every candidate through the production validator and solver.
- [x] Enforce the generator attempt limit and report reproducible failure seeds.
- [x] Implement date injection for daily generation without reading time inside
  the generator.
- [x] Preserve daily results across generator compatibility changes.
- [x] Measure solver latency and memory on representative puzzle classes.
- [x] Tune search order and representation only from measurements.
- [x] Document puzzle classes that remain unsuitable for generation.

Verification record (2026-09-01): `mise run check`, release tests for every
Cargo target, warning-denied Rust documentation, the locked dependency audit,
and five release measurement processes passed on x86_64 Arch Linux with Rust
1.98.0. Six solver integration tests cover deterministic score order, exact
replay, horizontal and vertical line catalogs, independent brute-force
agreement, all result classes, every limit, and cancellation during expansion.
Eight generator integration tests cover Gregorian dates, fixed seed text,
reproducible acceptance, trivial and duplicate rejection, solver-exhaustion
recovery, production validation, exact final replay, attempt exhaustion with
its seed, cancellation, invalid configuration, configuration storage bounds,
and a complete version 1 daily golden. The implementation uses fixed-width
integer random operations, canonical action order, and no platform or clock
input.
Native terminal execution on macOS and Windows remains part of the later smoke
matrix because this work adds no terminal surface.

Bounded-configuration follow-up (2026-09-02): configuration now validates the
pack identity and the 44-fold and 23-brush collection limits before retaining
them. Accepted collections use exact-length boxed slices. The public regression
checks each limit-plus-one error. `mise run check`, release tests for every
Cargo target, warning-denied Rust documentation, ten repeated generator-suite
runs, the original oversized-input reproduction, and release-binary behavior
all passed on the recorded Linux and Rust environment.

Exit gate:

- [x] The solver never reports an invalid solution, generator work always ends
  within its bounds, and daily output repeats across supported platforms.

### Phase 5, persistence and local packs

Goal: save progress durably and accept community puzzle content without risking
the player's filesystem or terminal.

- [x] Choose and document platform-specific data, config, and cache paths.
- [x] Add the SQLite dependency with a deliberate linkage and update policy.
- [x] Define the initial schema for metadata, settings, progress, attempts,
  replays, daily history, pack registry, and the one-operation install journal.
- [x] Implement ordered, restart-safe migrations.
- [x] Implement single-process database ownership or an explicit lock.
- [x] Persist one completed attempt and its progress in one transaction.
- [x] Persist settings without coupling them to terminal rendering types.
- [x] Fix the SQLite page size and main-file `max_page_count`, then choose and
  test the journal mode, checkpoint policy, and hard transient-sidecar budget.
- [x] Enforce recent-replay and database growth policy.
- [x] Preserve best solutions during pruning.
- [x] Report full disk, read-only path, lock conflict, corrupt database, and
  unsupported schema without silent reset.
- [x] Add migration and restart integration tests using isolated databases.
- [x] Define the versioned TOML puzzle and pack schemas.
- [x] Validate syntax, semantics, Unicode, control characters, IDs, dimensions,
  budgets, and declared licenses.
- [x] Enforce portable ASCII pack and puzzle IDs, path-name rules, and every
  declared pack storage bound before final installation.
- [x] Implement bounded directory installation.
- [x] Choose one archive format only after confirming safe bounded extraction.
- [x] Reject traversal, absolute paths, links, devices, excessive depth,
  excessive count, and size-limit violations.
- [x] Install through same-filesystem private staging, a committed pending
  record, and an atomic final rename.
- [x] Reconcile interrupted pack installation before loading playable packs.
- [x] Test interruption after the pending-record commit, final rename, registry
  transaction, and failed cleanup.
- [x] Fingerprint installed content and detect conflicting pack IDs.
- [x] Load only registry metadata at startup and verify one selected community
  pack against its fingerprint before play.
- [x] Remove a pack without deleting progress records needed to explain saved
  history.
- [x] Add parser, replay, metadata, and archive fuzz targets.
- [x] Run a focused security review of every content and storage boundary.

Verification record (2026-09-02): the locked ordinary check, strict Clippy,
all 100 tests in both debug and release across every local Cargo target,
warning-denied Rust documentation, and the release build passed on Rust 1.98.0
and x86_64 Arch Linux. Four maximum-size random parser inputs completed without
a crash. Five
release processes each committed 500 durable completion writes on the recorded
Btrfs SSD; p95 ranged from 21.544 to 28.613 milliseconds, below the
50-millisecond local-SSD target. The storage and pack suites cover transaction
rollback, restart, pruning, schema and corruption refusal, lock and permission
errors, every durable install state, cleanup retry, selected-pack fingerprint
drift, malicious archive shapes, and declared limits. A Windows target check
reached the bundled SQLite C build but could not complete without a Windows CRT;
native Windows and macOS execution remain in the later platform smoke matrix.
The final boundary review also covers a spilled hot rollback journal,
completion writes inside the reserved pages, managed-root symlinks, missing
registered directories, unknown managed entries, recovery timestamps, bounded
diagnostic collection, and independent puzzle validation errors.

Exit gate:

- [x] Progress survives interruption and restart, and malicious pack fixtures
  cannot escape the managed directory, exhaust declared limits, or emit raw
  terminal controls.

### Phase 6, terminal foundation

Goal: create a portable terminal lifecycle and rendering base before filling it
with game screens.

- [ ] Add and pin the selected Ratatui and Crossterm versions.
- [ ] Enter raw mode and the alternate screen through one owned lifecycle.
- [ ] Restore terminal state after normal quit and every handled startup or
  runtime failure.
- [ ] Define the panic policy and best-effort terminal restoration without
  treating malformed input as a panic.
- [ ] Implement the bounded event queue and input loop.
- [ ] Preserve key order, coalesce tick and resize notifications, apply
  backpressure at capacity, and keep shutdown independently observable.
- [ ] Own tick timing, animation timing, cancellation, and shutdown.
- [ ] Avoid detached background work.
- [ ] Detect terminal size and supported color capability.
- [ ] Implement true-color, ANSI 256, ANSI 16, monochrome, and ASCII choices.
- [ ] Implement preferred, narrow, and resize-message layouts.
- [ ] Keep resize handling valid during every transient state.
- [ ] Define reusable focus, dialog, help, rules-step, error, paper, branch, and
  status components.
- [ ] Convert the monochrome Orifude mark into reviewed terminal-safe artwork.
- [ ] Add reduced-motion and instant-reveal settings.
- [ ] Sanitize external display strings before they reach rendering.
- [ ] Test view behavior with Ratatui's test backend using small audited
  expectations.
- [ ] Add a shipped-binary smoke test that verifies terminal restoration.
- [ ] Add the pull-request native smoke matrix for Linux x86_64, macOS Apple
  Silicon, and Windows x86_64 through mise.
- [ ] Run exploratory QA in common light, dark, limited-color, and monochrome
  terminals.

Exit gate:

- [ ] The empty application shell starts, resizes, navigates, reports errors,
  and exits without corrupting the terminal on supported native platforms.

### Phase 7, complete playable loop

Goal: connect the domain engine, persistence, and TUI into the complete player
journey.

- [ ] Implement first-launch capability checks and the interactive lesson using
  the shared visual teaching components.
- [ ] Implement the home branch and progress summary.
- [ ] Implement journey selection and locked or completed states.
- [ ] Implement the puzzle target view.
- [ ] Implement folded-paper and stack rendering.
- [ ] Implement fold selection, crease preview, direction selection, and
  confirmation.
- [ ] Implement brush selection, footprint preview, and application.
- [ ] Implement undo, reset confirmation, and unfolded preview.
- [ ] Implement result comparison with separate missing and extra ink.
- [ ] Implement bounded, interruptible reveal animation.
- [ ] Implement success, par, and saved-keepsake screens.
- [ ] Save completion before confirming durable success to the player.
- [ ] Implement daily mode with an injected local date.
- [ ] Implement endless mode with visible solver or generator exhaustion errors.
- [ ] Implement installed pack selection and missing-pack history behavior.
- [ ] Implement the replayable `How to play` view with a bounded, engine-derived
  fold, stack, ink, unfold, and comparison sequence.
- [ ] Implement settings, contextual help, and key-conflict validation.
- [ ] Implement spoiler-free text result export.
- [ ] Preserve drafts and attempt state across unrelated dialogs and resizes.
- [ ] Add end-to-end journeys through the actual binary.
- [ ] Add the bounded `mise run test-native` task and run the full supported
  operating-system and architecture matrix on `shrek`, manual
  dispatch, and release candidates.
- [ ] QA the loop with keyboard-only use and no knowledge of internal commands.

Exit gate:

- [ ] A new player can learn, solve, save, revisit, and replay a puzzle without
  leaving the TUI or consulting the source code.

### Phase 8, content and identity

Goal: give the complete engine enough reviewed puzzles, copy, and visual rhythm
to feel like Orifude rather than a rules demo.

- [ ] Define the journey groups and the mechanic taught by each group.
- [ ] Create at least 40 handcrafted journey puzzles.
- [ ] Verify every puzzle against the production validator.
- [ ] Record at least one bounded solver solution for every official puzzle.
- [ ] Review difficulty progression with fresh players.
- [ ] Remove puzzles whose solution depends on unexplained interface behavior.
- [ ] Write short titles and descriptions without generic filler.
- [ ] Write tutorial cues that explain one action at a time.
- [ ] Create the initial home-branch progression states.
- [ ] Use the squirrel only for delivery and completion moments.
- [ ] Add at least one official example community pack.
- [ ] Document puzzle authoring, validation, licensing, and contribution.
- [ ] Produce ASCII, ANSI, and Unicode-safe visual variants.
- [ ] Verify that all states remain readable without color.
- [ ] Verify that reduced motion removes every nonessential animation.
- [ ] Review every external string for terminal control and layout safety.
- [ ] Record a deterministic terminal journey for the README and frontend.
- [ ] Confirm artwork use and derived assets with the project owner.

Exit gate:

- [ ] The journey contains the promised content, difficulty has been observed
  with real players, and every visual mode communicates the full game state.

### Phase 9, hardening and release QA

Goal: attack assumptions, measure budgets, and verify supported platforms before
packaging work can hide defects behind a nice archive.

- [ ] Freeze the v1 trust-boundary map and perform a focused security review.
- [ ] Review puzzle, replay, pack, archive, path, SQLite, terminal, and config
  inputs from source to effect.
- [ ] Run dependency advisory and license checks and assess applicability.
- [ ] Run local secret scanning without sending source or findings externally.
- [ ] Complete bounded fuzz campaigns and preserve useful regression cases.
- [ ] Run property tests with recorded seeds and explicit budgets.
- [ ] Measure startup, idle CPU, input latency, memory, solver, database, and
  artifact budgets in a release build.
- [ ] Resolve or document every exceeded budget before release.
- [ ] Test interruption during database write, migration, pack install, solver
  work, reveal, and shutdown.
- [ ] Test full disk, read-only storage, corrupt database, unsupported format,
  and lock conflict.
- [ ] Test empty, whitespace, long, malformed, mixed-script, combining, and
  control-character content.
- [ ] Run native QA on supported Linux distributions.
- [ ] Run native QA on supported macOS architectures.
- [ ] Run native QA on supported Windows architectures.
- [ ] Run the full native matrix from mise against the release binaries and
  record any designated-host environments that hosted CI cannot provide.
- [ ] Test common terminal emulators and record exclusions.
- [ ] Verify install, startup, upgrade, rollback, uninstall, and progress
  preservation journeys.
- [ ] Resolve every release-blocking finding and retest the original path.
- [ ] Record remaining known issues and residual risk.

Exit gate:

- [ ] QA records a `PASS` or `PASS WITH KNOWN ISSUES` verdict and a `ship`
  recommendation for the release-candidate behavior on every supported native
  platform.

### Phase 10, release archives and distribution

Goal: create repeatable, least-privileged publication for canonical archives,
installers, and the three approved package channels.

- [ ] Confirm release runners and targets match the approved platform matrix.
- [ ] Keep release publication separate from ordinary CI and require passing
  ordinary and native checks for the exact approved commit.
- [ ] Build every artifact on a suitable native runner or document verified
  cross-build boundaries.
- [ ] Set and test the final release profile.
- [ ] Produce deterministic archive names and contents.
- [ ] Include license, concise README, and binary in each archive.
- [ ] Generate one complete SHA-256 checksum file after every archive exists.
- [ ] Generate both release-specific installers from the completed archive and
  checksum manifest.
- [ ] Embed the expected checksum for every supported archive in each relevant
  installer.
- [ ] Add a local release check that extracts every archive and verifies binary
  version, help, checksum file, and embedded installer checksums.
- [ ] Write the version-pinned POSIX installer.
- [ ] Write the version-pinned PowerShell installer.
- [ ] Test both installers against a local immutable fixture release.
- [ ] Test clean install, upgrade, tampered archive, tampered checksum file,
  embedded-hash mismatch, unsupported platform, destination conflict, and
  cleanup behavior.
- [ ] Test that failed and truncated installer downloads are never executed and
  make no filesystem change.
- [ ] Document separate download, inspection, and execution commands. Do not
  publish pipe-to-shell or pipe-to-`Invoke-Expression` forms.
- [ ] Render the macOS-only Homebrew formula for `nuggocto/homebrew-tap`.
- [ ] Test the formula on Intel and Apple Silicon macOS.
- [ ] Render the Scoop manifest for `nuggocto/scoop-bucket`.
- [ ] Test Scoop install, version, upgrade, and uninstall on supported Windows.
- [ ] Render `PKGBUILD` and `.SRCINFO` for only `orifude-bin`.
- [ ] Confirm the dedicated AUR SSH identity `aur@sshmoi.com` and official remote
  without exposing private key material.
- [ ] Test `orifude-bin` in clean Arch Linux build and install environments.
- [ ] Restrict release credentials to the exact repository and operation each
  publisher needs.
- [ ] Pin all release workflow actions to immutable reviewed revisions.
- [ ] Ensure pull requests from forks cannot access publication credentials.
- [ ] Add dry-run behavior for every external repository update.
- [ ] Make publication stop if the tag, checksum, archive matrix, or current
  branch commit is inconsistent.
- [ ] Enable immutable GitHub releases and verify the generated release
  attestation with GitHub CLI.
- [ ] Document rollback and package correction procedures.

Exit gate:

- [ ] One release candidate installs from archives, both classic installers,
  Homebrew, Scoop, and `orifude-bin` using the exact artifacts intended for v1.

### Phase 11, `orifude-front`

Goal: replace the holding page with a small static project presentation and a
release-driven changelog without creating a second application.

- [ ] Confirm the Cloudflare Pages build command, output directory, production
  branch, domain, and `www` redirect.
- [ ] Replace the holding-page build with a minimal Astro static project.
- [ ] Keep `/` and `/changelog/` as the only product routes.
- [ ] Implement a real static not-found path.
- [ ] Add the real wordmark, icon, and squirrel-courier artwork from the supplied
  identity source.
- [ ] Generate responsive optimized assets without changing the identity.
- [ ] Build the landing hero with a short introduction, TUI description, status,
  and direct GitHub repository link.
- [ ] Explain the fold, ink, and unfold mechanic with the reviewed static visual
  sequence and plain captions.
- [ ] Add the reviewed terminal recording or still from the native application.
- [ ] Add release links only after the corresponding artifacts verify.
- [ ] Present POSIX, PowerShell, Homebrew, Scoop, and AUR instructions without
  hiding platform constraints.
- [ ] Document optional GitHub CLI commands for verifying the immutable release
  and a downloaded release asset.
- [ ] Add a direct changelog link.
- [ ] Build the changelog as chronological folded-paper release entries.
- [ ] Define reviewed structured release data tied to the canonical changelog in
  the main repository.
- [ ] Show version, date, summary, change categories, tag, GitHub release, and
  supported install channels for each published release.
- [ ] Verify that a new release cannot appear without its required changelog
  fields and valid links.
- [ ] Keep the semantic release structure readable without animation or CSS.
- [ ] Bundle fonts and remove external script, font, analytics, and tracker
  dependencies.
- [ ] Add canonical, description, social, sitemap, robots, favicon, and theme
  metadata.
- [ ] Add restrictive Cloudflare static security headers.
- [ ] Test keyboard focus, reduced motion, zoom, narrow reflow, contrast, and
  accessible image text.
- [ ] Measure static asset size and browser performance before deployment.
- [ ] Build from a clean checkout with locked frontend dependencies.
- [ ] Verify preview deployment before promoting production.
- [ ] Verify `https://orifude.com`, the `www` redirect, changelog route, release
  links, installer links, GitHub link, headers, and not-found behavior in
  production.

Exit gate:

- [ ] The production site presents the native game and every published release
  accurately, without a browser client, required JavaScript, tracking, or dead
  installation links.

### Phase 12, v1 release

Goal: publish `v1.0.0`, update every approved distribution channel, and verify
the result as a user would receive it.

- [ ] Complete every earlier exit gate or record an explicit owner-approved
  scope change in this document.
- [ ] Freeze release scope and stop unrelated refactoring.
- [ ] Set package and binary version to `1.0.0`.
- [ ] Finalize the canonical changelog and release notes.
- [ ] Confirm dependency, license, security, and known-issue records.
- [ ] Build the release candidate from a clean locked checkout.
- [ ] Record artifact hashes and QA environment evidence.
- [ ] Run the complete native end-to-end matrix against the release candidate.
- [ ] Obtain the final QA verdict and separate ship recommendation.
- [ ] Create the signed or protected `v1.0.0` tag at the approved commit.
- [ ] Publish GitHub release archives, checksum file, and installer files.
- [ ] Confirm that GitHub reports the published release as immutable and that
  release verification succeeds.
- [ ] Re-download every published artifact and verify it against the published
  checksum file, embedded installer value, and release attestation.
- [ ] Publish the macOS-only Homebrew formula to `homebrew-tap`.
- [ ] Publish the Windows Scoop manifest to `scoop-bucket`.
- [ ] Publish only `orifude-bin` to AUR with the dedicated SSH identity.
- [ ] Run clean public installation journeys through POSIX, PowerShell,
  Homebrew, Scoop, and AUR.
- [ ] Publish the v1 changelog entry and verified installation links on
  `orifude-front`.
- [ ] Verify the production landing page and changelog after Cloudflare Pages
  deployment.
- [ ] Confirm that installers and package manifests resolve only immutable v1
  artifacts and exact checksums.
- [ ] Preserve failure evidence and hold publication if any release channel is
  intermittent or inconsistent.
- [ ] Document any package channel delayed after the canonical GitHub release.
- [ ] Publish the final supported-platform and known-issue statement.
- [ ] Mark v1 complete only after public artifacts and links pass verification.

Exit gate:

- [ ] A new user can discover Orifude, install it through every advertised
  channel, complete and save a puzzle offline, read the v1 changelog, and verify
  the downloaded artifact.

## After v1

Ideas that may be reconsidered only after v1 evidence exists:

- Diagonal folds using triangular cell subdivision
- More ink colors and resist cells
- A richer local puzzle editor
- Additional official puzzle packs
- More terminal-safe export formats
- A source-building AUR package if users request it and the owner changes the
  one-package policy
- Other package managers supported by real maintainers

Accounts, telemetry, a browser game, online multiplayer, and required cloud
services remain outside the product unless this document is deliberately
rewritten. They must not arrive disguised as a convenient dependency.
