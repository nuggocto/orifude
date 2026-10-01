#!/usr/bin/env bash

set -euo pipefail

readonly timeout_seconds="${ORIFUDE_PROPERTY_SECONDS:-60}"

case "$timeout_seconds" in
    ''|*[!0-9]*)
        printf '%s\n' "error: ORIFUDE_PROPERTY_SECONDS must be a positive integer" >&2
        exit 2
        ;;
esac
if ((10#$timeout_seconds < 1 || 10#$timeout_seconds > 600)); then
    printf '%s\n' "error: ORIFUDE_PROPERTY_SECONDS must be between 1 and 600" >&2
    exit 2
fi

printf '%s\n' \
    "property_budget=2268 exhaustive folds; 8 seeds; 32 actions per seed; no shrinking; ${timeout_seconds}s per command"
# Cargo reports success when a name filter matches nothing, so each check must
# show that exactly one test ran and passed.
run_exact() {
    local target="$1"
    local name="$2"
    local output
    if ! output="$(timeout "${timeout_seconds}s" cargo test --locked --test "$target" "$name" -- --exact 2>&1)"; then
        printf '%s\n' "$output"
        exit 1
    fi
    printf '%s\n' "$output"
    if ! grep -q '^test result: ok\. 1 passed;' <<<"$output"; then
        printf 'error: %s did not run exactly one test named %s\n' "$target" "$name" >&2
        exit 1
    fi
}

run_exact engine fresh_folds_succeed_exactly_when_the_moving_side_fits_across_the_crease
run_exact engine replay_and_direct_execution_property_holds_for_fixed_action_sequences
run_exact solver solver_matches_an_independent_tiny_exhaustive_search
run_exact generator generation_is_reproducible_valid_and_replay_verified
run_exact content official_journey_is_valid_and_independently_solvable
