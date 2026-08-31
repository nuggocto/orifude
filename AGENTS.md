# AGENTS.md

Read `PROJECT.md` before planning or changing Orifude. It defines the product,
architecture, limits, current work, and v1 checklist.

## Skills

- Use the `rust` skill and then the `tiger-style` skill for project work.
- Use the `unslop` skill for all writing, including comments and user-facing
  text.
- Use the `test-quality` skill when writing or reviewing tests.
- Use the `security` skill when work touches a trust boundary, such as puzzle
  packs, archives, paths, SQLite, terminal output, CI, installers, releases, or
  the frontend deployment.
- Use the `qa` skill when verifying user-visible behavior, packaged artifacts,
  supported platforms, the frontend, or a release candidate.

## Project rules

- Keep phase names, numbers, progress, and checkboxes only in `PROJECT.md`.
  Never put them in source, tests, filenames, configuration, comments, CLI
  output, or other documentation.
- Treat `shrek` as the permanent default branch. Do not rename or replace it.
- Name code after domain behavior such as paper, fold, ink, puzzle, replay, and
  pack.
- Update the current item and checkboxes in `PROJECT.md` only after the stated
  behavior and verification are complete.
- Keep Orifude a native, keyboard-driven Rust TUI that works fully offline.
- Do not add accounts, telemetry, required network access, or hosted application
  services. Do not use AWS, GCP, Azure, or similar providers.
- Keep `orifude-front` a separate static Astro site on Cloudflare Pages. It has
  a project landing page and a release changelog, never a browser game.
- Do not edit `../orifude-front` unless the current work or user request includes
  it.
- Use the artwork under `/home/nuggocto/Pictures/Orifude` as the identity source.
  Describe Orifude as a coined name inspired by folding and brushwork, not as a
  Japanese dictionary word.
- Write in simple human prose. Product writing may borrow quiet images from
  paper, ink, branches, weather, and the terminal. Use complete, natural
  sentences and never isolate a few words as a dramatic ending.
- Follow the architecture, invariants, resource bounds, security controls,
  testing requirements, and release channels recorded in `PROJECT.md`.
- Prefer the smallest auditable change. Keep external failures recoverable and
  reserve assertions or panics for programmer-error invariants.
- Run the checks required by the affected surface. Exercise the shipped binary
  for user-visible behavior and supported-platform claims.
- Write each commit with a simple, well-written subject line and an explanatory
  body. The body must explain why the change was needed instead of restating the
  diff.
- Preserve user work and avoid unrelated edits.
- Only add a test if its failure would tell you something is actually broken.
  Assertions on styling values, colors, or internal structure fail on harmless
  changes and pass on real bugs, so leave them out.
