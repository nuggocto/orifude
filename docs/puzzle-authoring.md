# Writing puzzle packs

Orifude puzzle packs are local, inert TOML files. They contain no scripts and
do not need a network connection. The small
[`paper-garden`](../puzzles/example-pack/pack.toml) pack is a complete example
that can be copied and renamed.

## Check a pack

Run these commands from the repository after changing a pack:

```console
mise run run verify puzzles/example-pack
mise run run solve puzzles/example-pack
```

`verify` applies the same parser, limits, path rules, and game engine used when
a player installs a pack. `solve` independently searches every puzzle and
prints copyable TOML solution tables. A successful recorded solution proves
that its actions work, while `solve` checks that the bounded search can also
find a route.

Players can manage a local pack without opening the TUI:

```console
mise run run pack install puzzles/example-pack
mise run run pack list
mise run run pack remove paper-garden
```

Removing a pack removes its managed copy but keeps saved progress and replays.

## Directory shape

```text
paper-garden/
├── pack.toml
├── puzzles/
│   ├── first-seed.toml
│   ├── folded-leaves.toml
│   └── garden-path.toml
└── notes/
    └── first-seed.txt       optional
```

Every puzzle listed in `pack.toml` must have one matching file. A note is
optional and uses the same stem as a declared puzzle. Extra files, links,
special files, absolute paths, parent traversal, and nonportable names are
rejected.

ZIP packs use stored or deflated files and one bounded central directory ending
immediately before its footer. Orifude rejects exact and case-only duplicate
paths, inconsistent catalog lengths, and embedded footer signatures in catalog
metadata. Ordinary archive comments are allowed. Archives that rely on fallback
catalogs or offset repair are not supported; recreate them with an ordinary ZIP
tool or install the unpacked directory.

## Pack metadata

```toml
format_version = 1
id = "paper-garden"
title = "Paper garden"
description = "Three small papers."
authors = ["Orifude contributors"]
license = "Apache-2.0"
puzzles = ["first-seed", "folded-leaves", "garden-path"]
```

The `description` and `authors` fields may be omitted. The license is a valid
SPDX expression. IDs are stable lowercase ASCII keys: letters and digits may
be separated by single hyphens, with no leading or trailing hyphen. Changing
an ID creates a different pack or puzzle from the player's point of view.

## Puzzle file

```toml
format_version = 1
id = "folded-leaves"
title = "Folded leaves"
description = "One dot reaches two layers."
width = 4
height = 4
target = [
  "....",
  ".##.",
  "....",
  "....",
]
folds = [{ direction = "left", crease = 2 }]
brushes = [{ kind = "dot" }]
fold_budget = 1
stroke_budget = 1
par = { folds = 1, strokes = 1 }
tutorial_cues = [
  "Fold the right half to the left.",
  "Place one dot through the paired middle cells.",
  "Open the paper.",
]
author = "Ada Example"
license = "Apache-2.0"
solution = [
  { kind = "fold", direction = "left", crease = 2 },
  { kind = "dot", row = 1, column = 1 },
]
```

The target has one string per row. `#` means ink and `.` means clean paper.
Each row must match the declared width. Width and height are each between 4
and 12 cells.

A fold direction names where the moving side lands. For example, `left` moves
paper from the right side of the crease toward the left. Creases are numbered
from 1 at the first gap between cells.

Brushes may be a dot or a straight line:

```toml
brushes = [
  { kind = "dot" },
  { kind = "line", axis = "horizontal", length = 3 },
  { kind = "line", axis = "vertical", length = 2 },
]
```

