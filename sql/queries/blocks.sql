-- name: CreateBlockFromLetter :execrows
INSERT INTO blocks (blocker_id, blocked_id, created_at)
SELECT sqlc.arg(identity_id),
       CASE WHEN sender_id = sqlc.arg(identity_id) THEN recipient_id ELSE sender_id END,
       clock_timestamp()
FROM letters
WHERE letters.id = sqlc.arg(letter_id)
  AND opened_at IS NOT NULL
  AND recipient_id IS NOT NULL
  AND sqlc.arg(identity_id) IN (sender_id, recipient_id)
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(identity_id) AND deleted_at IS NULL)
  AND EXISTS (
      SELECT 1 FROM identities
      WHERE identities.id = CASE WHEN sender_id = sqlc.arg(identity_id) THEN recipient_id ELSE sender_id END
        AND deleted_at IS NULL
  )
ON CONFLICT DO NOTHING;

-- name: GetBlock :one
SELECT * FROM blocks
WHERE blocker_id = sqlc.arg(blocker_id) AND blocked_id = sqlc.arg(blocked_id);
