-- name: CreateLetter :one
INSERT INTO letters (
    id, sender_id, sender_alias, body_ciphertext, body_nonce, body_wrapped_key,
    body_kms_key_id, body_encryption_version, fold_seed, expires_at
) VALUES (
    sqlc.arg(id), sqlc.arg(sender_id), sqlc.arg(sender_alias), sqlc.arg(body_ciphertext),
    sqlc.arg(body_nonce), sqlc.arg(body_wrapped_key), sqlc.arg(body_kms_key_id),
    sqlc.arg(body_encryption_version), sqlc.arg(fold_seed), now() + interval '7 days'
)
RETURNING *;

-- name: GetLetterForSender :one
SELECT letters.* FROM letters
JOIN identities ON identities.id = sqlc.arg(sender_id) AND identities.deleted_at IS NULL
WHERE letters.id = sqlc.arg(id)
  AND letters.sender_id = sqlc.arg(sender_id)
  AND letters.sender_removed_at IS NULL
  AND (letters.recipient_id IS NOT NULL OR letters.expires_at > clock_timestamp());

-- name: GetLetterForRecipient :one
SELECT letters.* FROM letters
JOIN identities ON identities.id = sqlc.arg(recipient_id) AND identities.deleted_at IS NULL
WHERE letters.id = sqlc.arg(id)
  AND letters.recipient_id = sqlc.arg(recipient_id)
  AND letters.opened_at IS NOT NULL
  AND letters.recipient_removed_at IS NULL;

-- name: GetLetterForOpen :one
SELECT letters.* FROM letters
JOIN identities ON identities.id = sqlc.arg(recipient_id) AND identities.deleted_at IS NULL
WHERE letters.id = sqlc.arg(id)
  AND letters.recipient_id = sqlc.arg(recipient_id)
  AND letters.recipient_removed_at IS NULL
  AND (letters.opened_at IS NOT NULL OR letters.claim_expires_at > now());

-- name: ExpiredClaimExistsForRecipient :one
SELECT EXISTS (
    SELECT 1 FROM letters
    JOIN identities ON identities.id = sqlc.arg(recipient_id) AND identities.deleted_at IS NULL
    WHERE letters.id = sqlc.arg(id)
      AND letters.recipient_id = sqlc.arg(recipient_id)
      AND letters.opened_at IS NULL
      AND letters.claim_expires_at <= now()
) AS expired;

-- name: LockLetterForSender :one
SELECT * FROM letters
WHERE id = sqlc.arg(id) AND sender_id = sqlc.arg(sender_id)
FOR UPDATE;

-- name: LockLetterForRecipient :one
SELECT * FROM letters
WHERE id = sqlc.arg(id) AND recipient_id = sqlc.arg(recipient_id)
FOR UPDATE;

-- name: WithdrawLetter :one
UPDATE letters
SET withdrawn_at = now()
WHERE letters.id = sqlc.arg(id)
  AND letters.sender_id = sqlc.arg(sender_id)
  AND letters.recipient_id IS NULL
  AND letters.withdrawn_at IS NULL
  AND letters.expires_at > clock_timestamp()
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(sender_id) AND deleted_at IS NULL)
RETURNING *;

-- name: OpenLetter :one
UPDATE letters
SET opened_at = now(), claim_expires_at = NULL
WHERE letters.id = sqlc.arg(id)
  AND letters.recipient_id = sqlc.arg(recipient_id)
  AND letters.opened_at IS NULL
  AND letters.claim_expires_at > clock_timestamp()
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(recipient_id) AND deleted_at IS NULL)
RETURNING *;

-- name: AddLetterReply :one
UPDATE letters
SET reply_id = sqlc.arg(reply_id),
    reply_ciphertext = sqlc.arg(reply_ciphertext),
    reply_nonce = sqlc.arg(reply_nonce),
    reply_wrapped_key = sqlc.arg(reply_wrapped_key),
    reply_kms_key_id = sqlc.arg(reply_kms_key_id),
    reply_encryption_version = sqlc.arg(reply_encryption_version),
    replied_at = now()
WHERE letters.id = sqlc.arg(id)
  AND letters.recipient_id = sqlc.arg(recipient_id)
  AND letters.opened_at IS NOT NULL
  AND letters.reply_id IS NULL
  AND letters.recipient_removed_at IS NULL
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(recipient_id) AND deleted_at IS NULL)
RETURNING *;

-- name: GetLetterByReplyIDForRecipient :one
SELECT * FROM letters
WHERE letters.recipient_id = sqlc.arg(recipient_id)
  AND letters.reply_id = sqlc.arg(reply_id)
  AND letters.recipient_removed_at IS NULL
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(recipient_id) AND deleted_at IS NULL);
