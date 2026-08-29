#!/bin/sh
set -eu

dist=${1:-dist}
version=${2:-}

fail() {
    printf 'release check: %s\n' "$1" >&2
    exit 1
}

test -f "${dist}/checksums.txt" || fail "checksums.txt is missing"

if test -z "$version"; then
    set -- "${dist}"/orifude_*_linux_amd64.tar.gz
    test "$#" -eq 1 || fail "could not determine one release version"
    version=${1#"${dist}/orifude_"}
    version=${version%_linux_amd64.tar.gz}
fi

expected_files="
orifude_${version}_darwin_amd64.tar.gz
orifude_${version}_darwin_arm64.tar.gz
orifude_${version}_linux_amd64.tar.gz
orifude_${version}_linux_arm64.tar.gz
orifude_${version}_windows_amd64.zip
orifude_${version}_windows_arm64.zip
"

count=0
for artifact in $expected_files; do
    test -f "${dist}/${artifact}" || fail "${artifact} is missing"
    test "$(awk -v name="$artifact" '$2 == name { count++ } END { print count + 0 }' "${dist}/checksums.txt")" -eq 1 || \
        fail "${artifact} does not have exactly one checksum"
    count=$((count + 1))
done
test "$count" -eq 6 || fail "the release matrix is incomplete"
test "$(wc -l < "${dist}/checksums.txt" | tr -d ' ')" -eq 6 || fail "checksums.txt contains an unexpected entry"

(
    cd "$dist"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum -c checksums.txt
    else
        shasum -a 256 -c checksums.txt
    fi
)

for os in darwin linux; do
    for arch in amd64 arm64; do
        listing=$(tar -tzf "${dist}/orifude_${version}_${os}_${arch}.tar.gz" | sort)
        test "$listing" = "LICENSE
README.md
orifude" || fail "unexpected contents in ${os}/${arch} archive"
    done
done

for arch in amd64 arm64; do
    listing=$(unzip -Z1 "${dist}/orifude_${version}_windows_${arch}.zip" | sort)
    test "$listing" = "LICENSE
README.md
orifude.exe" || fail "unexpected contents in windows/${arch} archive"
done

printf 'Verified six release archives and their SHA-256 checksums.\n'
