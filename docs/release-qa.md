# Release QA

This record keeps published-artifact evidence and its limits. It does not make
earlier test results evidence for a later binary. Follow the
[distribution guide](distribution.md) for release commands and recovery.

## 1.1.2 publication decision on 2026-10-27

The immutable [release](https://github.com/nuggocto/orifude/releases/tag/v1.1.2)
uses signed source commit [6a69c68](https://github.com/nuggocto/orifude/commit/6a69c6805bf35dd17532f6a2760ce9fbb997677a)
and a verified signed annotated tag. Both installers are now safe to pipe: the
POSIX body is one `main` function called on the last line, and the PowerShell
body is one script block.
The release also carries the unreleased simplifications and the ZIP dependency
update. Save schemas, content formats, and generator compatibility are unchanged.
The [coverage limitations below](#coverage-and-limitations) still apply.

| Verification | Evidence |
| --- | --- |
| Native checks | [Nine CI jobs](https://github.com/nuggocto/orifude/actions/runs/40596284043), [eleven candidate jobs](https://github.com/nuggocto/orifude/actions/runs/40596292414), and [pack validation](https://github.com/nuggocto/orifude/actions/runs/40596284035) passed. Each candidate installation job piped truncated copies of the real installer and required the previous executable to survive, then installed through a complete pipe. |
| Public installers | [Five native journeys](https://github.com/nuggocto/orifude/actions/runs/40598040170) verified public downloads, installed bytes, play, save, restart, and replay. |
| Public packages | [Four native journeys](https://github.com/nuggocto/orifude/actions/runs/40598928739) passed for Intel and Apple Silicon Homebrew, Scoop, and x86_64 AUR. |
| Nix | [All nine tag CI jobs](https://github.com/nuggocto/orifude/actions/runs/40597882219) passed, including installed-player checks on Linux x86_64 and ARM64. `nix run github:nuggocto/orifude/v1.1.2 -- --version` reported 1.1.2 in a fresh Nix container. |

Publication compared all eight draft assets with the candidate. The publisher's
first `gh release verify` ran before GitHub had created the release attestation
and reported none. Rerunning it a minute later verified the release and every
asset attestation; the publisher now retries for up to two minutes.
[SHA256SUMS](https://github.com/nuggocto/orifude/releases/download/v1.1.2/SHA256SUMS)
records the five archive hashes. The POSIX script SHA-256 is
`ac428b0c0ec1548020d5e0df084c2b2316b1926e20ac14fc508d65ef8a7a5073` and the
PowerShell script SHA-256 is
`f6ae14709fdafba378795046f40a4f2b4ae88dcd1d5448f2227c64d686304b3a`.

Package updates are [Homebrew ed14c52](https://github.com/nuggocto/homebrew-tap/commit/ed14c529b3d2b3342b662b0208e5f273da4f6141),
[Scoop a7659cc](https://github.com/nuggocto/scoop-bucket/commit/a7659cc7fbda666e35148ecabaf2b83901d00c33),
and AUR commit `6eb8cfa68558e6ea46e4f3dda7fe40b86b53c7ab`, pushed after release
attestation checks with the dedicated `aur@sshmoi.com` SSH identity.

Local checks passed formatting, shell analysis, dependency policies,
warning-denied Clippy, all tests, doctests, and the release tool tests. Rendering
the new POSIX template with the published 1.1.1 hashes installed 1.1.1 through a
pipe; the same script cut before its last line installed nothing and exited 0,
and cut halfway failed with a syntax error. A Windows-style `irm | iex` of the
new structure in PowerShell 7 left the caller's error preference unchanged, and
a truncated copy failed to parse before running.

The Linux musl executable has SHA-256
`895701a5d5affa65f35be4068b8d631bf462521c642058f722382b5d05f08106`.
It is 5,731,456 bytes; its archive is 2,396,615 bytes. Startup, input, and
storage measurements were not repeated for this installer and dependency release.

## 1.1.1 publication decision on 2026-10-01

The immutable [release](https://github.com/nuggocto/orifude/releases/tag/v1.1.1)
uses signed source commit [70025e8](https://github.com/nuggocto/orifude/commit/70025e8bceb5c4aa1fbc0713bd5931d13ef94e88)
and a verified signed annotated tag. Dialogs now measure text with the renderer's
own wrapping and keep their body above the footer, and the paper panel and
keepsake rows shorten titles by terminal display width. An 80-character title in
two-column characters no longer hides the ready tool, guidance, export lines, or
scores. The property script now fails unless each named test runs. Save schemas,
content formats, and generator compatibility are unchanged; `unicode-width`
became a direct dependency at the version already in the tree. The
[coverage limitations below](#coverage-and-limitations) still apply.

| Verification | Evidence |
| --- | --- |
| Native checks | [Nine CI jobs](https://github.com/nuggocto/orifude/actions/runs/36916155466), [eleven candidate jobs](https://github.com/nuggocto/orifude/actions/runs/36916174149), and [pack validation](https://github.com/nuggocto/orifude/actions/runs/36916155427) passed. |
| Public installers | [Five native journeys](https://github.com/nuggocto/orifude/actions/runs/36918002797) verified public downloads, installed bytes, play, save, restart, and replay. |
| Public packages | [Four native journeys](https://github.com/nuggocto/orifude/actions/runs/36918655073) passed for Intel and Apple Silicon Homebrew, Scoop, and x86_64 AUR. |
| Nix | [All nine tag CI jobs](https://github.com/nuggocto/orifude/actions/runs/36917877941) passed, including installed-player checks on Linux x86_64 and ARM64. Public tagged-flake run and fresh-profile installation reported 1.1.1 in the pinned Nix container and resolved to the release commit. |

Publication compared all eight draft assets with the candidate and verified the
release and every asset attestation. [SHA256SUMS](https://github.com/nuggocto/orifude/releases/download/v1.1.1/SHA256SUMS)
records the five archive hashes. The PowerShell script SHA-256 is
`4d024c3a727a59e4d2b93603d2dbc935e9d169f7c5720197912cc395494541b2`.

Package updates are [Homebrew 150b11d](https://github.com/nuggocto/homebrew-tap/commit/150b11d382b6ea93a6dca5303406a8fbc518769c),
[Scoop 40248b1](https://github.com/nuggocto/scoop-bucket/commit/40248b17e3535ce6e768bcba9bdb1ce69f8b80e6),
and AUR commit `93f29b5393809e2fbecb4392a8fe564495806952`. Their public files match
the verified generated metadata. AUR's `.SRCINFO` was regenerated and pushed with
the dedicated `aur@sshmoi.com` SSH identity after GitHub publication. AUR's web
view first served 1.1.0 while its Git repository already held 1.1.1; the view and
RPC then reported `1.1.1-1`.

Local optimized checks passed 266 tests and one doctest, with formatting, shell
analysis, dependency policies, warning-denied Clippy, and the property script,
whose five checks each ran exactly one test. Five 60-second sanitizer campaigns
with seed 424242 completed about 9.4 million executions without failures. The
direct-binary lifecycle check installed the published 1.1.0 and 1.1.1 Linux musl
archives in turn: 1.1.0 saved a completion through its own terminal, 1.1.1
replayed it, 1.1.0 replayed it again after rollback, and installed packs and saved
progress were preserved.

The Linux musl executable has SHA-256
`4a24caa2c90e7e3dfa048fe9466b55074b770e78c2480cd7ea92ff4d2e9aef02`.
It is 5,713,504 bytes; its archive is 2,387,304 bytes. On the same host as 1.1.0,
twenty-five fresh and twenty-five returning startup samples gave p95 values of
61.809 and 45.635 ms. One hundred samples each gave input p95 4.935 ms, fold p95
5.503 ms, and brush p95 6.113 ms. Ordinary play used 6,956 KiB RSS; the journey
solver used 7,340 KiB. Measured idle CPU was 0% over three seconds. Five storage
processes had p95 between 20.806 and 28.952 ms. All configured budgets passed.
These warm-filesystem measurements share the limits recorded for 1.1.0. Results
remain in `target/release-measurement-1.1.1`.

## 1.1.0 publication decision on 2026-10-01

The immutable [release](https://github.com/nuggocto/orifude/releases/tag/v1.1.0)
uses signed source commit [eb2c309](https://github.com/nuggocto/orifude/commit/eb2c3094c2198bc3b29ad07087f24318e7035ab9)
and a verified signed annotated tag. Boards number their rows and columns, the
stack shows where each layer began, completion cards leave the opened paper
visible, and the journey, keepsakes, home, and settings screens are reorganized.
Keys no longer act behind the minimum-size message, a generator panic is reported
instead of leaving the loading screen waiting, and startup creates only the data
directory. Save schemas, content formats, generator compatibility, and dependency
versions are unchanged. The [coverage limitations below](#coverage-and-limitations)
still apply.

| Verification | Evidence |
| --- | --- |
| Native checks | [Nine CI jobs](https://github.com/nuggocto/orifude/actions/runs/36896106352), [eleven candidate jobs](https://github.com/nuggocto/orifude/actions/runs/36896151277), and [pack validation](https://github.com/nuggocto/orifude/actions/runs/36896106340) passed. |
| Public installers | [Five native journeys](https://github.com/nuggocto/orifude/actions/runs/36897653877) verified public downloads, installed bytes, play, save, restart, and replay. |
| Public packages | [Four native journeys](https://github.com/nuggocto/orifude/actions/runs/36898453785) passed for Intel and Apple Silicon Homebrew, Scoop, and x86_64 AUR. |
| Nix | [All nine tag CI jobs](https://github.com/nuggocto/orifude/actions/runs/36897504188) passed, including installed-player checks on Linux x86_64 and ARM64. Public tagged-flake run and fresh-profile installation reported 1.1.0 in the pinned Nix container and resolved to the release commit. |

The first CI run for the release commit waited behind the superseded run for its
parent commit, which was cancelled; the release commit's own run passed every job.

Publication compared all eight draft assets with the candidate and verified the
release and every asset attestation. [SHA256SUMS](https://github.com/nuggocto/orifude/releases/download/v1.1.0/SHA256SUMS)
records the five archive hashes. The PowerShell script SHA-256 is
`c9422b983e22b86b7b3b5b97a81445446c1efe1c054d423284547d15d85a5256`.

Package updates are [Homebrew efd8bd7](https://github.com/nuggocto/homebrew-tap/commit/efd8bd78f69f998f8b62013c836dc81e2fec6545),
[Scoop 8ce60ff](https://github.com/nuggocto/scoop-bucket/commit/8ce60ff0fe8225022275dae958ca1f0530af8b4f),
and AUR commit `49d47fe87b8b6d173d3aad4c62decde50a4ad0f9`. Their public files match
the verified generated metadata. AUR's `.SRCINFO` was regenerated and pushed with
the dedicated `aur@sshmoi.com` SSH identity after GitHub publication. AUR's RPC
first served 1.0.4 after the Git repository had updated, then reported `1.1.0-1`.

Local optimized checks passed 262 tests and one doctest, with formatting, shell
analysis, dependency policies, warning-denied Clippy, and independent-model
checks. Five 60-second sanitizer campaigns with seed 424242 started from the new
generated seed corpus and completed about 9.0 million executions without failures.
The direct-binary lifecycle check installed the published 1.0.4 and 1.1.0 Linux
musl archives in turn: 1.0.4 saved a completion through its own terminal, 1.1.0
replayed it, 1.0.4 replayed it again after rollback, and installed packs and saved
progress were preserved.

The Linux musl executable has SHA-256
`97a95d3379a5b81263dcdfad0711984aefeea9ee7fb761026565f3b72287843f`.
It is 5,703,224 bytes; its archive is 2,385,987 bytes. On Linux 7.2.5 x86_64 with
a Ryzen AI MAX+ 395, twenty-five fresh and twenty-five returning startup samples
gave p95 values of 58.224 and 39.366 ms. One hundred samples each gave input p95
5.268 ms, fold p95 5.432 ms, and brush p95 5.558 ms. Ordinary play used 4,792 KiB
RSS; the journey solver used 7,276 KiB. Measured idle CPU was 0% over three
seconds. Five storage processes, each measuring 500 fresh and 500 populated
completion writes, had p95 between 21.234 and 27.657 ms. All configured budgets
passed.

These warm-filesystem desktop measurements include tmux and frame observation, and
background load was not controlled. Fresh startup is lower than the 1.0.4 record,
but the two runs did not share conditions, so they establish no speedup. Player
timing used the published musl binary; solver and storage helpers used the local
GNU release build. The returning-startup measurement first timed out because it
waited for the home title that 1.1.0 replaced; commit
[3f4bd40](https://github.com/nuggocto/orifude/commit/3f4bd40060158bb1c35386617f998984d15b49eb) waits for the current
title, and the measurement then completed. Results remain in
`target/release-measurement-1.1.0`.

## 1.0.4 publication decision on 2026-09-15

The immutable [release](https://github.com/nuggocto/orifude/releases/tag/v1.0.4)
uses signed source commit [50bd5ac](https://github.com/nuggocto/orifude/commit/50bd5ac540441e43bd9b72b28faa7143fe8cef80)
and a verified signed annotated tag. It adds file names and actionable reasons to
pack diagnostics, separates paper state, storage, and rendering responsibilities,
and removes redundant defensive code and tests. Save schemas, content formats,
generator compatibility, and dependency versions are unchanged. The
[coverage limitations below](#coverage-and-limitations) still apply.

| Verification | Evidence |
| --- | --- |
| Native checks | [Nine CI jobs](https://github.com/nuggocto/orifude/actions/runs/34989688261), [eleven candidate jobs](https://github.com/nuggocto/orifude/actions/runs/34989703209), and [pack validation](https://github.com/nuggocto/orifude/actions/runs/34989688219) passed. |
| Public installers | [Five native journeys](https://github.com/nuggocto/orifude/actions/runs/34991803353) verified public downloads, installed bytes, play, save, restart, and replay. |
| Public packages | [Four native journeys](https://github.com/nuggocto/orifude/actions/runs/34992694636) passed for Intel and Apple Silicon Homebrew, Scoop, and x86_64 AUR. |
| Nix | [All nine tag CI jobs](https://github.com/nuggocto/orifude/actions/runs/34991646087) passed, including installed-player checks on Linux x86_64 and ARM64. Public tagged-flake run and fresh-profile installation reported 1.0.4 in the pinned Nix container and resolved to the release commit. |

The first candidate Intel macOS installation job failed during toolchain setup:
the runner's rustup process exited with SIGILL before any project code ran. The
Intel build and ordinary CI had passed on the same runner image version. A single
rerun of the failed job on a fresh runner passed every check, using the same
archives. No application or workflow change was made for that setup failure.
The original [failure](https://github.com/nuggocto/orifude/actions/runs/34989703209/job/104453170061)
and successful [rerun](https://github.com/nuggocto/orifude/actions/runs/34989703209/job/104454972158)
remain available in the candidate record.

Publication compared all eight draft assets with the candidate and verified the
release and every asset attestation. [SHA256SUMS](https://github.com/nuggocto/orifude/releases/download/v1.0.4/SHA256SUMS)
records the five archive hashes. The PowerShell script SHA-256 is
`99b766de70fb8f485024e74a8617ef69e2ca8cb4b44e42478168f6f7098cb3d4`.

Package updates are [Homebrew 5634a82](https://github.com/nuggocto/homebrew-tap/commit/5634a82fac05bd878a15f56e55acd0bc55e12e74),
[Scoop ec94511](https://github.com/nuggocto/scoop-bucket/commit/ec945115403ed1a9457ad83d827ed8de85894243),
and AUR commit `219320028a6d82435f5028e55c541b732a03ae7d`. Their public files match
the verified generated metadata. AUR's `.SRCINFO` was regenerated and pushed with
the dedicated `aur@sshmoi.com` SSH identity after GitHub publication.
AUR's package page and a fresh RPC query report `1.0.4-1`. An earlier single-package
RPC query continued to serve 1.0.3 after the Git repository and package page had
updated; it subsequently refreshed to 1.0.4-1. The public AUR installation test
independently verified 1.0.4.

Local optimized checks passed 258 tests and one doctest, with formatting, shell
analysis, dependency policies, warning-denied Clippy, independent-model checks,
and the secret scan. The separately invoked packaged-player test passed. Five
60-second sanitizer campaigns with seed 424242 completed 17,015,792 executions
without failures, using the existing accumulated corpus.

The packaged Linux binary reports each invalid puzzle's filename and the precise
stroke-budget limit where 1.0.3 emitted identical generic messages. New regression
tests also cover parser positions and bounded, sanitized diagnostic text. Direct
binary upgrade and rollback between 1.0.3 and 1.0.4 preserved installed packs and
saved progress in an isolated player directory.

The Linux musl executable has SHA-256
`580281295d45b638d8596e5deff7a9b7b69e62f50b47707eb140b7f5de14aeea`.
It is 5,762,016 bytes; its archive is 2,382,235 bytes. On Linux 7.2.3 x86_64 with
a Ryzen AI MAX+ 395, twenty-five fresh and twenty-five returning startup samples
gave p95 values of 224.497 and 85.991 ms.
One hundred samples each gave input p95 5.322 ms, fold p95 5.211 ms, and brush p95
5.070 ms. Ordinary play used 6,896 KiB RSS; the journey solver used 7,468 KiB.
Measured idle CPU was 0% over three seconds. Five storage processes, each measuring
500 fresh and 500 populated completion writes, had p95 between 21.028 and 29.288 ms.
All configured budgets passed.

These warm-filesystem desktop measurements include tmux and frame observation.
Background load was not controlled, and artifact verification briefly overlapped
storage measurement. They establish no speedup over 1.0.3. Player timing used the
packaged musl binary; solver and storage helpers used the local GNU release build.
Commands, environment details, raw samples, and results remain in
`target/release-measurement-1.0.4`; validation logs are under `target/`.

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
and RPC index report `orifude-bin` version `1.0.3-1`.

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

Publication compared every draft asset with the candidate and verified the
release and all eight asset attestations. [SHA256SUMS](https://github.com/nuggocto/orifude/releases/download/v1.0.1/SHA256SUMS)
contains the archive hashes. The PowerShell script SHA-256 is
`c0879a523df2bab1102359784431015da15d85aa08c50749504f7947d33c87e8`.

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
