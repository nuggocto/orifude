# Orifude

Orifude is a quiet, offline puzzle game for the terminal. Fold a small sheet,
place ink through its layers, then open it and match the target exactly. The
name is coined from the ideas of folding and brushwork.

The game is written in Rust, uses only the keyboard, and keeps progress on the
player's computer. It has an interactive first lesson, a handcrafted journey,
a deterministic daily paper, an endless local generator, keepsakes, and local
community packs.

```text
Pattern to match   Folded paper           Stack, bottom to top
. # # .            0 0 @ 0                0: cell 6
                                           1: cell 5

One fold lets one dot pass through both layers.
```

A deterministic [first-paper terminal recording](docs/recordings/journey.cast)
is checked in for documentation. It can be replayed with any asciicast v2
player.

## Play

The [website](https://orifude.com) lists verified installation channels and release
notes. The [distribution guide](docs/distribution.md#installing-an-exact-published-version)
explains how to download, inspect, and run an exact-version installer. The game
works offline after installation.

Start an installed copy with:

```console
orifude
```

The first launch explains the goal and leads through one real paper. During a
puzzle:

- An available fold or brush is ready as soon as the paper opens. `Enter` uses it.
- Arrow keys or `h`, `j`, `k`, `l` change a ready fold or move the brush cursor.
- Filled `●` marks placed ink; `◉` means the cursor is resting on ink. In ASCII
  mode, `*` is placed ink, `@` is the dry cursor, and `&` is the cursor on ink.
- `Tab` moves through folds, brushes, and Open paper. `f` and `b` jump straight
  to the fold and brush tools. `Esc` cancels a tool and readies Open paper.
- Opening compares every cell. `?` marks missing ink and `!` marks extra ink.
  The reference is the expected fold and stroke count, not a requirement for
  solving the paper.
- `v` replays a saved solution from fresh paper. `Enter` or Right advances one
  action, Left rewinds, and one final step opens the paper for comparison.
- `Space` previews the ink on the unfolded sheet.
- `u` undoes, `r` resets, `?` opens a short tool guide, and `q` leaves.

Bindings, color use, glyph mode, and motion can be changed inside terminal
settings. The minimum interactive terminal is 60 columns by 20 rows. Smaller
windows keep the current state and ask to be resized.

## Local puzzle packs

Orifude accepts bounded pack directories and ZIP archives containing inert
TOML and optional text notes. It never downloads pack content.

```console
orifude verify puzzles/example-pack
orifude solve puzzles/example-pack
orifude pack install puzzles/example-pack
orifude pack list
orifude pack remove paper-garden
```

The complete format, validation workflow, licensing notes, and contribution
checklist are in [Writing puzzle packs](docs/puzzle-authoring.md). The
[`paper-garden`](puzzles/example-pack/pack.toml) directory is a working example.

## Development

Install [rustup](https://rustup.rs/) and
[mise](https://mise.jdx.dev/getting-started.html) 2026.8.14 or newer. The exact
Rust toolchain is declared in `rust-toolchain.toml`. `Cargo.lock` and
`mise.lock` keep application builds and development tools reproducible.

```console
mise install --locked rust github:EmbarkStudios/cargo-deny shellcheck@0.11.0
mise run check
mise run test-native
mise run run
```

`mise run check` verifies formatting, Clippy lints, tests, documentation,
dependency policy, release-tool integrity, and the release build. The distribution
tool is a Rust development example. `mise run test-native` exercises the
shipped binary in a native pseudoterminal, including the first lesson, a saved
journey paper, restart, replay, preview, undo, reset, resize recovery, daily
generation, malformed-pack handling, and terminal restoration.

Focused tasks include `mise run run`, `mise run test`, `mise run lint`,
`mise run build`, the bounded parser and domain harnesses, and the solver,
paper, and storage measurements listed in `mise.toml`.

The product contract and work queue live in [`PROJECT.md`](PROJECT.md).
Implementation decisions and verification evidence live in
[`NOTEBOOK.md`](NOTEBOOK.md). Orifude is open source under the
[Apache 2.0 license](LICENSE).

## Share puzzle packs

Create packs as plain TOML files and submit them through pull requests. Isolated
CI validates and solves every puzzle; a maintainer reviews the content and
license before publication. The [pack guide](docs/puzzle-authoring.md#submit-through-a-pull-request)
explains the source layout, commands, review, versioning, and local installation.
Reviewed ZIP downloads and checksums appear on [orifude.com](https://orifude.com/#puzzle-packs).
