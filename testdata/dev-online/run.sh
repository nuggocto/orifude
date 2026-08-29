#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
runtime_dir=$(mktemp -d)
binary="$runtime_dir/orifude"
server_binary="$runtime_dir/tuitestpostoffice"
server_log="$runtime_dir/postoffice.log"
config_home="$runtime_dir/config"
container="orifude-dev-online-postgres-$(basename "$runtime_dir")"
server_pid=
test_invite="R0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0c"

cleanup() {
	if test -n "$server_pid"; then
		kill "$server_pid" >/dev/null 2>&1 || true
		wait "$server_pid" >/dev/null 2>&1 || true
	fi
	docker rm -f "$container" >/dev/null 2>&1 || true
	rm -rf -- "$runtime_dir"
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM

command -v docker >/dev/null 2>&1 || {
	printf 'dev-online requires Docker\n' >&2
	exit 1
}
command -v curl >/dev/null 2>&1 || {
	printf 'dev-online requires curl\n' >&2
	exit 1
}

(cd "$root" && CGO_ENABLED=0 go build -tags=testtool -o "$binary" ./cmd/orifude)
(cd "$root" && CGO_ENABLED=0 go build -tags=testtool -o "$server_binary" ./cmd/tuitestpostoffice)

docker run --detach --rm \
	--name "$container" \
	--env POSTGRES_PASSWORD=orifude \
	--env POSTGRES_DB=orifude_dev \
	--publish 127.0.0.1::5432 \
	postgres:18 >/dev/null

attempt=0
until docker exec "$container" pg_isready --username postgres --dbname orifude_dev >/dev/null 2>&1; do
	attempt=$((attempt + 1))
	if test "$attempt" -ge 60; then
		docker logs "$container"
		exit 1
	fi
	sleep 1
done

database_port=$(docker inspect --format '{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}' "$container")
database_url="postgres://postgres:orifude@127.0.0.1:$database_port/orifude_dev?sslmode=disable"

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

printf '\nInteractive post office: %s\n' "$server_origin"
printf 'Private-alpha test invite: %s\n' "$test_invite"
printf 'The database, synthetic KMS data, and local identity are disposable.\n'
printf 'Approve the owner-only file fallback when prompted.\n'
printf 'Press Enter to launch Orifude. Quit the TUI to stop and clean up.\n'
IFS= read -r _

ORIFUDE_OFFLINE_DEMO=0 \
ORIFUDE_API_URL="$server_origin" \
XDG_CONFIG_HOME="$config_home" \
DBUS_SESSION_BUS_ADDRESS=unix:path=/dev/null \
"$binary"
