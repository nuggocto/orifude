-- name: InsertDPoPReplay :execrows
INSERT INTO dpop_replays (session_token_hash, jti_hash, expires_at)
SELECT access_sessions.token_hash, sqlc.arg(jti_hash), access_sessions.expires_at
FROM access_sessions
JOIN identities ON identities.id = access_sessions.identity_id
WHERE access_sessions.token_hash = sqlc.arg(session_token_hash)
  AND access_sessions.revoked_at IS NULL
  AND access_sessions.expires_at > now()
  AND identities.deleted_at IS NULL
  AND identities.key_thumbprint = access_sessions.key_thumbprint;
