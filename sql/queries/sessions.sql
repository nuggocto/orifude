-- name: CreateAuthChallenge :one
INSERT INTO auth_challenges (
    id, identity_id, public_key, key_thumbprint, purpose, nonce_hash, expires_at
) VALUES (
    sqlc.arg(id), sqlc.narg(identity_id), sqlc.arg(public_key), sqlc.arg(key_thumbprint),
    sqlc.arg(purpose), sqlc.arg(nonce_hash), now() + interval '5 minutes'
)
RETURNING *;

-- name: GetAuthChallengeForUpdate :one
SELECT * FROM auth_challenges
WHERE id = sqlc.arg(id)
FOR UPDATE;

-- name: ConsumeAuthChallenge :one
UPDATE auth_challenges
SET used_at = clock_timestamp()
WHERE id = sqlc.arg(id)
  AND purpose = sqlc.arg(purpose)
  AND used_at IS NULL
  AND expires_at > clock_timestamp()
RETURNING *;

-- name: CreateAccessSession :one
INSERT INTO access_sessions (token_hash, identity_id, key_thumbprint, expires_at)
VALUES (sqlc.arg(token_hash), sqlc.arg(identity_id), sqlc.arg(key_thumbprint), now() + interval '15 minutes')
RETURNING *;

-- name: GetActiveAccessSession :one
SELECT access_sessions.*
FROM access_sessions
JOIN identities ON identities.id = access_sessions.identity_id
WHERE access_sessions.token_hash = sqlc.arg(token_hash)
  AND access_sessions.revoked_at IS NULL
  AND access_sessions.expires_at > now()
  AND identities.deleted_at IS NULL
  AND identities.key_thumbprint = access_sessions.key_thumbprint;

-- name: AccessSessionExpired :one
SELECT EXISTS (
    SELECT 1 FROM access_sessions
    WHERE token_hash = sqlc.arg(token_hash)
      AND expires_at <= now()
      AND revoked_at IS NULL
) AS expired;

-- name: RevokeIdentitySessions :execrows
UPDATE access_sessions
SET revoked_at = now()
WHERE identity_id = sqlc.arg(identity_id) AND revoked_at IS NULL;
