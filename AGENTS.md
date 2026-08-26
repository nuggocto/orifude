# Agent guide

## Source of truth

- Read `PROJECT.md` before changing behavior, architecture, data, or product copy.
- Keep the TUI as the only operational client. The separate landing site only
  explains the project and distributes releases.
- Numbered roadmap labels belong only in `PROJECT.md`. Do not repeat them in
  code, comments, commits, changelogs, the README, or any other file.
- Do not edit generated sqlc files by hand.

## Engineering

- Prefer the smallest complete change. Reuse the standard library and existing
  code before adding helpers or dependencies.
- Keep concrete types until a consuming package needs an interface.
- Treat letter text, bearer tokens, invite codes, and forwarded network data as
  hostile input. Never log message content or credentials.
- Keep database state transitions in short PostgreSQL transactions. The server
  remains authoritative.
- Format touched Go files with `gofmt`. Run the narrowest useful checks during
  development and `go test ./...` before handing off a working Go change.
- Keep commit subjects simple. Add a concise, well-written body that explains
  why the change exists and any important tradeoffs instead of repeating the
  diff.

## Tests

Only add a test if its failure would tell you something is actually broken. Assertions on styling values, colors, or internal structure fail on harmless changes and pass on real bugs, so leave them out.

Test observable behavior, authorization boundaries, state transitions, input
limits, concurrency, and generated-code drift. Do not snapshot terminal styling.

## Releases

- Do not add GoReleaser configuration until `cmd/orifude` builds.
- Release artifacts require checksums.
- Keep package metadata in the existing Homebrew and Scoop repositories and the
  separate AUR repository described in `PROJECT.md`.
