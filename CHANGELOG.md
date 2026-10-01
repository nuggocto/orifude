# Changelog

This changelog covers the folding-and-ink puzzle game. The retired
letter-exchange application is a separate product and is not an upgrade source.

## Unreleased

### Fixed

- Keys pressed while the terminal is below the minimum size no longer change
  the hidden paper, menus, or dialogs. Ctrl+C still quits, and the resize
  message now says so.
- If paper generation stops unexpectedly, Orifude reports the error and
  restores the terminal instead of waiting on the loading screen.
- Startup no longer fails when the unused configuration or cache directory
  cannot be created.
- The lesson's completion card no longer says a keepsake was saved; the lesson
  leaves none.

### Changed

- Every board numbers its rows and columns, so hints such as "row 2, column 3"
  point to a visible place on the paper.
- The stack panel shows where each layer began on the open sheet instead of
  internal cell numbers.
- The completion card sits below the opened paper when there is room, so the
  matched result stays visible.
- The journey groups its papers under their eight chapters, marks finished,
  open, and locked papers, and shows the gift each chapter brings home.
- Keepsakes use the paper names you played, such as "Journey 1.1 First drop",
  and explain the empty list.
- Home shows the chapter gifts collected so far and a short note for the
  selected choice. Settings and key bindings line up in two sections.
- Panel titles and text keep clear of the borders, dialogs fit their message,
  and the key hints at the bottom show each key in bold.

## 1.0.4 - 2026-09-15

Make puzzle-pack errors easier to fix and the codebase easier to maintain.

### Fixed

- Pack validation identifies each invalid puzzle file and reports TOML line and
  column positions or the specific puzzle rule that failed. Diagnostic output
  remains bounded and safe for terminals.

### Changed

- Separate paper state transitions, storage responsibilities, and board rendering
  into focused modules. Rendering uses named inputs and explicit display modes.
- Simplify bounded arithmetic and test setup, remove an unused custom state hash,
  and keep tests focused on game behavior, compatibility, and failure recovery.

## 1.0.3 - 2026-09-12

Keep installed packs usable and the selected tool visible.

### Fixed

- Licenses with valid SPDX whitespace no longer prevent the game from starting
  after pack installation. Affected registry entries recover automatically,
  preserving installed pack files, saved progress, and replays.
- Compact terminal layouts keep the current tool visible after a fold or brush
  stroke, including when switching to Open paper.

## 1.0.2 - 2026-09-11

Continue straight to the next Journey paper after completing a puzzle.

### Added

- Press Tab on a saved Journey completion to open the next paper, including
  the first paper of the next group.
- Build and play on Linux x86_64 and ARM64 with a pinned Nix flake, or add
  Orifude to a NixOS configuration.

### Fixed

- The opening animation uses neutral text until the final comparison, so
  failed attempts no longer appear to be saving a matched result.
- Completion controls and their help text fit the minimum 60-by-20 terminal.

## 1.0.1 - 2026-09-08

Install Orifude with fewer steps and clearer instructions.

### Changed

- Installers create a user-owned destination automatically and still accept a custom directory.
- Windows installation adds Orifude to the user PATH, with a -NoPath option for custom setups.
- Generated papers use plain instructions for playing and cancelling.

## 1.0.0 - 2026-09-07

The first Orifude puzzle release brings folding, brushwork, and a growing branch
of keepsakes to the terminal. Play at your own pace, keep everything on your
computer, and return to a paper whenever you like.

### Added

- Fold paper horizontally or vertically, place dots and lines of ink through
  its layers, and open it to match a pattern exactly.
- Learn through an interactive first lesson and a replayable explanation of
  folds, layer order, and brushwork.
- Play forty handcrafted papers in eight groups, with new keepsakes joining
  the home branch as you progress.
- Open a deterministic daily paper or generate another puzzle in the endless
  garden, with no network connection or timer.
- Undo, reset, preview an unfolded paper, and inspect missing or extra ink.
- Save progress, settings, and best solutions locally in SQLite. Replay a saved
  solution one action at a time after restarting the game.
- Install and remove local puzzle packs without losing their saved history.
  An example pack and author guide explain the bounded TOML format.
- Validate and solve puzzle packs with the `verify` and `solve` commands, and
  manage installed packs from the command line.
- Use keyboard controls, remappable bindings, ASCII glyphs, monochrome output,
  and reduced motion. Small terminals retain the paper while asking for more room.
- Distribute native archives for Linux x86_64 and ARM64, Intel and Apple Silicon
  macOS, and Windows x86_64, with SHA-256 checksums.
- Provide exact-version POSIX and PowerShell installers, a macOS Homebrew
  formula, a Windows Scoop manifest, and the `orifude-bin` AUR package.
- Explain the game and publish release notes on a static website with bundled
  artwork and fonts.

### Security

- Validate puzzle, replay, and pack contents before changing player state.
  Reject archive path escapes, links, duplicate members, executable content,
  and oversized inputs.
- Keep terminal output free of external control sequences and bound paper,
  search, history, archive, and storage resource use.
- Save progress transactionally and reconcile interrupted pack installations
  before managed content becomes playable.
- Verify installer downloads against embedded archive checksums before replacing
  an existing executable. Failed downloads preserve the previous installation.
- Bind release assets and package metadata to exact versions and checksums,
  with immutable GitHub releases and release verification before package updates.
