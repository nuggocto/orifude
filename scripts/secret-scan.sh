#!/usr/bin/env bash

set -euo pipefail

readonly credential_pattern='-----BEGIN ([A-Z0-9 ]+ )?PRIVATE KEY-----|A[KS]IA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,}|AIza[0-9A-Za-z_-]{35}|xox[baprs]-[0-9A-Za-z-]{10,}|sk-(proj-)?[A-Za-z0-9_-]{20,}'
readonly excluded=':(exclude)scripts/secret-scan.sh'
readonly result_file="$(mktemp)"
trap 'rm -f "$result_file"' EXIT
found=0

if ! printf '%s\n' 'AKIA0000000000000000' | LC_ALL=C grep -Eq -e "$credential_pattern"; then
    printf '%s\n' "error: the credential patterns failed their local self-check" >&2
    exit 1
fi

report_matches() {
    local scope="$1"
    local matches="$2"
    if [[ -n "$matches" ]]; then
        printf 'credential-like material found in %s:\n%s\n' "$scope" "$matches" >&2
        found=1
    fi
}

scan_git() {
    local scope="$1"
    shift
    if git grep -I -l -E -e "$credential_pattern" "$@" >"$result_file"; then
        report_matches "$scope" "$(<"$result_file")"
    else
        local status=$?
        if ((status != 1)); then
            printf 'error: the credential scan failed in %s\n' "$scope" >&2
            exit "$status"
        fi
    fi
}

scan_git "the tracked working tree" -- . "$excluded"

while IFS= read -r -d '' path; do
    [[ "$path" == "scripts/secret-scan.sh" ]] && continue
    if LC_ALL=C rg -I -l -e "$credential_pattern" -- "$path" >"$result_file"; then
        report_matches "an untracked file" "$path"
    else
        status=$?
        if ((status != 1)); then
            printf 'error: the credential scan failed for an untracked file\n' >&2
            exit "$status"
        fi
    fi
done < <(git ls-files -z --others --exclude-standard)

while IFS= read -r revision; do
    scan_git "Git revision $revision" "$revision" -- . "$excluded"
done < <(git rev-list --all)

if ((found != 0)); then
    exit 1
fi
printf '%s\n' "No high-confidence credential pattern was found in the local tree or Git history."
