# Release QA

This record keeps published-artifact evidence and its limits. It does not make
earlier test results evidence for a later binary. Follow the
[distribution guide](distribution.md) for release commands and recovery.

## 1.0.3 publication decision on 2026-09-12

The immutable [release](https://github.com/nuggocto/orifude/releases/tag/v1.0.3)
uses signed source commit [54b2130](https://github.com/nuggocto/orifude/commit/54b2130bf0197a643a1f781a6818db1d0959072a)
and a verified signed annotated tag. It normalizes valid SPDX whitespace before
installation, repairs affected registries without changing pack files or saved
play, and keeps the selected tool visible in compact layouts. Save schemas and
Cargo dependencies are unchanged. The [coverage limitations below](#coverage-and-limitations)
still apply.

| Verification | Evidence |
| --- | --- |
| Native checks | [Nine CI jobs](https://github.com/nuggocto/orifude/actions/runs/34704505187), [eleven candidate jobs](https://github.com/nuggocto/orifude/actions/runs/34704505177), and [pack validation](https://github.com/nuggocto/orifude/actions/runs/34704505291) passed. |
| Public installers | [Five native journeys](https://github.com/nuggocto/orifude/actions/runs/34705122298) verified public downloads, installed bytes, play, save, restart, and replay. |
| Public packages | [Four native journeys](https://github.com/nuggocto/orifude/actions/runs/34705496315) passed for Intel and Apple Silicon Homebrew, Scoop, and x86_64 AUR. |
| Nix | [All nine tag CI jobs](https://github.com/nuggocto/orifude/actions/runs/34705072050) passed, including installed-player checks on Linux x86_64 and ARM64. Public tagged-flake run and fresh-profile installation commands also reported 1.0.3 in the pinned Nix container. |

Publication compared all eight draft assets with the candidate and verified the
release and every asset attestation. [SHA256SUMS](https://github.com/nuggocto/orifude/releases/download/v1.0.3/SHA256SUMS)
records the five archive hashes. The PowerShell script SHA-256 is
`6f818c68fb3034626846b09748c727ee0078b030f5a6dd140effd8810777d7ca`.
Package updates are [Homebrew 8c6d023](https://github.com/nuggocto/homebrew-tap/commit/8c6d023be5e10579e87384d5f087396c512fbad3),
[Scoop 4227efe](https://github.com/nuggocto/scoop-bucket/commit/4227efe4d7ba233f696a630497b678ae82bfa8bc),
and AUR commit `30c6bed402976c0812d7dfa75f253f0944c3b0b2`.
Public repository contents match the verified metadata, and the AUR package page
reports `orifude-bin` version `1.0.3-1`.

The packaged Linux musl executable has SHA-256
`d74f54c0252a312eb718d4f893fae2cc16b5b3343b1ec6019528d6dc3d7b8db9`.
Installing a tab-suffixed license with the old 1.0.2 binary reproduced the corrupt
registry error. Listing with the packaged 1.0.3 binary repaired it; listing with
the old binary then worked too. Regression tests verify preserved pack bytes,
fingerprints, progress, and replays, plus transaction rollback and retry after a
failed repair. At 60-by-20, manual tmux checks of the packaged binary confirmed
that Tab after folding shows Open paper and Shift-Tab shows Dot brush. Captures
remain in `/tmp/orifude-103-artifact-ui`.

Local optimized checks passed 257 tests and one doctest, along with formatting,
shell analysis, dependency policies, and warning-denied Clippy. The packaged
player journey, independent-model checks, secret scan, and five 60-second
sanitizer campaigns with seed 424242 passed. The campaigns completed 24,978,343
executions across the five fuzz targets without failures.

On Linux 7.2.3 x86_64 with a Ryzen AI MAX+ 395, 25 fresh and 25 returning startup
samples gave p95 values of 111.761 and 96.740 ms. One hundred samples each gave
input p95 5.327 ms, fold p95 5.762 ms, and brush p95 5.469 ms. Ordinary play used
6,960 KiB RSS; the journey solver used 7,340 KiB. Measured idle CPU was 0% over
three seconds. Five storage runs had p95 between 21.293 and 28.775 ms. The shipped
executable is 5,741,824 bytes and its archive is 2,369,943 bytes. All configured
budgets passed. These warm-filesystem desktop measurements include tmux and frame
observation; unrelated system load was not controlled. They establish no speedup
over 1.0.2. Player timing used the packaged musl binary; solver and storage helpers
used the local GNU release build. Exact commands, environment details, raw samples,
and results remain in `target/release-measurement-1.0.3`.

## 1.0.2 publication decision on 2026-09-11

The immutable [release](https://github.com/nuggocto/orifude/releases/tag/v1.0.2)
uses signed source commit [535d9a7](https://github.com/nuggocto/orifude/commit/535d9a708ee9ecd3db79ca59923e80bd87ec59c6)
and a verified signed annotated tag. It adds direct Journey continuation and a
Nix flake, and corrects the opening message and compact completion controls.
Save schemas and Cargo dependencies are unchanged. The
[coverage limitations below](#coverage-and-limitations) still apply.

| Verification | Evidence |
| --- | --- |
| Native checks | [Nine CI jobs](https://github.com/nuggocto/orifude/actions/runs/34604129409), [eleven candidate jobs](https://github.com/nuggocto/orifude/actions/runs/34604129309), and [pack validation](https://github.com/nuggocto/orifude/actions/runs/34604129238) passed. The first macOS ARM64 CI setup lost its rustup process to SIGKILL; that job passed on a fresh runner without a source change. |
| Public installers | [Five native journeys](https://github.com/nuggocto/orifude/actions/runs/34605479565) verified public downloads, installed bytes, play, save, restart, and replay. |
| Public packages | [Four native journeys](https://github.com/nuggocto/orifude/actions/runs/34606186147) passed for both Homebrew architectures, Scoop, and x86_64 AUR. |
| Nix | [Tag CI](https://github.com/nuggocto/orifude/actions/runs/34605393560) passed both native Linux flake checks and the other seven CI jobs. The public tagged flake also passed local installation and player checks. |
| Website | [Frontend 3d653d2](https://github.com/nuggocto/orifude-front/commit/3d653d2523d9659ec6641632ee58ec9ad37c4ef8) passed [hosted checks](https://github.com/nuggocto/orifude-front/actions/runs/34607263711), including 39 Linux and 26 Windows browser cases and native launcher fixtures. |

Publication compared all eight draft assets with the candidate and verified the
release and every asset attestation. [SHA256SUMS](https://github.com/nuggocto/orifude/releases/download/v1.0.2/SHA256SUMS)
records the five archive hashes. The PowerShell script SHA-256 is
`8456bbb2d3c056ecb1eb09bc1c87e456f5a5a2fb4b14732c09fdbc141083e97e`.
Package updates are [Homebrew 224858f](https://github.com/nuggocto/homebrew-tap/commit/224858f623bcd2f4832ffc52fc2dd98f30734d81),
[Scoop cc765fd](https://github.com/nuggocto/scoop-bucket/commit/cc765fddd53ac06e293b12dfae51bde0325176a5),
and AUR commit `863f7e8bab15695be459bc023822784f6b60f66c`.
The public AUR page and RPC index report `orifude-bin` version `1.0.2-1`.

The packaged Linux musl executable has SHA-256
`04eb9b0fc25d8fc94dc0ab400d04970a8728ebc9604ab158485adce9e13dc5e8`.
Manual tmux checks reproduced the failed lesson opening with neutral text, then
confirmed two missing cells. Saved Journey completions offered Tab at 100-by-30
and 60-by-20, advanced into the next group, and kept each completion count at one.
The last paper stayed complete after Tab and returned home with all forty saved.
Captures remain in `/tmp/orifude-102-game-qa`; its synthetic player data was removed.

Local optimized checks passed 253 tests and one doctest, along with formatting,
shell analysis, dependency policies, and warning-denied Clippy. Independent-model
checks, the secret scan, and five 60-second sanitizer campaigns with seed 424242
passed. On Linux 7.2.3 x86_64 with a Ryzen AI MAX+ 395, 25 startup samples and 100
input samples gave startup p95 111.801 ms, returning startup p95 87.036 ms, and
input p95 5.555 ms. Ordinary play used 6,828 KiB RSS, and measured idle CPU was 0%.
Five storage runs had p95 between 21.331 and 28.587 ms. All configured budgets
passed. Player timing used the packaged musl binary; solver and storage helpers
used the local GNU build. Raw measurements remain in
`target/release-measurement-1.0.2`.

The published Nix run and profile commands both reported 1.0.2. The package ran
its installed-player journey after copying the production executable, keeping
test-only features out of the installed game. NixOS configuration evaluation
accepted the package; a complete NixOS system was not booted for this release.

Live checks of [deployment 7ef1c694](https://7ef1c694.orifude-front.pages.dev)
and production verified all four routes, current release text, CSP, eight exact
clipboard pastes, and installation layout at 1440, 390, and 320 pixels. Preview
routes retained noindex and worked with JavaScript disabled. The production-copied
POSIX command installed and reinstalled 1.0.2 inside a disposable container,
preserved a saved-data sentinel, cleaned its temporary files, and produced the
exact musl executable above. Frontend captures remain under its ignored
`.preview/release-1.0.2/` directory.

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
of later releases or minimum-OS performance. Solver and storage helpers used the local
GNU build; player timing used the packaged musl binary.
