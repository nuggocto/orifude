-- name: RecordRateLimitEvent :one
INSERT INTO rate_limit_events (identity_id, kind, created_at)
SELECT sqlc.arg(identity_id), sqlc.arg(kind), clock_timestamp()
WHERE EXISTS (SELECT 1 FROM identities WHERE id = sqlc.arg(identity_id) AND deleted_at IS NULL)
RETURNING *;

-- name: CountRateLimitEvents :one
SELECT count(*) FROM rate_limit_events
WHERE identity_id = sqlc.arg(identity_id)
  AND kind = sqlc.arg(kind)
  AND created_at >= sqlc.arg(since);

-- name: RateLimitAllowed :one
SELECT
    (sqlc.arg(cooldown_seconds)::integer <= 0 OR NOT EXISTS (
        SELECT 1 FROM rate_limit_events
        WHERE rate_limit_events.identity_id = sqlc.arg(rate_identity_id)
          AND rate_limit_events.kind = sqlc.arg(rate_kind)
          AND rate_limit_events.created_at >= clock_timestamp() - make_interval(secs => sqlc.arg(cooldown_seconds)::integer)
    ))
    AND (sqlc.arg(hour_limit)::integer <= 0 OR (
        SELECT count(*) FROM rate_limit_events
        WHERE rate_limit_events.identity_id = sqlc.arg(rate_identity_id)
          AND rate_limit_events.kind = sqlc.arg(rate_kind)
          AND rate_limit_events.created_at >= clock_timestamp() - interval '1 hour'
    ) < sqlc.arg(hour_limit)::integer)
    AND (sqlc.arg(day_limit)::integer <= 0 OR (
        SELECT count(*) FROM rate_limit_events
        WHERE rate_limit_events.identity_id = sqlc.arg(rate_identity_id)
          AND rate_limit_events.kind = sqlc.arg(rate_kind)
          AND rate_limit_events.created_at >= clock_timestamp() - interval '1 day'
    ) < sqlc.arg(day_limit)::integer) AS allowed;

-- name: DeleteOldRateLimitEvents :execrows
DELETE FROM rate_limit_events
WHERE created_at < sqlc.arg(before);

-- name: DeleteOldRateLimitEventsBatch :many
WITH expired AS (
    SELECT id FROM rate_limit_events
    WHERE created_at < now() - make_interval(secs => sqlc.arg(retention_seconds)::integer)
    ORDER BY created_at, id
    FOR UPDATE SKIP LOCKED
    LIMIT sqlc.arg(batch_size)
)
DELETE FROM rate_limit_events
USING expired
WHERE rate_limit_events.id = expired.id
RETURNING rate_limit_events.id;
