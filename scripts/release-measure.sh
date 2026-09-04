#!/usr/bin/env bash

set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
    printf '%s\n' "error: release measurement currently requires Linux" >&2
    exit 1
fi

for tool in tmux awk sort stat getconf gzip strip sha256sum; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'error: required measurement tool is missing: %s\n' "$tool" >&2
        exit 1
    fi
done

repository="$(git rev-parse --show-toplevel)"
readonly repository
binary="$(realpath "${1:-$repository/target/release/orifude}")"
readonly binary
readonly startup_runs="${ORIFUDE_STARTUP_RUNS:-25}"
readonly input_runs="${ORIFUDE_INPUT_RUNS:-100}"
readonly idle_seconds="${ORIFUDE_IDLE_SECONDS:-3}"
readonly storage_runs="${ORIFUDE_STORAGE_RUNS:-5}"
output="${ORIFUDE_MEASURE_OUTPUT:-$repository/target/release-measurement-$(date -u +%Y%m%dT%H%M%SZ)}"
readonly output
readonly work_root="$repository/target/orifude-release-measure-$$"
readonly socket="orifude-measure-$$"
readonly session="paper"
readonly raw="$output/terminal.csv"

positive_bounded() {
    local name="$1"
    local value="$2"
    local maximum="$3"
    case "$value" in
        ''|*[!0-9]*)
            printf 'error: %s must be a positive integer\n' "$name" >&2
            exit 2
            ;;
    esac
    if ((10#$value < 1 || 10#$value > maximum)); then
        printf 'error: %s must be between 1 and %s\n' "$name" "$maximum" >&2
        exit 2
    fi
}

positive_bounded ORIFUDE_STARTUP_RUNS "$startup_runs" 100
positive_bounded ORIFUDE_INPUT_RUNS "$input_runs" 1000
positive_bounded ORIFUDE_IDLE_SECONDS "$idle_seconds" 30
positive_bounded ORIFUDE_STORAGE_RUNS "$storage_runs" 10

if [[ ! -x "$binary" || ! -f "$binary" ]]; then
    printf '%s\n' "error: the release binary must be an executable regular file" >&2
    exit 1
fi
if [[ -e "$work_root" ]]; then
    printf '%s\n' "error: the private measurement root already exists" >&2
    exit 1
fi
if [[ -e "$output" ]] && [[ -n "$(find "$output" -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null)" ]]; then
    printf '%s\n' "error: the measurement output directory is not empty" >&2
    exit 1
fi

mkdir -p "$work_root" "$output"

# The EXIT trap invokes this function indirectly.
# shellcheck disable=SC2317,SC2329
cleanup() {
    tmux -L "$socket" kill-server >/dev/null 2>&1 || true
    if [[ "$work_root" == "$repository"/target/orifude-release-measure-* ]]; then
        rm -rf -- "$work_root"
    fi
}
trap cleanup EXIT

now_ns() {
    date +%s%N
}

start_session() {
    local state="$1"
    local launch
    mkdir -p "$state/data" "$state/config" "$state/cache"
    printf -v launch 'exec env TERM=xterm-256color XDG_DATA_HOME=%q XDG_CONFIG_HOME=%q XDG_CACHE_HOME=%q %q' \
        "$state/data" "$state/config" "$state/cache" "$binary"
    tmux -L "$socket" new-session -d -x 100 -y 30 -s "$session" "$launch"
}

wait_for_text() {
    local text="$1"
    local timeout_ms="$2"
    local started deadline current
    started="$(now_ns)"
    deadline=$((started + timeout_ms * 1000000))
    while tmux -L "$socket" has-session -t "$session" 2>/dev/null; do
        if tmux -L "$socket" capture-pane -p -t "$session" | grep -Fq -- "$text"; then
            return 0
        fi
        current="$(now_ns)"
        if ((current >= deadline)); then
            printf 'error: terminal did not render %q within %s ms\n' "$text" "$timeout_ms" >&2
            return 1
        fi
        sleep 0.001
    done
    printf '%s\n' "error: terminal process exited before the expected frame" >&2
    return 1
}

stop_session() {
    if ! tmux -L "$socket" has-session -t "$session" 2>/dev/null; then
        return
    fi
    tmux -L "$socket" send-keys -t "$session" -l 'x'
    tmux -L "$socket" send-keys -t "$session" -l 'q'
    if wait_for_text "Leave Orifude?" 2000; then
        tmux -L "$socket" send-keys -t "$session" -l 'y'
    fi
    local attempt=0
    while tmux -L "$socket" has-session -t "$session" 2>/dev/null && ((attempt < 400)); do
        sleep 0.005
        ((attempt += 1))
    done
    if tmux -L "$socket" has-session -t "$session" 2>/dev/null; then
        printf '%s\n' "error: measured terminal did not stop" >&2
        return 1
    fi
}

percentile() {
    local metric="$1"
    local percentile="$2"
    awk -F, -v metric="$metric" '$1 == metric { print $3 }' "$raw" \
        | sort -n \
        | awk -v percentile="$percentile" '
            { values[NR] = $1 }
            END {
                selected = int((NR * percentile + 99) / 100)
                if (selected < 1) selected = 1
                print values[selected]
            }
        '
}

measure_solver_rss() {
    local stderr_file="$work_root/solver-stderr"
    local peak=0
    local sample state status_content
    "$binary" solve "$repository/puzzles/journey" >/dev/null 2>"$stderr_file" &
    local pid=$!
    while [[ -r "/proc/$pid/status" ]]; do
        if ! status_content="$(cat "/proc/$pid/status" 2>/dev/null)"; then
            break
        fi
        sample="$(awk '$1 == "VmHWM:" { print $2 }' <<<"$status_content")"
        if [[ -n "$sample" ]] && ((sample > peak)); then
            peak="$sample"
        fi
        state="$(awk '$1 == "State:" { print $2 }' <<<"$status_content")"
        [[ "$state" == "Z" ]] && break
    done
    if ! wait "$pid"; then
        printf 'error: solver measurement failed: %s\n' "$(<"$stderr_file")" >&2
        return 1
    fi
    if ((peak == 0)); then
        printf '%s\n' "error: solver memory was not observable" >&2
        return 1
    fi
    printf '%s\n' "$peak"
}

printf '%s\n' "metric,index,microseconds" >"$raw"

for ((run = 1; run <= startup_runs; run += 1)); do
    state="$work_root/startup-$run"
    started="$(now_ns)"
    start_session "$state"
    wait_for_text "Enter starts the lesson." 5000
    ended="$(now_ns)"
    printf 'startup,%s,%s\n' "$run" "$(((ended - started) / 1000))" >>"$raw"
    stop_session
done

state="$work_root/input"
start_session "$state"
wait_for_text "Enter starts the lesson." 5000
tmux -L "$socket" send-keys -t "$session" -l 'x'
wait_for_text "The paper is ready." 2000

for ((run = 1; run <= input_runs; run += 1)); do
    started="$(now_ns)"
    tmux -L "$socket" send-keys -t "$session" -l '?'
    wait_for_text "Keyboard help" 2000
    ended="$(now_ns)"
    printf 'input,%s,%s\n' "$run" "$(((ended - started) / 1000))" >>"$raw"
    tmux -L "$socket" send-keys -t "$session" -l '?'
    wait_for_text "The paper is ready." 2000
done

tmux -L "$socket" send-keys -t "$session" Enter
wait_for_text "Pattern to match" 5000
app_pid="$(tmux -L "$socket" display-message -p -t "$session" '#{pane_pid}')"
if [[ ! -r "/proc/$app_pid/stat" || ! -r "/proc/$app_pid/status" ]]; then
    printf '%s\n' "error: measured process accounting is unavailable" >&2
    exit 1
fi

clock_ticks="$(getconf CLK_TCK)"
before_ticks="$(awk '{ print $14 + $15 }' "/proc/$app_pid/stat")"
idle_started="$(now_ns)"
sleep "$idle_seconds"
after_ticks="$(awk '{ print $14 + $15 }' "/proc/$app_pid/stat")"
idle_ended="$(now_ns)"
idle_cpu_percent="$(awk -v delta="$((after_ticks - before_ticks))" \
    -v ticks="$clock_ticks" -v elapsed_ns="$((idle_ended - idle_started))" \
    'BEGIN { printf "%.3f", (delta / ticks) / (elapsed_ns / 1000000000) * 100 }')"
rss_kib="$(awk '$1 == "VmRSS:" { print $2 }' "/proc/$app_pid/status")"
stop_session

solver_rss_kib="$(measure_solver_rss)"
cargo run --quiet --locked --release --example solver_measure >"$output/solver.txt"
for ((run = 1; run <= storage_runs; run += 1)); do
    cargo run --quiet --locked --release --example storage_measure \
        >"$output/storage-$run.txt"
done
storage_p95_min_us="$(
    awk -F= '$1 == "p95_us" { print $2 }' "$output"/storage-*.txt | sort -n | head -n 1
)"
storage_p95_max_us="$(
    awk -F= '$1 == "p95_us" { print $2 }' "$output"/storage-*.txt | sort -n | tail -n 1
)"

