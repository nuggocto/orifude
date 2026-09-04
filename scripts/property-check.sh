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
timeout "${timeout_seconds}s" cargo test --locked --test engine fold_and_failed_action_properties_hold_across_every_board_boundary -- --exact
timeout "${timeout_seconds}s" cargo test --locked --test engine replay_and_direct_execution_property_holds_for_fixed_action_sequences -- --exact
timeout "${timeout_seconds}s" cargo test --locked --test solver solver_matches_an_independent_tiny_exhaustive_search -- --exact
timeout "${timeout_seconds}s" cargo test --locked --test generator generation_is_reproducible_valid_and_replay_verified -- --exact
timeout "${timeout_seconds}s" cargo test --locked --test content official_journey_is_valid_and_independently_solvable -- --exact
