-- name: CreateIdentity :one
WITH reservation AS (
    INSERT INTO alias_reservations (alias_key)
    VALUES (sqlc.arg(alias_key))
    ON CONFLICT DO NOTHING
    RETURNING alias_key
)
INSERT INTO identities (public_key, key_thumbprint, revocation_hash, alias, alias_key)
SELECT sqlc.arg(public_key), sqlc.arg(key_thumbprint), sqlc.arg(revocation_hash), sqlc.arg(alias), reservation.alias_key
FROM reservation
RETURNING *;

-- name: GetIdentityByID :one
SELECT * FROM identities WHERE id = sqlc.arg(id);

-- name: LockIdentity :one
SELECT * FROM identities
WHERE id = sqlc.arg(id)
FOR NO KEY UPDATE;

-- name: LockRegistrationKey :exec
SELECT pg_advisory_xact_lock(hashtextextended(encode(sqlc.arg(key_thumbprint)::bytea, 'hex'), 0));

-- name: GetActiveIdentityByPublicKey :one
SELECT * FROM identities
WHERE public_key = sqlc.arg(public_key) AND deleted_at IS NULL
FOR NO KEY UPDATE;

-- name: GetActiveIdentityByThumbprint :one
SELECT * FROM identities
WHERE key_thumbprint = sqlc.arg(key_thumbprint) AND deleted_at IS NULL;

-- name: GetIdentityByRevocationHashForUpdate :one
SELECT * FROM identities
WHERE revocation_hash = sqlc.arg(revocation_hash) AND deleted_at IS NULL
FOR NO KEY UPDATE;

-- name: LockActiveIdentity :one
SELECT * FROM identities
WHERE id = sqlc.arg(id) AND deleted_at IS NULL
FOR NO KEY UPDATE;

-- name: AliasKeyAvailable :one
SELECT NOT EXISTS (
    SELECT 1 FROM identities WHERE identities.alias_key = sqlc.arg(alias_key)
    UNION ALL
    SELECT 1 FROM alias_reservations WHERE alias_reservations.alias_key = sqlc.arg(alias_key)
) AS available;

-- name: ReserveIdentityAlias :execrows
INSERT INTO alias_reservations (alias_key)
SELECT alias_key FROM identities
WHERE id = sqlc.arg(identity_id) AND alias_key IS NOT NULL
ON CONFLICT DO NOTHING;

-- name: TouchIdentity :execrows
UPDATE identities
SET last_seen_at = now()
WHERE id = sqlc.arg(id) AND deleted_at IS NULL;

-- name: MarkIdentityDeleted :one
UPDATE identities
SET revocation_hash = NULL,
    alias = NULL,
    alias_key = NULL,
    deleted_at = now()
WHERE id = sqlc.arg(id) AND deleted_at IS NULL
RETURNING *;

-- name: LockNextInactiveIdentity :one
SELECT * FROM identities
WHERE deleted_at IS NULL AND last_seen_at <= now() - interval '1 year'
ORDER BY last_seen_at, id
FOR NO KEY UPDATE SKIP LOCKED
LIMIT 1;

-- name: ReleaseIdentityUnopenedClaims :execrows
WITH deleted AS (
    DELETE FROM letters
    WHERE recipient_id = sqlc.arg(identity_id)
      AND opened_at IS NULL
      AND sender_removed_at IS NOT NULL
)
UPDATE letters
SET recipient_id = NULL,
    recipient_alias = NULL,
    claimed_at = NULL,
    claim_expires_at = NULL,
    recipient_removed_at = NULL
WHERE letters.recipient_id = sqlc.arg(identity_id)
  AND letters.opened_at IS NULL
  AND letters.sender_removed_at IS NULL;

-- name: DeleteIdentityWaitingLetters :execrows
DELETE FROM letters
WHERE sender_id = sqlc.arg(identity_id) AND recipient_id IS NULL;

-- name: RemoveIdentityKeepsakes :execrows
UPDATE letters
SET sender_removed_at = CASE WHEN sender_id = sqlc.arg(identity_id) THEN now() ELSE sender_removed_at END,
    recipient_removed_at = CASE WHEN recipient_id = sqlc.arg(identity_id) THEN now() ELSE recipient_removed_at END
WHERE (sender_id = sqlc.arg(identity_id) AND recipient_id IS NOT NULL)
   OR (recipient_id = sqlc.arg(identity_id) AND opened_at IS NOT NULL);

-- name: DeleteFullyRemovedIdentityLetters :execrows
DELETE FROM letters
WHERE (sender_id = sqlc.arg(identity_id) OR recipient_id = sqlc.arg(identity_id))
  AND sender_removed_at IS NOT NULL
  AND (recipient_id IS NULL OR recipient_removed_at IS NOT NULL);

-- name: DeleteIdentityBlocks :execrows
DELETE FROM blocks
WHERE blocker_id = sqlc.arg(identity_id) OR blocked_id = sqlc.arg(identity_id);

-- name: CreateInvite :one
INSERT INTO invites (token_hash, expires_at)
VALUES (sqlc.arg(token_hash), now() + interval '7 days')
RETURNING *;

-- name: GetInviteForUpdate :one
SELECT * FROM invites
WHERE token_hash = sqlc.arg(token_hash)
FOR UPDATE;

-- name: GetRedeemableInviteForUpdate :one
SELECT * FROM invites
WHERE token_hash = sqlc.arg(token_hash)
  AND redeemed_at IS NULL
  AND revoked_at IS NULL
  AND expires_at > clock_timestamp()
FOR UPDATE;

-- name: RedeemInvite :one
UPDATE invites
SET redeemed_at = clock_timestamp(), redeemed_by = sqlc.arg(identity_id)
WHERE token_hash = sqlc.arg(token_hash)
  AND redeemed_at IS NULL
  AND revoked_at IS NULL
  AND expires_at > clock_timestamp()
RETURNING *;

-- name: RevokeInvite :execrows
UPDATE invites
SET revoked_at = now()
WHERE token_hash = sqlc.arg(token_hash)
  AND redeemed_at IS NULL
  AND revoked_at IS NULL;
