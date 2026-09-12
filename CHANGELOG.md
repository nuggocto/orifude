# Changelog

This changelog covers the folding-and-ink puzzle game. The retired
letter-exchange application is a separate product and is not an upgrade source.

## Unreleased

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
