#!/usr/bin/env bash

set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
    printf '%s\n' "error: Linux distribution checks require a Linux Docker host" >&2
    exit 1
fi
if ! command -v docker >/dev/null 2>&1; then
    printf '%s\n' "error: Docker is required for distribution checks" >&2
    exit 1
fi
if (($# != 1)) || [[ "$1" != /* ]]; then
    printf '%s\n' "error: provide one absolute executable Linux musl binary path" >&2
    exit 2
fi

readonly binary="$(realpath -- "$1")"
readonly repository="$(git rev-parse --show-toplevel)"
if [[ ! -x "$binary" || ! -f "$binary" ]]; then
    printf '%s\n' "error: the Linux musl binary must be an executable regular file" >&2
    exit 1
fi

readonly expected_version="$("$binary" --version)"
readonly expected_verification="$("$binary" verify "$repository/puzzles/example-pack")"
readonly images=(
    "ubuntu:24.04@sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517"
    "debian:12-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171"
    "fedora:44@sha256:43b29f65a41eb9c35e1cd5323e3bdf3b655c2357a9f4f1ff2f9c2798e5045d80"
    "archlinux:base@sha256:82b1b08faae9d61e3e7e13d562f4d09114d939105b0d59ff34140f3bd418593a"
)

for image in "${images[@]}"; do
    printf 'distribution_image=%s\n' "$image"
    docker run --rm \
        --platform linux/amd64 \
        --network none \
        --read-only \
        --cap-drop ALL \
        --security-opt no-new-privileges \
        --tmpfs /state:rw,nosuid,nodev,mode=1777,size=32m \
        --user 65534:65534 \
        --env "EXPECTED_VERSION=$expected_version" \
        --env "EXPECTED_VERIFICATION=$expected_verification" \
        --env XDG_DATA_HOME=/state/data \
        --env XDG_CONFIG_HOME=/state/config \
        --env XDG_CACHE_HOME=/state/cache \
        --mount "type=bind,source=$binary,target=/orifude,readonly" \
        --mount "type=bind,source=$repository/puzzles/example-pack,target=/example-pack,readonly" \
        "$image" \
        /bin/sh -eu -c '
            expect_equal() {
                if [ "$1" != "$2" ]; then
                    printf "error: %s did not match the host result\n" "$3" >&2
                    exit 1
                fi
            }
            version="$(/orifude --version)"
            expect_equal "$version" "$EXPECTED_VERSION" version
            verified="$(/orifude verify /example-pack)"
            expect_equal "$verified" "$EXPECTED_VERIFICATION" verification
            installed="$(/orifude pack install /example-pack)"
            expect_equal "$installed" "Installed pack paper-garden." installation
            listed="$(/orifude pack list)"
            expected_list="$(printf "paper-garden\tPaper garden")"
            expect_equal "$listed" "$expected_list" listing
            removed="$(/orifude pack remove paper-garden)"
            expect_equal "$removed" \
                "Removed pack paper-garden. Saved progress was kept." removal
            empty="$(/orifude pack list)"
            expect_equal "$empty" "No puzzle packs are installed." empty-list
            . /etc/os-release
            printf "distribution=%s version=%s architecture=%s result=pass\n" \
                "$ID" "${VERSION_ID:-unknown}" "$(uname -m)"
        '
done
