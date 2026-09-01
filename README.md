# Orifude

Orifude is a quiet puzzle game for the terminal. You fold a small sheet of
paper, place ink through its layers, and open it again to see whether the marks
match the pattern.

It is written in Rust and played entirely with the keyboard. The game works
without a network connection and keeps progress on the player's computer.

Orifude is open source under the Apache 2.0 license and is still being built.
The current binary establishes the command-line contract but does not contain
the playable game yet.

## How to play

Each puzzle begins with a target pattern and a fresh sheet of paper.

1. Study the target, then fold the paper across the allowed creases.
2. Place a dot or line of ink on the folded paper. The ink passes through every
   layer beneath the brush.
3. Open the paper and compare the result with the target. Every required mark
   must be present, and there must be no extra ink.
4. Undo a step or restart the paper whenever you want to try another approach.

The final TUI is not available yet. Contributors can run `mise run paper` to
try the current plain-text folding exercise. The complete game rules live in
[`PROJECT.md`](PROJECT.md#canonical-game-rules).

## Development

Install [rustup](https://rustup.rs/) and
[mise](https://mise.jdx.dev/getting-started.html) 2026.8.14 or newer. The exact
Rust toolchain is declared in `rust-toolchain.toml`; mise reads that declaration
instead of keeping another Rust version. `mise.lock` records the reviewed URL
and checksum for the cargo-deny download. Rustup installs the exact declared
Rust toolchain and components.

`Cargo.lock` is committed for reproducible application builds. Tasks that
resolve or compile Rust dependencies use it in locked mode.

Install the tools and run every ordinary check:

```console
mise install rust github:EmbarkStudios/cargo-deny --locked
mise run check
```

The same task runs in CI. It checks formatting, Clippy lints, unit and
integration tests, documentation examples, the release build, dependency
advisories, licenses, and dependency sources. These focused tasks are also
available:

```console
mise run run
mise run fmt
mise run fmt-check
mise run lint
mise run test
mise run doctest
mise run build
mise run audit
mise run paper
mise run paper-measure
```

`mise run paper` opens a model-driven ASCII walkthrough followed by a bounded
exercise for predicting one-fold and two-fold layer order. When it asks for the
top cell ID, enter only that number and press Enter.
`mise run paper-measure` compares the dense paper state with the rejected
coordinate-to-stack map in a release build.

The planned native targets and minimum operating-system versions are recorded
as package metadata in `Cargo.toml`. The complete product contract and work
queue live in `PROJECT.md`. Work toward the first puzzle-game release is
recorded in [`CHANGELOG.md`](CHANGELOG.md).
