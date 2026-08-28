-- name: DeleteExpiredAuthChallenges :execrows
DELETE FROM auth_challenges
WHERE expires_at <= now() OR used_at IS NOT NULL;

-- name: DeleteExpiredAuthChallengesBatch :many
WITH expired AS (
    SELECT id FROM auth_challenges
    WHERE expires_at <= now() OR used_at IS NOT NULL
    ORDER BY expires_at, id
    FOR UPDATE SKIP LOCKED
    LIMIT sqlc.arg(batch_size)
)
DELETE FROM auth_challenges
USING expired
WHERE auth_challenges.id = expired.id
RETURNING auth_challenges.id;

-- name: DeleteExpiredAccessSessions :execrows
DELETE FROM access_sessions
WHERE expires_at <= now() OR revoked_at IS NOT NULL;

-- name: DeleteExpiredAccessSessionsBatch :many
WITH expired AS (
    SELECT token_hash FROM access_sessions
    WHERE expires_at <= now() OR revoked_at IS NOT NULL
    ORDER BY expires_at, token_hash
    FOR UPDATE SKIP LOCKED
    LIMIT sqlc.arg(batch_size)
)
DELETE FROM access_sessions
USING expired
WHERE access_sessions.token_hash = expired.token_hash
RETURNING access_sessions.token_hash;

-- name: DeleteExpiredDPoPReplays :execrows
DELETE FROM dpop_replays
WHERE expires_at <= now();

-- name: DeleteExpiredDPoPReplaysBatch :many
WITH expired AS (
    SELECT session_token_hash, jti_hash FROM dpop_replays
    WHERE expires_at <= now()
    ORDER BY expires_at, session_token_hash, jti_hash
    FOR UPDATE SKIP LOCKED
    LIMIT sqlc.arg(batch_size)
)
DELETE FROM dpop_replays
USING expired
WHERE dpop_replays.session_token_hash = expired.session_token_hash
  AND dpop_replays.jti_hash = expired.jti_hash
RETURNING dpop_replays.jti_hash;

-- name: ReleaseExpiredClaims :many
WITH expired AS (
    SELECT id FROM letters
    WHERE opened_at IS NULL AND claim_expires_at <= now()
    ORDER BY claim_expires_at, id
    FOR UPDATE SKIP LOCKED
    LIMIT sqlc.arg(batch_size)
), deleted AS (
    DELETE FROM letters
    USING expired
    WHERE letters.id = expired.id
      AND letters.sender_removed_at IS NOT NULL
    RETURNING letters.id
), released AS (
    UPDATE letters
    SET recipient_id = NULL,
        recipient_alias = NULL,
        claimed_at = NULL,
        claim_expires_at = NULL,
        recipient_removed_at = NULL
    FROM expired
    WHERE letters.id = expired.id
      AND letters.sender_removed_at IS NULL
    RETURNING letters.id
)
SELECT id FROM deleted
UNION ALL
SELECT id FROM released;

-- name: DeleteExpiredWaitingLetters :many
WITH expired AS (
    SELECT id FROM letters
    WHERE recipient_id IS NULL
      AND withdrawn_at IS NULL
      AND expires_at <= now()
    ORDER BY expires_at, id
    FOR UPDATE SKIP LOCKED
    LIMIT sqlc.arg(batch_size)
)
DELETE FROM letters
USING expired
WHERE letters.id = expired.id
RETURNING letters.id;

-- name: DeleteExpiredWithdrawnLetters :many
WITH expired AS (
    SELECT id FROM letters
    WHERE recipient_id IS NULL
      AND withdrawn_at IS NOT NULL
      AND expires_at <= now()
    ORDER BY expires_at, id
    FOR UPDATE SKIP LOCKED
    LIMIT sqlc.arg(batch_size)
)
DELETE FROM letters
USING expired
WHERE letters.id = expired.id
RETURNING letters.id;

-- name: LockNextReportForEvidencePurge :one
SELECT * FROM reports
WHERE evidence_purge_at IS NOT NULL
  AND evidence_purge_at <= now()
  AND evidence_purged_at IS NULL
ORDER BY evidence_purge_at, id
FOR UPDATE SKIP LOCKED
LIMIT 1;

-- name: PurgeReportEvidence :one
UPDATE reports
SET evidence_ciphertext = NULL,
    evidence_nonce = NULL,
    evidence_wrapped_key = NULL,
    evidence_kms_key_id = NULL,
    evidence_encryption_version = NULL,
    evidence_purged_at = now()
WHERE id = sqlc.arg(id)
  AND evidence_purge_at <= now()
  AND evidence_purged_at IS NULL
RETURNING *;

-- name: DeleteExpiredReports :many
WITH expired AS (
    SELECT id FROM reports
    WHERE record_purge_at <= now()
    ORDER BY record_purge_at, id
    FOR UPDATE SKIP LOCKED
    LIMIT sqlc.arg(batch_size)
)
DELETE FROM reports
USING expired
WHERE reports.id = expired.id
RETURNING reports.id;

-- name: DeleteExpiredModerationAudit :many
WITH expired AS (
    SELECT id FROM moderation_audit
    WHERE purge_at <= now()
    ORDER BY purge_at, id
    FOR UPDATE SKIP LOCKED
    LIMIT sqlc.arg(batch_size)
)
DELETE FROM moderation_audit
USING expired
WHERE moderation_audit.id = expired.id
RETURNING moderation_audit.id;
