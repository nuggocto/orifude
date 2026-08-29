#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/orifude-installer-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM

mkdir -p "$work/bin" "$work/fixture" "$work/archive" "$work/install"
printf '#!/bin/sh\nprintf "orifude fixture\\n"\n' > "$work/archive/orifude"
printf 'license\n' > "$work/archive/LICENSE"
printf 'readme\n' > "$work/archive/README.md"
chmod +x "$work/archive/orifude"
tar -czf "$work/fixture/orifude_0.2.0_linux_amd64.tar.gz" -C "$work/archive" LICENSE README.md orifude
hash=$(sha256sum "$work/fixture/orifude_0.2.0_linux_amd64.tar.gz" | awk '{ print $1 }')
printf '%s  orifude_0.2.0_linux_amd64.tar.gz\n' "$hash" > "$work/fixture/checksums.txt"

cat > "$work/bin/uname" <<'MOCK'
#!/bin/sh
case ${1:-} in
    -s) printf 'Linux\n' ;;
    -m) printf 'x86_64\n' ;;
    *) exit 1 ;;
esac
MOCK

cat > "$work/bin/curl" <<'MOCK'
#!/bin/sh
set -eu
output=
url=
while test "$#" -gt 0; do
    case $1 in
        --output)
            output=$2
            shift 2
            ;;
        http*)
            url=$1
            shift
            ;;
        *) shift ;;
    esac
done
test -n "$output"
case $url in
    */checksums.txt) source_file=checksums.txt ;;
    *) source_file=orifude_0.2.0_linux_amd64.tar.gz ;;
esac
cp "$INSTALLER_FIXTURE_DIR/$source_file" "$output"
MOCK
chmod +x "$work/bin/uname" "$work/bin/curl"

PATH="$work/bin:$PATH" INSTALLER_FIXTURE_DIR="$work/fixture" \
    ORIFUDE_INSTALL_DIR="$work/install" "$root/scripts/install.sh" --version 0.2.0
test -x "$work/install/orifude"

if PATH="$work/bin:$PATH" INSTALLER_FIXTURE_DIR="$work/fixture" \
    ORIFUDE_INSTALL_DIR="$work/install" "$root/scripts/install.sh" --version 0.3.0 >/dev/null 2>&1; then
    printf 'installer accepted an unsupported version\n' >&2
    exit 1
fi

rm -f "$work/install/orifude"
printf '%064d  orifude_0.2.0_linux_amd64.tar.gz\n' 0 > "$work/fixture/checksums.txt"
if PATH="$work/bin:$PATH" INSTALLER_FIXTURE_DIR="$work/fixture" \
    ORIFUDE_INSTALL_DIR="$work/install" "$root/scripts/install.sh" >/dev/null 2>&1; then
    printf 'installer accepted a mismatched checksum\n' >&2
    exit 1
fi
test ! -e "$work/install/orifude"

printf 'POSIX installer checks passed.\n'
