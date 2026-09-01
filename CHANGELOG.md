# Changelog

Orifude has not published a puzzle-game release yet. This changelog begins with
the puzzle-game reboot; history from the retired letter-exchange application is
intentionally not included.

## Unreleased

### Added

- Defined Orifude as a native, keyboard-driven folding-and-ink puzzle that
  works fully offline and keeps progress on the player's computer.
- Recorded the canonical paper, fold, ink, target, undo, replay, scoring,
  storage, puzzle-pack, terminal, platform, security, and release contracts.
- Established a dependency-free Rust application with a pinned toolchain,
  locked builds, stable command-line exit statuses, help and version output,
  typed output errors, and unsafe code denied.
- Added reproducible mise tasks for formatting, linting, tests, documentation,
  release builds, dependency policy checks, the paper exercise, and paper-model
  measurements.
- Added read-only ordinary CI for pull requests and pushes to the permanent
  `shrek` branch, with pinned actions, bounded job time, and no publication
  credentials.
- Added a bounded dense paper model in which every physical cell keeps a stable
  identity, coordinate, layer, face, and orientation.
- Added centered vertical and horizontal fold prototypes in every direction,
  moved-layer reversal, dot stamping through a stack, exact target comparison,
  and complete undo snapshots.
- Added focused tests for construction, fold directions, layer order, empty
  destinations, ink comparison, failed-action immutability, action budgets,
  and exact undo restoration.
- Added six plain-text folding exercises and a release-mode measurement that
  compares the dense paper model with a coordinate-to-stack map.
- Added a model-driven walkthrough that shows a crease, the folded stack order,
  ink passing through both layers, and how to enter the predicted top cell
  before the player makes a prediction.
- Added validated puzzle identities, targets, allowed actions, budgets, and par
  as the construction boundary for playable paper.
- Added every bounded horizontal and vertical fold, dot and line brushes,
  atomic action rejection, reset, result scoring, and exact state keys.
- Added versioned bounded replays that bind to the exact validated gameplay
  revision, execute on fresh paper, and report failures without exposing
  partial state.
- Added exhaustive crease properties, fixed-seed action properties, and a
  dependency-free bounded domain-action fuzz harness.
- Added a deterministic solver with exact state keys, fold-first score order,
  independent visited-state, memory, depth, and cancellation limits, and
  production replay verification for every reported solution.
- Added versioned deterministic puzzle generation with injected calendar dates,
  bounded candidate attempts, validator and solver checks, reproducible failure
  seeds, and a preserved daily-output golden.
- Added solver and generator integration tests plus a release measurement for
  frontier memory, visited lookup cost, path restoration, and representative
  solved and unsolved puzzles.
- Added concise play instructions to the README.

### Changed

- Clarified that a moved stack may become the only stack at an empty destination
  inside the original bounded working area.
- Recorded the current direct-push policy for `shrek` while preserving the
  earlier protected-branch verification as dated history.
- Chose dense physical-cell storage and complete snapshots after measuring the
  rejected map-based representation and the maximum paper bounds.
- Moved the plain-text paper exercise onto the validated production domain API.
- Reused paper allocations during reset so bounded search can restore parent
  paths without rebuilding the canonical cell vector each time.

### Fixed

- Made oversized paper-exercise input stop the session instead of allowing the
  unread tail to be interpreted as later commands.
- Kept deterministic generation trying its remaining bounded candidates after
  one candidate exhausts the solver.
- Removed a redundant invariant scan before in-place paper reset while keeping
  the rebuilt state checked before reset returns.

### Security

- Bounded command-line parsing, diagnostic cause traversal, paper dimensions,
  physical cells, actions, history, scratch storage, and exercise input.
- Prevented command-line arguments and paper-exercise input from being echoed
  into terminal output.
- Added locked advisory, license, and dependency-source checks to the ordinary
  repository check.
