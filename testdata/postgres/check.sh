#!/bin/sh
set -eu

container="orifude-postgres-$$"

cleanup() {
    docker rm -f "$container" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

docker run --detach --rm \
    --name "$container" \
    --env POSTGRES_PASSWORD=orifude \
    --env POSTGRES_DB=orifude_test \
    --publish 127.0.0.1::5432 \
    postgres:18 >/dev/null

attempt=0
until docker exec "$container" pg_isready --username postgres --dbname orifude_test >/dev/null 2>&1; do
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 60 ]; then
        docker logs "$container"
        exit 1
    fi
    sleep 1
done

port=$(docker inspect --format '{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}' "$container")
export TEST_DATABASE_URL="postgres://postgres:orifude@127.0.0.1:$port/orifude_test?sslmode=disable"

attempt=0
until go tool goose -dir sql/migrations postgres "$TEST_DATABASE_URL" status >/dev/null 2>&1; do
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 30 ]; then
        docker logs "$container"
        exit 1
    fi
    sleep 1
done

go tool goose -dir sql/migrations postgres "$TEST_DATABASE_URL" up
go test -p=1 -tags=integration ./cmd/postoffice ./internal/...
go test -race -p=1 -tags=integration ./cmd/postoffice ./internal/...
go tool goose -dir sql/migrations postgres "$TEST_DATABASE_URL" down

tables=$(docker exec "$container" psql --username postgres --dbname orifude_test --tuples-only --no-align \
    --command "SELECT count(*) FROM pg_tables WHERE schemaname = 'public' AND tablename <> 'goose_db_version'")
test "$tables" = "0"
