#!/bin/sh
set -eu

dist=${1:-dist}
version=${2:-}

test -n "${GH_TOKEN:-}" || {
    printf 'homebrew publisher: GH_TOKEN is required\n' >&2
    exit 1
}

temporary=$(mktemp -d "${TMPDIR:-/tmp}/orifude-homebrew.XXXXXX")
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
formula="${temporary}/orifude.rb"
"$(dirname "$0")/render-homebrew.sh" "$dist" "$version" > "$formula"
ruby -c "$formula" >/dev/null

content=$(base64 -w 0 "$formula")
path="repos/nuggocto/homebrew-tap/contents/Formula/orifude.rb"
sha=$(gh api "${path}?ref=shrek" --jq .sha 2>/dev/null || true)

if test -n "$sha"; then
    gh api --method PUT "$path" \
        -f message="Update Orifude to v${version}" \
        -f content="$content" \
        -f branch=shrek \
        -f sha="$sha" >/dev/null
else
    gh api --method PUT "$path" \
        -f message="Add Orifude v${version}" \
        -f content="$content" \
        -f branch=shrek >/dev/null
fi

printf 'Published Formula/orifude.rb to nuggocto/homebrew-tap.\n'
