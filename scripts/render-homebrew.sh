#!/bin/sh
set -eu

dist=${1:-dist}
version=${2:-}

fail() {
    printf 'homebrew renderer: %s\n' "$1" >&2
    exit 1
}

if test -z "$version"; then
    test -f "${dist}/metadata.json" || fail "metadata.json is missing"
    version=$(sed -n 's/.*"version":"\([^"]*\)".*/\1/p' "${dist}/metadata.json")
fi

test -n "$version" || fail "a release version is required"
case "$version" in
    *[!0-9A-Za-z.+-]*) fail "the release version contains unsupported characters" ;;
esac
test -f "${dist}/checksums.txt" || fail "checksums.txt is missing"

checksum() {
    name=$1
    value=$(awk -v name="$name" '$2 == name { print $1 }' "${dist}/checksums.txt")
    test "$(printf '%s' "$value" | wc -c)" -eq 64 || fail "invalid checksum for ${name}"
    test "$(awk -v name="$name" '$2 == name { count++ } END { print count + 0 }' "${dist}/checksums.txt")" -eq 1 || \
        fail "ambiguous checksum for ${name}"
    printf '%s' "$value"
}

darwin_amd64=$(checksum "orifude_${version}_darwin_amd64.tar.gz")
darwin_arm64=$(checksum "orifude_${version}_darwin_arm64.tar.gz")
linux_amd64=$(checksum "orifude_${version}_linux_amd64.tar.gz")
linux_arm64=$(checksum "orifude_${version}_linux_arm64.tar.gz")

sed \
    -e "s/@VERSION@/${version}/g" \
    -e "s/@DARWIN_AMD64@/${darwin_amd64}/g" \
    -e "s/@DARWIN_ARM64@/${darwin_arm64}/g" \
    -e "s/@LINUX_AMD64@/${linux_amd64}/g" \
    -e "s/@LINUX_ARM64@/${linux_arm64}/g" \
    "$(dirname "$0")/orifude.rb.tmpl"
