#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
binary=$(mktemp)

cleanup() {
    rm -f "$binary"
}
trap cleanup EXIT HUP INT TERM

case $(uname -m) in
    x86_64 | amd64) arch=amd64 ;;
    aarch64 | arm64) arch=arm64 ;;
    *) printf 'unsupported VHS architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
esac

(cd "$root" && CGO_ENABLED=0 GOOS=linux GOARCH="$arch" go build -o "$binary" ./cmd/orifude)

mkdir -p "$root/testdata/vhs/output"
docker run --rm \
    --network none \
    --env XDG_CONFIG_HOME=/tmp/orifude-config \
    --env DBUS_SESSION_BUS_ADDRESS=unix:path=/dev/null \
    --workdir /vhs \
    --volume "$root:/vhs" \
    --volume "$binary:/usr/local/bin/orifude:ro" \
    "ghcr.io/charmbracelet/vhs@sha256:9d5fc3dc0c160b0fb1d2212baff07e6bdf3fa9438c504a3237484567302fcf93" \
    testdata/vhs/landing.tape

test -s "$root/testdata/vhs/output/orifude-landing.webm"
test -s "$root/testdata/vhs/output/orifude-landing.ascii"
