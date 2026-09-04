#!/usr/bin/env bash

set -euo pipefail

readonly toolchain="nightly-2026-07-01"
readonly seconds="${ORIFUDE_FUZZ_SECONDS:-60}"
readonly seed="${ORIFUDE_FUZZ_SEED:-424242}"
readonly output_root="${ORIFUDE_FUZZ_OUTPUT:-target/fuzz-campaign}"

case "$seconds" in
    ''|*[!0-9]*)
        printf '%s\n' "error: ORIFUDE_FUZZ_SECONDS must be a positive integer" >&2
        exit 2
        ;;
esac
if ((10#$seconds < 1 || 10#$seconds > 3600)); then
    printf '%s\n' "error: ORIFUDE_FUZZ_SECONDS must be between 1 and 3600" >&2
    exit 2
fi
case "$seed" in
    ''|*[!0-9]*)
        printf '%s\n' "error: ORIFUDE_FUZZ_SEED must be an unsigned integer" >&2
        exit 2
        ;;
esac

if ! rustup run "$toolchain" rustc --version >/dev/null 2>&1; then
    printf '%s\n' "error: Rust $toolchain is required for sanitizer instrumentation" >&2
    exit 1
fi
readonly toolchain_root="$(rustup run "$toolchain" rustc --print sysroot)"
if [[ ! -d "$toolchain_root/lib/rustlib/src/rust/library" ]]; then
    printf '%s\n' "error: Rust $toolchain must include the rust-src component" >&2
    exit 1
fi
if ! cargo fuzz --version >/dev/null 2>&1; then
    printf '%s\n' "error: cargo-fuzz is required" >&2
    exit 1
fi

mkdir -p "$output_root"
export CARGO_FROZEN=true

run_target() {
    local target="$1"
    local maximum="$2"
    local corpus="$output_root/corpus/$target"
    local artifacts="$output_root/artifacts/$target"
    mkdir -p "$corpus" "$artifacts"
    printf 'fuzz_target=%s seed=%s seconds=%s max_bytes=%s\n' \
        "$target" "$seed" "$seconds" "$maximum"
    cargo "+$toolchain" fuzz run "$target" "$corpus" \
        --fuzz-dir fuzz \
        -- \
        "-artifact_prefix=$artifacts/" \
        "-max_len=$maximum" \
        "-max_total_time=$seconds" \
        -timeout=5 \
        "-seed=$seed" \
        -verbosity=0 \
        -print_final_stats=1
}

run_target domain_actions 256
run_target puzzle_parser 65537
run_target pack_metadata 32769
run_target replay_parser 65537
run_target archive_parser 8388609
