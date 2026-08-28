-- +goose Up

CREATE TABLE identities (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_key bytea NOT NULL UNIQUE CHECK (octet_length(public_key) = 65),
    key_thumbprint bytea NOT NULL UNIQUE CHECK (octet_length(key_thumbprint) = 32),
    revocation_hash bytea UNIQUE CHECK (revocation_hash IS NULL OR octet_length(revocation_hash) = 32),
    alias text UNIQUE CHECK (alias IS NULL OR (char_length(alias) BETWEEN 2 AND 24 AND octet_length(alias) <= 96)),
    alias_key text UNIQUE CHECK (alias_key IS NULL OR octet_length(alias_key) BETWEEN 1 AND 512),
    created_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz,
    UNIQUE (id, key_thumbprint),
    CHECK (last_seen_at >= created_at),
    CHECK (deleted_at IS NULL OR deleted_at >= created_at),
    CHECK (
        (deleted_at IS NULL AND revocation_hash IS NOT NULL AND alias IS NOT NULL AND alias_key IS NOT NULL)
        OR (deleted_at IS NOT NULL AND revocation_hash IS NULL AND alias IS NULL AND alias_key IS NULL)
    )
);

CREATE TABLE alias_reservations (
    alias_key text PRIMARY KEY CHECK (octet_length(alias_key) BETWEEN 1 AND 512),
    reserved_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE auth_challenges (
    id varchar(22) PRIMARY KEY CHECK (id ~ '^[A-Za-z0-9_-]{21}[AQgw]$'),
    identity_id bigint REFERENCES identities (id) ON DELETE CASCADE,
    public_key bytea NOT NULL CHECK (octet_length(public_key) = 65),
    key_thumbprint bytea NOT NULL CHECK (octet_length(key_thumbprint) = 32),
    purpose smallint NOT NULL CHECK (purpose IN (1, 2)),
    nonce_hash bytea NOT NULL CHECK (octet_length(nonce_hash) = 32),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    used_at timestamptz,
    CHECK ((purpose = 1 AND identity_id IS NULL) OR (purpose = 2 AND identity_id IS NOT NULL)),
    CHECK (expires_at = created_at + interval '5 minutes'),
    CHECK (used_at IS NULL OR used_at BETWEEN created_at AND expires_at)
);

CREATE TABLE access_sessions (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    identity_id bigint NOT NULL,
    key_thumbprint bytea NOT NULL CHECK (octet_length(key_thumbprint) = 32),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    FOREIGN KEY (identity_id, key_thumbprint) REFERENCES identities (id, key_thumbprint) ON DELETE CASCADE,
    CHECK (expires_at = created_at + interval '15 minutes'),
    CHECK (revoked_at IS NULL OR revoked_at >= created_at)
);

CREATE TABLE dpop_replays (
    session_token_hash bytea NOT NULL REFERENCES access_sessions (token_hash) ON DELETE CASCADE,
    jti_hash bytea NOT NULL CHECK (octet_length(jti_hash) = 32),
    expires_at timestamptz NOT NULL,
    PRIMARY KEY (session_token_hash, jti_hash)
);

CREATE TABLE invites (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    redeemed_at timestamptz,
    redeemed_by bigint REFERENCES identities (id),
    revoked_at timestamptz,
    CHECK (expires_at = created_at + interval '7 days'),
    CHECK ((redeemed_at IS NULL) = (redeemed_by IS NULL)),
    CHECK (redeemed_at IS NULL OR redeemed_at BETWEEN created_at AND expires_at),
    CHECK (revoked_at IS NULL OR (redeemed_at IS NULL AND revoked_at >= created_at))
);

CREATE TABLE letters (
    id varchar(22) PRIMARY KEY CHECK (id ~ '^[A-Za-z0-9_-]{21}[AQgw]$'),
    sender_id bigint NOT NULL REFERENCES identities (id),
    recipient_id bigint REFERENCES identities (id),
    sender_alias text NOT NULL CHECK (char_length(sender_alias) BETWEEN 2 AND 24 AND octet_length(sender_alias) <= 96),
    recipient_alias text CHECK (recipient_alias IS NULL OR (char_length(recipient_alias) BETWEEN 2 AND 24 AND octet_length(recipient_alias) <= 96)),
    body_ciphertext bytea NOT NULL CHECK (octet_length(body_ciphertext) BETWEEN 17 AND 12304),
    body_nonce bytea NOT NULL CHECK (octet_length(body_nonce) = 12),
    body_wrapped_key bytea NOT NULL CHECK (octet_length(body_wrapped_key) BETWEEN 1 AND 6144),
    body_kms_key_id text NOT NULL CHECK (octet_length(body_kms_key_id) BETWEEN 1 AND 2048),
    body_encryption_version smallint NOT NULL CHECK (body_encryption_version > 0),
    fold_seed bigint NOT NULL CHECK (fold_seed >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    claimed_at timestamptz,
    claim_expires_at timestamptz,
    opened_at timestamptz,
    reply_id varchar(22) UNIQUE CHECK (reply_id IS NULL OR reply_id ~ '^[A-Za-z0-9_-]{21}[AQgw]$'),
    reply_ciphertext bytea CHECK (reply_ciphertext IS NULL OR octet_length(reply_ciphertext) BETWEEN 17 AND 12304),
    reply_nonce bytea CHECK (reply_nonce IS NULL OR octet_length(reply_nonce) = 12),
    reply_wrapped_key bytea CHECK (reply_wrapped_key IS NULL OR octet_length(reply_wrapped_key) BETWEEN 1 AND 6144),
    reply_kms_key_id text CHECK (reply_kms_key_id IS NULL OR octet_length(reply_kms_key_id) BETWEEN 1 AND 2048),
    reply_encryption_version smallint CHECK (reply_encryption_version IS NULL OR reply_encryption_version > 0),
    replied_at timestamptz,
    withdrawn_at timestamptz,
    expires_at timestamptz NOT NULL,
    sender_removed_at timestamptz,
    recipient_removed_at timestamptz,
    CHECK (expires_at = created_at + interval '7 days'),
    CHECK (recipient_id IS NULL OR sender_id <> recipient_id),
    CHECK (
        (recipient_id IS NULL AND recipient_alias IS NULL AND claimed_at IS NULL AND claim_expires_at IS NULL AND opened_at IS NULL AND recipient_removed_at IS NULL)
        OR (
            recipient_id IS NOT NULL
            AND recipient_alias IS NOT NULL
            AND claimed_at IS NOT NULL
            AND (
                (opened_at IS NULL AND claim_expires_at IS NOT NULL)
                OR (opened_at IS NOT NULL AND claim_expires_at IS NULL)
            )
        )
    ),
    CHECK (claimed_at IS NULL OR claimed_at >= created_at),
    CHECK (claim_expires_at IS NULL OR claim_expires_at = claimed_at + interval '24 hours'),
    CHECK (opened_at IS NULL OR opened_at >= claimed_at),
    CHECK (recipient_removed_at IS NULL OR opened_at IS NOT NULL),
    CHECK (
        (reply_id IS NULL AND reply_ciphertext IS NULL AND reply_nonce IS NULL AND reply_wrapped_key IS NULL AND reply_kms_key_id IS NULL AND reply_encryption_version IS NULL AND replied_at IS NULL)
        OR (reply_id IS NOT NULL AND reply_ciphertext IS NOT NULL AND reply_nonce IS NOT NULL AND reply_wrapped_key IS NOT NULL AND reply_kms_key_id IS NOT NULL AND reply_encryption_version IS NOT NULL AND replied_at IS NOT NULL AND opened_at IS NOT NULL)
    ),
    CHECK (replied_at IS NULL OR replied_at >= opened_at),
    CHECK (withdrawn_at IS NULL OR (recipient_id IS NULL AND opened_at IS NULL AND reply_id IS NULL AND withdrawn_at >= created_at)),
    CHECK (sender_removed_at IS NULL OR sender_removed_at >= created_at)
);

CREATE TABLE blocks (
    blocker_id bigint NOT NULL REFERENCES identities (id) ON DELETE CASCADE,
    blocked_id bigint NOT NULL REFERENCES identities (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (blocker_id, blocked_id),
    CHECK (blocker_id <> blocked_id)
);

CREATE TABLE reports (
    id varchar(22) PRIMARY KEY CHECK (id ~ '^[A-Za-z0-9_-]{21}[AQgw]$'),
    letter_id varchar(22) NOT NULL CHECK (letter_id ~ '^[A-Za-z0-9_-]{21}[AQgw]$'),
    reporter_id bigint NOT NULL,
    reported_identity_id bigint NOT NULL,
    target smallint NOT NULL CHECK (target IN (1, 2)),
    reason smallint NOT NULL CHECK (reason BETWEEN 1 AND 7),
    created_at timestamptz NOT NULL DEFAULT now(),
    reviewed_at timestamptz,
    reviewed_by text CHECK (reviewed_by IS NULL OR octet_length(reviewed_by) BETWEEN 1 AND 512),
    disposition smallint CHECK (disposition IS NULL OR disposition IN (1, 2, 3)),
    closed_at timestamptz,
    evidence_purge_at timestamptz,
    record_purge_at timestamptz,
    evidence_ciphertext bytea CHECK (evidence_ciphertext IS NULL OR octet_length(evidence_ciphertext) BETWEEN 17 AND 12304),
    evidence_nonce bytea CHECK (evidence_nonce IS NULL OR octet_length(evidence_nonce) = 12),
    evidence_wrapped_key bytea CHECK (evidence_wrapped_key IS NULL OR octet_length(evidence_wrapped_key) BETWEEN 1 AND 6144),
    evidence_kms_key_id text CHECK (evidence_kms_key_id IS NULL OR octet_length(evidence_kms_key_id) BETWEEN 1 AND 2048),
    evidence_encryption_version smallint CHECK (evidence_encryption_version IS NULL OR evidence_encryption_version > 0),
    evidence_purged_at timestamptz,
    UNIQUE (letter_id, reporter_id),
    CHECK (reporter_id <> reported_identity_id),
    CHECK ((reviewed_at IS NULL) = (reviewed_by IS NULL)),
    CHECK (reviewed_at IS NULL OR reviewed_at >= created_at),
    CHECK (
        (disposition IS NULL AND closed_at IS NULL AND evidence_purge_at IS NULL AND record_purge_at IS NULL)
        OR (disposition IS NOT NULL AND closed_at IS NOT NULL AND evidence_purge_at IS NOT NULL AND record_purge_at IS NOT NULL AND reviewed_at IS NOT NULL)
    ),
    CHECK (closed_at IS NULL OR closed_at >= reviewed_at),
    CHECK (evidence_purge_at IS NULL OR evidence_purge_at = closed_at + interval '90 days'),
    CHECK (record_purge_at IS NULL OR record_purge_at = closed_at + interval '1 year'),
    CHECK (
        (evidence_purged_at IS NULL AND evidence_ciphertext IS NOT NULL AND evidence_nonce IS NOT NULL AND evidence_wrapped_key IS NOT NULL AND evidence_kms_key_id IS NOT NULL AND evidence_encryption_version IS NOT NULL)
        OR (evidence_purged_at IS NOT NULL AND evidence_ciphertext IS NULL AND evidence_nonce IS NULL AND evidence_wrapped_key IS NULL AND evidence_kms_key_id IS NULL AND evidence_encryption_version IS NULL AND evidence_purge_at IS NOT NULL)
    ),
    CHECK (evidence_purged_at IS NULL OR evidence_purged_at >= evidence_purge_at)
);

CREATE TABLE moderation_audit (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id varchar(22) NOT NULL UNIQUE CHECK (request_id ~ '^[A-Za-z0-9_-]{21}[AQgw]$'),
    report_id varchar(22) NOT NULL CHECK (report_id ~ '^[A-Za-z0-9_-]{21}[AQgw]$'),
    moderator_subject text NOT NULL CHECK (octet_length(moderator_subject) BETWEEN 1 AND 512),
    action smallint NOT NULL CHECK (action IN (1, 2)),
    purpose text NOT NULL CHECK (purpose = 'reported-content-review'),
    outcome smallint NOT NULL CHECK (outcome IN (1, 2)),
    created_at timestamptz NOT NULL DEFAULT now(),
    purge_at timestamptz NOT NULL,
    CHECK (purge_at = created_at + interval '1 year')
);

CREATE TABLE rate_limit_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    identity_id bigint NOT NULL REFERENCES identities (id) ON DELETE CASCADE,
    kind smallint NOT NULL CHECK (kind IN (1, 2, 3)),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX letters_waiting_idx
    ON letters (created_at, id)
    WHERE recipient_id IS NULL
      AND withdrawn_at IS NULL;

CREATE INDEX letters_expiry_idx
    ON letters (expires_at, id)
    WHERE recipient_id IS NULL
      AND withdrawn_at IS NULL;

CREATE INDEX letters_sender_idx
    ON letters (sender_id, created_at DESC, id DESC);

CREATE INDEX letters_recipient_idx
    ON letters (recipient_id, created_at DESC, id DESC)
    WHERE recipient_id IS NOT NULL;

CREATE INDEX reports_unreviewed_idx
    ON reports (created_at, id)
    WHERE reviewed_at IS NULL;

CREATE INDEX identities_inactive_idx
    ON identities (last_seen_at, id)
    WHERE deleted_at IS NULL;

CREATE INDEX auth_challenges_expiry_idx
    ON auth_challenges (expires_at, id);

CREATE INDEX access_sessions_identity_idx
    ON access_sessions (identity_id, expires_at);

CREATE INDEX access_sessions_expiry_idx
    ON access_sessions (expires_at, token_hash);

CREATE INDEX dpop_replays_expiry_idx
    ON dpop_replays (expires_at, session_token_hash);

CREATE INDEX reports_evidence_purge_idx
    ON reports (evidence_purge_at, id)
    WHERE evidence_purge_at IS NOT NULL
      AND evidence_purged_at IS NULL;

CREATE INDEX reports_record_purge_idx
    ON reports (record_purge_at, id)
    WHERE record_purge_at IS NOT NULL;

CREATE INDEX moderation_audit_retention_idx
    ON moderation_audit (purge_at, id);

CREATE INDEX rate_limit_events_identity_idx
    ON rate_limit_events (identity_id, kind, created_at);

CREATE INDEX rate_limit_events_retention_idx
    ON rate_limit_events (created_at, id);

-- +goose Down

-- Down is for disposable and unreleased databases. Released production schemas use a forward repair migration.

DROP TABLE rate_limit_events;
DROP TABLE moderation_audit;
DROP TABLE reports;
DROP TABLE blocks;
DROP TABLE letters;
DROP TABLE invites;
DROP TABLE dpop_replays;
DROP TABLE access_sessions;
DROP TABLE auth_challenges;
DROP TABLE alias_reservations;
DROP TABLE identities;
