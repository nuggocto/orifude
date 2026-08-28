-- name: ListSentKeepsakes :many
SELECT letters.* FROM letters
WHERE letters.sender_id = sqlc.arg(identity_id)
  AND letters.sender_removed_at IS NULL
  AND (letters.opened_at IS NOT NULL OR letters.claim_expires_at > clock_timestamp() OR letters.expires_at > clock_timestamp())
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(identity_id) AND deleted_at IS NULL)
ORDER BY letters.created_at DESC, letters.id DESC
LIMIT sqlc.arg(page_size);

-- name: ListSentKeepsakesAfter :many
SELECT letters.* FROM letters
WHERE letters.sender_id = sqlc.arg(identity_id)
  AND letters.sender_removed_at IS NULL
  AND (letters.opened_at IS NOT NULL OR letters.claim_expires_at > clock_timestamp() OR letters.expires_at > clock_timestamp())
  AND (letters.created_at, letters.id) < (sqlc.arg(cursor_created_at), sqlc.arg(cursor_id)::varchar(22))
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(identity_id) AND deleted_at IS NULL)
ORDER BY letters.created_at DESC, letters.id DESC
LIMIT sqlc.arg(page_size);

-- name: ListReceivedKeepsakes :many
SELECT letters.* FROM letters
WHERE letters.recipient_id = sqlc.arg(identity_id)
  AND letters.opened_at IS NOT NULL
  AND letters.recipient_removed_at IS NULL
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(identity_id) AND deleted_at IS NULL)
ORDER BY letters.created_at DESC, letters.id DESC
LIMIT sqlc.arg(page_size);

-- name: ListReceivedKeepsakesAfter :many
SELECT letters.* FROM letters
WHERE letters.recipient_id = sqlc.arg(identity_id)
  AND letters.opened_at IS NOT NULL
  AND letters.recipient_removed_at IS NULL
  AND (letters.created_at, letters.id) < (sqlc.arg(cursor_created_at), sqlc.arg(cursor_id)::varchar(22))
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(identity_id) AND deleted_at IS NULL)
ORDER BY letters.created_at DESC, letters.id DESC
LIMIT sqlc.arg(page_size);

-- name: RemoveSentKeepsake :one
UPDATE letters
SET sender_removed_at = now()
WHERE letters.id = sqlc.arg(id)
  AND letters.sender_id = sqlc.arg(identity_id)
  AND letters.sender_removed_at IS NULL
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(identity_id) AND deleted_at IS NULL)
RETURNING *;

-- name: RemoveReceivedKeepsake :one
UPDATE letters
SET recipient_removed_at = now()
WHERE letters.id = sqlc.arg(id)
  AND letters.recipient_id = sqlc.arg(identity_id)
  AND letters.opened_at IS NOT NULL
  AND letters.recipient_removed_at IS NULL
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(identity_id) AND deleted_at IS NULL)
RETURNING *;

-- name: DeleteFullyRemovedLetter :execrows
DELETE FROM letters
WHERE letters.id = sqlc.arg(id)
  AND sender_removed_at IS NOT NULL
  AND (recipient_id IS NULL OR recipient_removed_at IS NOT NULL);
