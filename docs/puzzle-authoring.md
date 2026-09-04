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
