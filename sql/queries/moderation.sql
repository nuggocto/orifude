-- name: GetModerationAuditByRequestID :one
SELECT * FROM moderation_audit
WHERE request_id = sqlc.arg(request_id);

-- name: LockNextUnreviewedReport :one
SELECT * FROM reports
WHERE reviewed_at IS NULL
  AND evidence_purged_at IS NULL
  AND (evidence_purge_at IS NULL OR evidence_purge_at > clock_timestamp())
ORDER BY created_at, id
FOR UPDATE SKIP LOCKED
LIMIT 1;

-- name: LockReportForReview :one
SELECT * FROM reports
WHERE id = sqlc.arg(id)
FOR UPDATE;

-- name: MarkReportReviewed :one
UPDATE reports
SET reviewed_at = COALESCE(reviewed_at, now()),
    reviewed_by = COALESCE(reviewed_by, sqlc.arg(moderator_subject))
WHERE id = sqlc.arg(id)
  AND (reviewed_by IS NULL OR reviewed_by = sqlc.arg(moderator_subject))
  AND evidence_purged_at IS NULL
  AND (evidence_purge_at IS NULL OR evidence_purge_at > clock_timestamp())
RETURNING *;

-- name: GetReviewableReportForOwner :one
SELECT * FROM reports
WHERE id = sqlc.arg(id)
  AND reviewed_by = sqlc.arg(moderator_subject)
  AND evidence_purged_at IS NULL
  AND (evidence_purge_at IS NULL OR evidence_purge_at > clock_timestamp());

-- name: CreateModerationAudit :one
INSERT INTO moderation_audit (
    request_id, report_id, moderator_subject, action, purpose, outcome, purge_at
) VALUES (
    sqlc.arg(request_id), sqlc.arg(report_id), sqlc.arg(moderator_subject),
    sqlc.arg(action), 'reported-content-review', sqlc.arg(outcome), now() + interval '1 year'
)
RETURNING *;

-- name: LockReportForClose :one
SELECT * FROM reports
WHERE id = sqlc.arg(id)
FOR UPDATE;

-- name: CloseReport :one
UPDATE reports
SET disposition = COALESCE(disposition, sqlc.arg(disposition)),
    closed_at = COALESCE(closed_at, now()),
    evidence_purge_at = COALESCE(evidence_purge_at, now() + interval '90 days'),
    record_purge_at = COALESCE(record_purge_at, now() + interval '1 year')
WHERE id = sqlc.arg(id)
  AND reviewed_at IS NOT NULL
  AND (disposition IS NULL OR disposition = sqlc.arg(disposition))
RETURNING *;

-- name: GetDisabledIdentityFromReport :one
SELECT reported_identity_id FROM reports
WHERE id = sqlc.arg(id) AND disposition = 3 AND closed_at IS NOT NULL;
