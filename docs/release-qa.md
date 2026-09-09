# Release QA

This record keeps published-artifact evidence and its limits. It does not make
earlier test results evidence for a later binary. Follow the
[distribution guide](distribution.md) for release commands and recovery.

## 1.0.1 publication decision on 2026-09-08

Shipped with the [coverage limitations below](#coverage-and-limitations).
This patch changes installer defaults and player messages, with no save-schema
or dependency change. The immutable [release](https://github.com/nuggocto/orifude/releases/tag/v1.0.1)
uses signed source commit [4800168](https://github.com/nuggocto/orifude/commit/480016875b6ac7296f3c36915999aa2e39dd10b7).

| Verification | Evidence |
| --- | --- |
| Native checks | [Seven CI jobs](https://github.com/nuggocto/orifude/actions/runs/34268188969), [eleven candidate jobs](https://github.com/nuggocto/orifude/actions/runs/34268188948), and [pack validation](https://github.com/nuggocto/orifude/actions/runs/34268188806) passed. |
| Public installers | [Five native journeys](https://github.com/nuggocto/orifude/actions/runs/34269489611) checked installed bytes, play, save, restart, and replay. |
| Public packages | [Four journeys](https://github.com/nuggocto/orifude/actions/runs/34270233779) passed for both Homebrew architectures, Scoop, and x86_64 AUR. |
| Website | [Frontend 9c5daca](https://github.com/nuggocto/orifude-front/commit/9c5dacac4cf042acd60f7e9e03d6d252858cec9c) passed [hosted checks](https://github.com/nuggocto/orifude-front/actions/runs/34271138988), including 39 Linux and 26 Windows browser cases and native launcher fixtures. |

Publication compared every draft asset with the candidate and verified the
release and all eight asset attestations. [SHA256SUMS](https://github.com/nuggocto/orifude/releases/download/v1.0.1/SHA256SUMS)
contains the archive hashes. The PowerShell script SHA-256 is
`c0879a523df2bab1102359784431015da15d85aa08c50749504f7947d33c87e8`.

Live checks of [deployment 4e84316b](https://4e84316b.orifude-front.pages.dev)
and production covered all four routes, CSP, seven exact clipboard pastes, and
expanded installation instructions at 1440, 390, and 320 pixels. The
production-copied POSIX command installed and reinstalled 1.0.1 in private paths,
preserving a test profile and saved-data sentinel and removing temporary files.
Its executable matched the verified archive. Local captures and results remain
under the frontend's ignored `.preview/cleanup/` directory.

<a id="current-publication-decision-on-2026-09-07"></a>

## 1.0.0 publication decision on 2026-09-07

The first puzzle-game [release](https://github.com/nuggocto/orifude/releases/tag/v1.0.0)
uses signed source commit [f5db86d](https://github.com/nuggocto/orifude/commit/f5db86de6407e6da81d3acb53ff0474de048340e).
[CI](https://github.com/nuggocto/orifude/actions/runs/34120524324),
[candidate checks](https://github.com/nuggocto/orifude/actions/runs/34120524196),
[five public installers](https://github.com/nuggocto/orifude/actions/runs/34121865648),
and [four public package journeys](https://github.com/nuggocto/orifude/actions/runs/34122676056)
passed. Publication verified the release and all eight asset attestations.
Archive hashes remain in its [SHA256SUMS](https://github.com/nuggocto/orifude/releases/download/v1.0.0/SHA256SUMS).
The owner approved shipping with the same minimum-OS and terminal-GUI evidence
limits below. Version 1.0.1 did not replace these immutable assets.

## Pack publication verification on 2026-09-07

[Paper garden 1.0.0](https://github.com/nuggocto/orifude/releases/tag/pack-paper-garden-v1.0.0)
uses reviewed source [54a7f7d](https://github.com/nuggocto/orifude/commit/54a7f7d2ccd2c292c73f5425b8fc4c8d87fd90e5).
Its 5,752-byte ZIP has SHA-256
`bdce44bad07e92faf4bc1d564b14b913e1e90a6b9581c4ad9f3633984eeef2e6`.
[Source validation](https://github.com/nuggocto/orifude/actions/runs/34138254786)
and [publication verification](https://github.com/nuggocto/orifude/actions/runs/34139078008)
passed, including all three asset attestations. The [review](pack-reviews/paper-garden-1.0.0.md)
records authorship and redistribution terms.

The public 1.0.0 Linux x86_64 musl binary verified, solved, installed, and
completed all three papers in an isolated 100-by-30 tmux terminal on Arch Linux.
Removing and reinstalling the pack preserved the saved replay bytes; a new
process replayed Garden path successfully. This was native Linux pack play,
not a separate public pack-installation check on every OS.

## Coverage and limitations

[Cargo platform metadata](../Cargo.toml) declares Linux 5.10+ on x86_64 and ARM64,
macOS 13+ on Intel and Apple Silicon, and Windows 10 22H2+ on x86_64. Hosted
journeys cover Ubuntu 24.04 on both architectures, macOS 15 on both architectures,
and Windows Server 2025. Linux container checks cover Ubuntu, Debian, Fedora,
and Arch userlands on the host kernel.

Exact minimum OS versions and the macOS Terminal and Windows Terminal GUIs were
not independently verified on designated hosts. Owner-reported Windows and macOS
play did not record OS versions or artifact hashes. Native PTY and ConPTY tests
provide repeatable process and terminal-protocol coverage, not GUI inspection.
ARM64 AUR assembly and native ARM64 binary play are separate from public x86_64
AUR installation.

Frontend automation runs Chromium, Firefox, and WebKit on Linux, and Chromium
and Firefox on Windows. Windows WebKit clipboard and link-focus behavior excludes
that automation target; Linux WebKit is not native Safari or iPhone certification.
The owner accepted the blocked Cloudflare 404 injection and waived the `www`
redirect. Automated accessibility checks do not establish full conformance.

## Repeatable checks and budgets

Use `mise run check`, `mise run release-check`, and `mise run property-check`
for ordinary, optimized, and independent-model checks. `mise run fuzz-campaign`
runs bounded sanitizer campaigns. Packaged installation, failure preservation,
and save/restart/replay checks use disposable hosts as described in the
[distribution guide](distribution.md).

Run `mise run release-measure -- /absolute/path/to/extracted/orifude` on Linux
with tmux and no competing build or test load. Record the source, executable
hash, machine, workload, and samples. [The measurement script](../scripts/release-measure.sh)
checks startup p95 below 250 ms, input p95 within 33.334 ms, idle CPU below 1%,
ordinary play below 64 MiB RSS, solver below 128 MiB RSS, and durable-write p95
below 50 ms. It also records binary size without a fixed size ceiling.

The measured 1.0.0 musl executable had SHA-256
`ec57fa12e4c9d8809290b3e9578d5b52694b4a578690104caf8060ec3de12600`.
On Linux 7.1.9 x86_64 with a Ryzen AI MAX+ 395, startup p95 was 131.243 ms,
input p95 5.565 ms, and ordinary-play RSS 6,832 KiB. All measured budgets passed.
These warm-filesystem lab results are a historical baseline, not measurements
of 1.0.1 or minimum-OS performance. Solver and storage helpers used the local
GNU build; player timing used the packaged musl binary.
