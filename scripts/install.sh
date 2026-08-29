#!/bin/sh
set -eu

version="0.2.0"
repository="nuggocto/orifude"
install_dir=${ORIFUDE_INSTALL_DIR:-"${HOME}/.local/bin"}

fail() {
    printf 'orifude installer: %s\n' "$1" >&2
    exit 1
}

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar >/dev/null 2>&1 || fail "tar is required"

while test "$#" -gt 0; do
    case $1 in
        --version)
            test "$#" -ge 2 || fail "--version requires a value"
            test "$2" = "$version" || fail "this installer only supports v${version}"
            shift 2
            ;;
        --install-dir)
            test "$#" -ge 2 || fail "--install-dir requires a value"
            test -n "$2" || fail "--install-dir cannot be empty"
            install_dir=$2
            shift 2
            ;;
        -h | --help)
            printf 'Usage: install.sh [--version %s] [--install-dir DIRECTORY]\n' "$version"
            exit 0
            ;;
        *) fail "unknown argument: $1" ;;
    esac
done

case $(uname -s) in
    Darwin) os=darwin ;;
    Linux) os=linux ;;
    *) fail "unsupported operating system: $(uname -s)" ;;
esac

case $(uname -m) in
    x86_64 | amd64) arch=amd64 ;;
    arm64 | aarch64) arch=arm64 ;;
    *) fail "unsupported architecture: $(uname -m)" ;;
esac

archive="orifude_${version}_${os}_${arch}.tar.gz"
release_url="https://github.com/${repository}/releases/download/v${version}"
temporary=$(mktemp -d "${TMPDIR:-/tmp}/orifude-install.XXXXXX")
trap 'rm -rf "$temporary"' EXIT HUP INT TERM

curl --fail --location --proto '=https' --tlsv1.2 \
    --output "${temporary}/${archive}" "${release_url}/${archive}"
curl --fail --location --proto '=https' --tlsv1.2 \
    --output "${temporary}/checksums.txt" "${release_url}/checksums.txt"

expected=$(awk -v name="$archive" '$2 == name { print $1 }' "${temporary}/checksums.txt")
case $expected in
    [0-9a-f][0-9a-f]*) ;;
    *) fail "release checksum is missing or malformed" ;;
esac
test ${#expected} -eq 64 || fail "release checksum is malformed"
test "$(awk -v name="$archive" '$2 == name { count++ } END { print count + 0 }' "${temporary}/checksums.txt")" -eq 1 || \
    fail "release checksum is ambiguous"

if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "${temporary}/${archive}" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "${temporary}/${archive}" | awk '{ print $1 }')
else
    fail "sha256sum or shasum is required"
fi
test "$actual" = "$expected" || fail "checksum verification failed"

tar -xzf "${temporary}/${archive}" -C "$temporary"
binary="${temporary}/orifude"
test -f "$binary" || fail "verified archive does not contain orifude"
test ! -L "$binary" || fail "refusing a symbolic-link binary"

mkdir -p "$install_dir"
staged="${install_dir}/.orifude.${$}.tmp"
trap 'rm -rf "$temporary"; rm -f "$staged"' EXIT HUP INT TERM
install -m 0755 "$binary" "$staged"
mv -f "$staged" "${install_dir}/orifude"

printf 'Installed Orifude v%s to %s/orifude\n' "$version" "$install_dir"
