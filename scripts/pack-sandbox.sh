#!/usr/bin/env bash
set -euo pipefail

if (($# != 2 && $# != 5)); then
    printf '%s\n' 'usage: pack-sandbox.sh TRUSTED_BINARY CATALOG [PACK_ID VERSION OUTPUT_PARENT]' >&2
    exit 2
fi
[[ ! -L "$2" && -d "$2" ]]
binary=$(realpath -- "$1")
catalog=$(realpath -- "$2")
readonly image='ubuntu:24.04@sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517'
container="orifude-pack-$(cat /proc/sys/kernel/random/uuid)"
trap 'docker rm --force "$container" >/dev/null 2>&1 || true' EXIT
arguments=(check /catalog)
mounts=()
if (($# == 5)); then
    [[ "$3" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ && ${#3} -le 64 ]]
    [[ "$4" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ && ${#4} -le 32 ]]
    output=$(realpath -- "$5")
    mounts+=(--mount "type=bind,source=$output,target=/output")
    arguments=(build "/catalog/$3/$4" "$4" /output/pack)
fi
docker pull "$image" >/dev/null
timeout --signal=TERM --kill-after=10s 600s docker run --name "$container" \
    --network none --read-only --cap-drop ALL --security-opt no-new-privileges \
    --user "$(id -u):$(id -g)" --cpus 2 --memory 512m --memory-swap 512m \
    --pids-limit 32 --ulimit nofile=128:128 --ulimit fsize=16777216:16777216 \
    --tmpfs /tmp:rw,nosuid,nodev,noexec,mode=1777,size=32m \
    --mount "type=bind,source=$binary,target=/pack-release,readonly" \
    --mount "type=bind,source=$catalog,target=/catalog,readonly" \
    "${mounts[@]}" "$image" /pack-release "${arguments[@]}"
