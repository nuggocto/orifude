-- name: CreateAuthChallenge :one
WITH creation AS (SELECT clock_timestamp() AS value)
INSERT INTO auth_challenges (
    id, identity_id, public_key, key_thumbprint, purpose, nonce_hash, created_at, expires_at
) VALUES (
    sqlc.arg(id), sqlc.narg(identity_id), sqlc.arg(public_key), sqlc.arg(key_thumbprint),
    sqlc.arg(purpose), sqlc.arg(nonce_hash), (SELECT value FROM creation),
    (SELECT value + interval '5 minutes' FROM creation)
)
RETURNING *;

-- name: GetAuthChallengeForUpdate :one
SELECT * FROM auth_challenges
WHERE id = sqlc.arg(id)
FOR UPDATE;

-- name: ConsumeAuthChallenge :one
WITH consumed_at AS (SELECT clock_timestamp() AS value)
UPDATE auth_challenges
SET used_at = consumed_at.value
FROM consumed_at
WHERE id = sqlc.arg(id)
  AND purpose = sqlc.arg(purpose)
  AND used_at IS NULL
  AND expires_at > consumed_at.value
RETURNING auth_challenges.*;

-- name: CreateAccessSession :one
WITH issuance AS (SELECT clock_timestamp() AS value)
INSERT INTO access_sessions (token_hash, identity_id, key_thumbprint, created_at, expires_at)
SELECT sqlc.arg(token_hash), sqlc.arg(identity_id), sqlc.arg(key_thumbprint),
       issuance.value, issuance.value + interval '15 minutes'
FROM issuance
RETURNING *;

-- name: GetActiveAccessSession :one
SELECT access_sessions.*
FROM access_sessions
JOIN identities ON identities.id = access_sessions.identity_id
WHERE access_sessions.token_hash = sqlc.arg(token_hash)
  AND access_sessions.revoked_at IS NULL
  AND access_sessions.expires_at > clock_timestamp()
  AND identities.deleted_at IS NULL
  AND identities.key_thumbprint = access_sessions.key_thumbprint;

-- name: AccessSessionExpired :one
SELECT EXISTS (
    SELECT 1 FROM access_sessions
    WHERE token_hash = sqlc.arg(token_hash)
      AND expires_at <= clock_timestamp()
      AND revoked_at IS NULL
) AS expired;

-- name: RevokeIdentitySessions :execrows
UPDATE access_sessions
SET revoked_at = clock_timestamp()
WHERE identity_id = sqlc.arg(identity_id) AND revoked_at IS NULL;
