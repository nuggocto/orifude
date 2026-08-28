-- name: GetActiveClaimForUpdate :one
SELECT * FROM letters
WHERE recipient_id = sqlc.arg(recipient_id)
  AND opened_at IS NULL
  AND claim_expires_at > clock_timestamp()
  AND recipient_removed_at IS NULL
ORDER BY claimed_at, id
FOR UPDATE;

-- name: ReleaseExpiredClaimsForIdentity :execrows
UPDATE letters
SET recipient_id = NULL,
    recipient_alias = NULL,
    claimed_at = NULL,
    claim_expires_at = NULL,
    recipient_removed_at = NULL
WHERE recipient_id = sqlc.arg(recipient_id)
  AND opened_at IS NULL
  AND claim_expires_at <= clock_timestamp();

-- name: SelectEligibleLetterForClaim :one
SELECT * FROM letters
WHERE recipient_id IS NULL
  AND withdrawn_at IS NULL
  AND expires_at > clock_timestamp()
  AND sender_id <> sqlc.arg(recipient_id)
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = letters.sender_id AND identities.deleted_at IS NULL)
  AND NOT EXISTS (SELECT 1 FROM reports WHERE reports.letter_id = letters.id)
  AND NOT EXISTS (
      SELECT 1 FROM blocks
      WHERE (blocker_id = sqlc.arg(recipient_id) AND blocked_id = letters.sender_id)
         OR (blocker_id = letters.sender_id AND blocked_id = sqlc.arg(recipient_id))
  )
ORDER BY created_at, id
FOR UPDATE SKIP LOCKED
LIMIT 1;

-- name: AssignLetterClaim :one
WITH claim_time AS (SELECT clock_timestamp() AS value)
UPDATE letters
SET recipient_id = sqlc.arg(recipient_id),
    recipient_alias = sqlc.arg(recipient_alias),
    claimed_at = claim_time.value,
    claim_expires_at = claim_time.value + interval '24 hours'
FROM claim_time
WHERE letters.id = sqlc.arg(id)
  AND letters.recipient_id IS NULL
  AND letters.withdrawn_at IS NULL
  AND letters.expires_at > claim_time.value
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = letters.sender_id AND identities.deleted_at IS NULL)
  AND NOT EXISTS (SELECT 1 FROM reports WHERE reports.letter_id = letters.id)
  AND NOT EXISTS (
      SELECT 1 FROM blocks
      WHERE (blocker_id = sqlc.arg(recipient_id) AND blocked_id = letters.sender_id)
         OR (blocker_id = letters.sender_id AND blocked_id = sqlc.arg(recipient_id))
  )
RETURNING letters.*;
