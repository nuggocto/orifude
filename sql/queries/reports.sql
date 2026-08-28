-- name: CreateReport :one
INSERT INTO reports (
    id, letter_id, reporter_id, reported_identity_id, target, reason,
    evidence_ciphertext, evidence_nonce, evidence_wrapped_key,
    evidence_kms_key_id, evidence_encryption_version
)
SELECT sqlc.arg(id),
       letters.id,
       sqlc.arg(reporter_id),
       CASE sqlc.arg(target)::smallint WHEN 1 THEN letters.sender_id ELSE letters.recipient_id END,
       sqlc.arg(target),
       sqlc.arg(reason),
       sqlc.arg(evidence_ciphertext),
       sqlc.arg(evidence_nonce),
       sqlc.arg(evidence_wrapped_key),
       sqlc.arg(evidence_kms_key_id),
       sqlc.arg(evidence_encryption_version)
FROM letters
WHERE letters.id = sqlc.arg(letter_id)
  AND letters.opened_at IS NOT NULL
  AND (
      (sqlc.arg(target)::smallint = 1 AND letters.recipient_id = sqlc.arg(reporter_id) AND letters.recipient_removed_at IS NULL)
      OR (sqlc.arg(target)::smallint = 2 AND letters.sender_id = sqlc.arg(reporter_id) AND letters.reply_id IS NOT NULL AND letters.sender_removed_at IS NULL)
  )
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(reporter_id) AND deleted_at IS NULL)
RETURNING reports.*;

-- name: GetReportByIDForReporter :one
SELECT * FROM reports
WHERE reports.id = sqlc.arg(id)
  AND reports.reporter_id = sqlc.arg(reporter_id)
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(reporter_id) AND deleted_at IS NULL);

-- name: GetReportByLetterForReporter :one
SELECT * FROM reports
WHERE reports.letter_id = sqlc.arg(letter_id)
  AND reports.reporter_id = sqlc.arg(reporter_id)
  AND EXISTS (SELECT 1 FROM identities WHERE identities.id = sqlc.arg(reporter_id) AND deleted_at IS NULL);

-- name: HideReportedLetter :execrows
UPDATE letters
SET recipient_removed_at = CASE WHEN reports.target = 1 THEN now() ELSE letters.recipient_removed_at END,
    sender_removed_at = CASE WHEN reports.target = 2 THEN now() ELSE letters.sender_removed_at END
FROM reports
WHERE reports.id = sqlc.arg(report_id)
  AND reports.letter_id = letters.id
  AND reports.reporter_id = sqlc.arg(reporter_id);