Line lengths are between 2 and 12 cells. A puzzle may declare at most 44 fold
rules, 23 brush rules, 12 fold actions, and 8 brush actions. The full limits
are kept in [`PROJECT.md`](../PROJECT.md#explicit-v1-bounds).

`par`, tutorial cues, per-puzzle author and license, and `solution` are
optional. A solution uses zero-based row and column values because it is file
data; the TUI shows the same positions starting at 1. Orifude replays a supplied
solution through the production engine during validation and rejects the whole
pack if it does not solve the target exactly.

## Writing useful papers

Introduce one idea, then combine it with rules the player has already seen.
Keep each cue to one next action and explain any interface behavior before the
puzzle depends on it. A target should be readable without color, and a solution
should not rely on accidental cursor placement or an undocumented key.

Before contributing a pack:

1. Choose an SPDX license you have the right to grant.
2. Run `verify` and `solve` on the complete directory.
3. Play every puzzle from a clean start using only its visible instructions.
4. Ask another person to try the progression without coaching.
5. Submit the pack files, license information, and the observed play notes.

Do not include credentials, personal data, generated binaries, terminal
control characters, or content you do not have permission to redistribute.

## Submit through a pull request

1. Fork [nuggocto/orifude](https://github.com/nuggocto/orifude) and create a branch
   from `shrek`.
2. Copy `puzzles/example-pack` into `community/YOUR-PACK-ID/1.0.0/`. Set your own
   pack ID, title, authors, and SPDX license in `pack.toml`. Rename and edit the
   puzzle files and their declared IDs together. Keep `pack.toml` at that
   directory's root; do not add scripts, images, ZIPs, or extra license files.
3. Run `orifude verify community/YOUR-PACK-ID/1.0.0` and
   `orifude solve community/YOUR-PACK-ID/1.0.0` using the installed game, or use
   `mise run run` before either command from a source checkout. Play every puzzle.
4. Open a [pack pull request](https://github.com/nuggocto/orifude/compare/shrek...shrek?expand=1&template=puzzle-pack.md).
   Select your fork and branch. Include your authorship and license information,
   command results, play notes, and any borrowed material's source.
5. Respond to the maintainer's review. CI validates and solves the data using
   trusted code in an isolated container. A timeout or exhausted solver is a
   failed check; simplify the search space before resubmitting.

A maintainer reviews the puzzles, progression, text, authorship, and license.
They record the decision in `docs/pack-reviews/PACK-ID-VERSION.md`, merge the
reviewed files, and publish that exact version. CI success alone does not publish
anything. CODEOWNERS requests the owner's review; it is not a branch protection
rule. Publication is a separate manual maintainer action.

Pack versions are independent of Orifude's game version. Start at `1.0.0`, keep
old directories unchanged, and submit corrections in a new version directory.
Keep stable IDs when updating the same pack or puzzle. Published assets are
immutable, so even a text correction needs a new pack version. Packs remain
compatible with the existing `1.0.0` game and need no application update.

## Download and install a published pack

The [landing page](https://orifude.com/#puzzle-packs) links reviewed ZIPs and their
SHA-256 checksum files. Download the named pack ZIP, not GitHub's automatic
source-code archive, and verify the ZIP before installing it:

```console
sha256sum --check SHA256SUMS
orifude verify paper-garden-1.0.0.zip
orifude pack install paper-garden-1.0.0.zip
```

On macOS, use `shasum -a 256 -c SHA256SUMS`. On Windows, compare
`Get-FileHash .\paper-garden-1.0.0.zip -Algorithm SHA256` with `SHA256SUMS`.
GitHub CLI can additionally verify the immutable release and downloaded asset:

```console
gh release verify pack-paper-garden-v1.0.0 --repo nuggocto/orifude
gh release verify-asset pack-paper-garden-v1.0.0 paper-garden-1.0.0.zip --repo nuggocto/orifude
```

Launch `orifude` and choose Puzzle packs. The game never downloads packs itself.
To replace an installed version, remove its pack ID first, then install the new
ZIP. Removal preserves progress and keepsakes; changed gameplay can make an old
replay incompatible with the new puzzle revision.

## Maintainer publication

Run `mise run pack-catalog-check -- community` for the whole catalog, or prepare
a local proposal in a new directory:

```console
mise run pack-release-build -- community/paper-garden/1.0.0 1.0.0 target/paper-garden-proposal
```

The proposal contains a deterministic ZIP, `SHA256SUMS`, and `pack.json` for the
website. The builder verifies the archive fingerprint against the source and
solves every puzzle before writing the proposal. The catalog accepts at most
128 versions; each pack retains the game's existing limits.

After the merged commit's Puzzle packs workflow passes, run Publish puzzle pack
on `shrek` with that full commit, pack ID, and version. Confirm the review only
after completing and recording it. Leave Publish disabled to inspect the
proposal artifact first. Enable it to publish; the write job receives only
prepared data and never executes pack content. It creates a draft, compares
re-downloaded bytes, then publishes and checks GitHub's release and asset
attestations. Tags use `pack-PACK-ID-vVERSION` and never become the latest game
release. Existing tags and assets are not overwritten.

If publication fails, inspect the failed run and any draft before proceeding.
A draft can be removed after inspection; a published immutable release needs a
new pack version for corrections. Update the website only after verifying the
public ZIP, checksum, and a local install. Copy its `pack.json` data into the
reviewed website catalog and record the source commit and release evidence.

Include the full redistribution license and required attribution in the pack,
using a declared puzzle's `notes/PUZZLE-ID.txt` file. The example's
`notes/first-seed.txt` shows this. Notes must be valid UTF-8, contain no control
characters (including newlines or tabs), and fit within 16 KiB each. Keep license
wording intact while replacing line breaks with spaces. A maintainer must check
that the included terms cover the pack and preserve any required notices.
