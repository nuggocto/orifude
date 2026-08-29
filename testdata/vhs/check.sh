#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
binary=$(mktemp)
server_binary=$(mktemp)
server_log=$(mktemp)
container="orifude-vhs-postgres-$$"
server_pid=

cleanup() {
    if test -n "$server_pid"; then
        kill "$server_pid" >/dev/null 2>&1 || true
        wait "$server_pid" >/dev/null 2>&1 || true
    fi
    docker rm -f "$container" >/dev/null 2>&1 || true
    rm -f "$binary" "$server_binary" "$server_log"
}
trap cleanup EXIT HUP INT TERM

case $(uname -m) in
	x86_64 | amd64) arch=amd64 ;;
	aarch64 | arm64) arch=arm64 ;;
	*) printf 'unsupported VHS architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
esac

(cd "$root" && CGO_ENABLED=0 GOOS=linux GOARCH="$arch" go build -o "$binary" ./cmd/orifude)
(cd "$root" && CGO_ENABLED=0 go build -tags=testtool -o "$server_binary" ./cmd/tuitestpostoffice)

docker run --detach --rm \
    --name "$container" \
    --env POSTGRES_PASSWORD=orifude \
    --env POSTGRES_DB=orifude_vhs \
    --publish 127.0.0.1::5432 \
    postgres:18 >/dev/null

attempt=0
until docker exec "$container" pg_isready --username postgres --dbname orifude_vhs >/dev/null 2>&1; do
    attempt=$((attempt + 1))
    if test "$attempt" -ge 60; then
        docker logs "$container"
        exit 1
    fi
    sleep 1
done

database_port=$(docker inspect --format '{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}' "$container")
database_url="postgres://postgres:orifude@127.0.0.1:$database_port/orifude_vhs?sslmode=disable"

attempt=0
until (cd "$root" && go tool goose -dir sql/migrations postgres "$database_url" status) >/dev/null 2>&1; do
    attempt=$((attempt + 1))
    if test "$attempt" -ge 60; then
        docker logs "$container"
        exit 1
    fi
    sleep 1
done
(cd "$root" && go tool goose -dir sql/migrations postgres "$database_url" up)

DATABASE_URL="$database_url" \
LISTEN_ADDR="127.0.0.1:0" \
"$server_binary" >"$server_log" 2>&1 &
server_pid=$!

attempt=0
server_origin=
until test -n "$server_origin" && curl --fail --silent "$server_origin/readyz" >/dev/null 2>&1; do
	server_origin=$(sed -n 's/^ready \(http:\/\/127\.0\.0\.1:[0-9][0-9]*\)$/\1/p' "$server_log" | head -n 1)
    attempt=$((attempt + 1))
    if ! kill -0 "$server_pid" >/dev/null 2>&1 || test "$attempt" -ge 60; then
        cat "$server_log" >&2
        exit 1
    fi
    sleep 1
done

mkdir -p "$root/testdata/vhs/output"
docker run --rm \
	--network host \
	--env ORIFUDE_API_URL="$server_origin" \
	--env XDG_CONFIG_HOME=/tmp/orifude-config \
	--env DBUS_SESSION_BUS_ADDRESS=unix:path=/dev/null \
	--workdir /vhs \
	--volume "$root:/vhs" \
	--volume "$binary:/usr/local/bin/orifude:ro" \
	"ghcr.io/charmbracelet/vhs@sha256:9d5fc3dc0c160b0fb1d2212baff07e6bdf3fa9438c504a3237484567302fcf93" \
	testdata/vhs/online.tape
test -s "$root/testdata/vhs/output/orifude-online.ascii"
