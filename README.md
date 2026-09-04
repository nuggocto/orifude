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

Install mise and the pinned Rust toolchain, then start the game:

```console
mise run run
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
mise run run verify puzzles/example-pack
mise run run solve puzzles/example-pack
mise run run pack install puzzles/example-pack
mise run run pack list
mise run run pack remove paper-garden
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
mise install rust github:EmbarkStudios/cargo-deny --locked
mise run check
mise run test-native
```

`mise run check` verifies formatting, Clippy lints, tests, documentation,
dependency policy, and the release build. `mise run test-native` exercises the
shipped binary in a native pseudoterminal, including the first lesson, a saved
journey paper, restart, replay, preview, undo, reset, resize recovery, daily
generation, malformed-pack handling, and terminal restoration.

Focused tasks include `mise run run`, `mise run test`, `mise run lint`,
`mise run build`, the bounded parser and domain harnesses, and the solver,
paper, and storage measurements listed in `mise.toml`.

The product contract and work queue live in [`PROJECT.md`](PROJECT.md).
Implementation decisions and verification evidence live in
[`NOTEBOOK.md`](NOTEBOOK.md). Orifude is open source under the
[Apache 2.0 license](LICENSE) and has not published its first puzzle-game
release yet.