original_bytes="$(stat -c '%s' "$binary")"
cp "$binary" "$work_root/orifude.stripped"
strip --strip-all "$work_root/orifude.stripped"
stripped_bytes="$(stat -c '%s' "$work_root/orifude.stripped")"
gzip -n -9 "$work_root/orifude.stripped"
compressed_bytes="$(stat -c '%s' "$work_root/orifude.stripped.gz")"
binary_sha256="$(sha256sum "$binary" | awk '{ print $1 }')"
startup_p50_us="$(percentile startup 50)"
startup_p95_us="$(percentile startup 95)"
startup_p99_us="$(percentile startup 99)"
input_p50_us="$(percentile input 50)"
input_p95_us="$(percentile input 95)"
input_p99_us="$(percentile input 99)"

for value in \
    "$startup_p50_us" "$startup_p95_us" "$startup_p99_us" \
    "$input_p50_us" "$input_p95_us" "$input_p99_us" \
    "$rss_kib" "$solver_rss_kib" "$storage_p95_min_us" "$storage_p95_max_us"; do
    case "$value" in
        ''|*[!0-9]*)
            printf '%s\n' "error: a release measurement is missing or malformed" >&2
            exit 1
            ;;
    esac
done

{
    printf 'binary=%s\n' "$binary"
    printf 'binary_sha256=%s\n' "$binary_sha256"
    printf 'startup_runs=%s\n' "$startup_runs"
    printf 'startup_p50_us=%s\n' "$startup_p50_us"
    printf 'startup_p95_us=%s\n' "$startup_p95_us"
    printf 'startup_p99_us=%s\n' "$startup_p99_us"
    printf 'input_runs=%s\n' "$input_runs"
    printf 'input_p50_us=%s\n' "$input_p50_us"
    printf 'input_p95_us=%s\n' "$input_p95_us"
    printf 'input_p99_us=%s\n' "$input_p99_us"
    printf 'idle_seconds=%s\n' "$idle_seconds"
    printf 'idle_cpu_percent=%s\n' "$idle_cpu_percent"
    printf 'ordinary_play_rss_kib=%s\n' "$rss_kib"
    printf 'journey_solver_rss_kib=%s\n' "$solver_rss_kib"
    printf 'storage_runs=%s\n' "$storage_runs"
    printf 'storage_p95_min_us=%s\n' "$storage_p95_min_us"
    printf 'storage_p95_max_us=%s\n' "$storage_p95_max_us"
    printf 'release_binary_bytes=%s\n' "$original_bytes"
    printf 'stripped_binary_bytes=%s\n' "$stripped_bytes"
    printf 'stripped_gzip_bytes=%s\n' "$compressed_bytes"
} | tee "$output/summary.txt"

printf 'raw_samples=%s\n' "$raw"

budget_failure=0
if ((startup_p95_us >= 250000)); then
    printf '%s\n' "error: startup p95 exceeded 250 milliseconds" >&2
    budget_failure=1
fi
if ((input_p95_us >= 33334)); then
    printf '%s\n' "error: input p95 exceeded one 30 Hz frame" >&2
    budget_failure=1
fi
if ! awk -v value="$idle_cpu_percent" 'BEGIN { exit !(value < 1.0) }'; then
    printf '%s\n' "error: idle CPU reached or exceeded 1 percent" >&2
    budget_failure=1
fi
if ((rss_kib >= 65536)); then
    printf '%s\n' "error: ordinary play reached or exceeded 64 MiB RSS" >&2
    budget_failure=1
fi
if ((solver_rss_kib >= 131072)); then
    printf '%s\n' "error: journey solver reached or exceeded 128 MiB RSS" >&2
    budget_failure=1
fi
if ((storage_p95_max_us >= 50000)); then
    printf '%s\n' "error: a storage p95 reached or exceeded 50 milliseconds" >&2
    budget_failure=1
fi
exit "$budget_failure"
