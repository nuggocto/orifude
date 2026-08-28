package api

import "time"

type ChallengePurpose string

const (
	ChallengePurposeRegistration ChallengePurpose = "registration"
	ChallengePurposeSession      ChallengePurpose = "session"
)

type TokenType string

const TokenTypeDPoP TokenType = "DPoP"

type LetterState string

const (
	LetterStateWaiting   LetterState = "waiting"
	LetterStateClaimed   LetterState = "claimed"
	LetterStateOpened    LetterState = "opened"
	LetterStateReplied   LetterState = "replied"
	LetterStateWithdrawn LetterState = "withdrawn"
	LetterStateReported  LetterState = "reported"
)

type LetterRole string

const (
	LetterRoleSender    LetterRole = "sender"
	LetterRoleRecipient LetterRole = "recipient"
)

type ReportTarget string

const (
	ReportTargetOriginal ReportTarget = "original"
	ReportTargetReply    ReportTarget = "reply"
)

type ReportReason string

const (
	ReportReasonHarassment                 ReportReason = "harassment"
	ReportReasonHatefulContent             ReportReason = "hateful_content"
	ReportReasonSexualContent              ReportReason = "sexual_content"
	ReportReasonThreats                    ReportReason = "threats"
	ReportReasonSpamOrScams                ReportReason = "spam_or_scams"
	ReportReasonExposedPersonalInformation ReportReason = "exposed_personal_information"
	ReportReasonOtherUnsafeContent         ReportReason = "other_unsafe_content"
)

type ModerationDisposition string

const (
	ModerationDispositionNoAction         ModerationDisposition = "no_action"
	ModerationDispositionDuplicate        ModerationDisposition = "duplicate"
	ModerationDispositionIdentityDisabled ModerationDisposition = "identity_disabled"
)

type ModerationPurpose string

const ModerationPurposeReportedContentReview ModerationPurpose = "reported-content-review"

type ErrorCode string

const (
	ErrorCodeInvalidRequest       ErrorCode = "invalid_request"
	ErrorCodeInvalidProof         ErrorCode = "invalid_proof"
	ErrorCodeAuthenticationFailed ErrorCode = "authentication_failed"
	ErrorCodeSessionExpired       ErrorCode = "session_expired"
	ErrorCodeClockSkew            ErrorCode = "clock_skew"
	ErrorCodeDPoPReplay           ErrorCode = "dpop_replay"
	ErrorCodeConflict             ErrorCode = "conflict"
	ErrorCodeIdentityConflict     ErrorCode = "identity_conflict"
	ErrorCodeInviteInvalid        ErrorCode = "invite_invalid"
	ErrorCodeAliasInvalid         ErrorCode = "alias_invalid"
	ErrorCodeNotFound             ErrorCode = "not_found"
	ErrorCodeRateLimited          ErrorCode = "rate_limited"
	ErrorCodeServiceUnavailable   ErrorCode = "service_unavailable"
	ErrorCodeLetterAlreadyReplied ErrorCode = "letter_already_replied"
	ErrorCodeLetterNotClaimed     ErrorCode = "letter_not_claimed"
	ErrorCodeClaimExpired         ErrorCode = "claim_expired"
	ErrorCodeReportAlreadyExists  ErrorCode = "report_already_exists"
	ErrorCodeEvidenceExpired      ErrorCode = "evidence_expired"
	ErrorCodeReportNotReviewed    ErrorCode = "report_not_reviewed"
	ErrorCodeReportAlreadyClosed  ErrorCode = "report_already_closed"
)

type APIError struct {
	Code    ErrorCode `json:"code"`
	Message string    `json:"message"`
}

func (e APIError) Error() string { return e.Message }

type ErrorResponse struct {
	Error APIError `json:"error"`
}

// PublicJWK is the registration/session challenge key representation. DPoP
// proofs carry the same key without alg in their protected JOSE header.
type PublicJWK struct {
	KeyType   string `json:"kty"`
	Curve     string `json:"crv"`
	X         string `json:"x"`
	Y         string `json:"y"`
	Algorithm string `json:"alg"`
}

type CreateChallengeRequest struct {
	Purpose   ChallengePurpose `json:"purpose"`
	PublicJWK PublicJWK        `json:"public_jwk"`
}

type CreateChallengeResponse struct {
	ChallengeID string    `json:"challenge_id"`
	Nonce       string    `json:"nonce"`
	ExpiresIn   int       `json:"expires_in"`
	ServerTime  time.Time `json:"server_time"`
}

type CreateIdentityRequest struct {
	ChallengeID    string `json:"challenge_id"`
	Alias          string `json:"alias"`
	InviteCode     string `json:"invite_code,omitempty"`
	RevocationHash string `json:"revocation_hash"`
}

type CreateIdentityResponse struct {
	TokenType   TokenType `json:"token_type"`
	AccessToken string    `json:"access_token"`
	ExpiresIn   int       `json:"expires_in"`
}

type CreateSessionRequest struct {
	ChallengeID string `json:"challenge_id"`
}

type CreateSessionResponse struct {
	TokenType   TokenType `json:"token_type"`
	AccessToken string    `json:"access_token"`
	ExpiresIn   int       `json:"expires_in"`
}

type RevokeIdentityRequest struct {
	RevocationCredential string `json:"revocation_credential"`
}

type RevokeIdentityResponse struct{}

type GetMeRequest struct{}

type Limits struct {
	BodyCodePoints int `json:"body_code_points"`
	BodyBytes      int `json:"body_bytes"`
	RequestBytes   int `json:"request_bytes"`
}

type GetMeResponse struct {
	Alias            string `json:"alias"`
	LatestTUIVersion string `json:"latest_tui_version"`
	Limits           Limits `json:"limits"`
}

type DeleteIdentityRequest struct{}

type DeleteIdentityResponse struct{}

type CreateLetterRequest struct {
	LetterID string `json:"letter_id"`
	Body     string `json:"body"`
}

type CreateLetterResponse struct {
	LetterID  string      `json:"letter_id"`
	State     LetterState `json:"state"`
	FoldSeed  int64       `json:"fold_seed"`
	CreatedAt time.Time   `json:"created_at"`
	ExpiresAt time.Time   `json:"expires_at"`
}

type ClaimLetterRequest struct{}

type ClaimLetterResponse struct {
	LetterID       string    `json:"letter_id"`
	FoldSeed       int64     `json:"fold_seed"`
	CreatedAt      time.Time `json:"created_at"`
	ClaimExpiresAt time.Time `json:"claim_expires_at"`
}

type GetLetterRequest struct {
	LetterID string `json:"-"`
}

type Message struct {
	Body      string    `json:"body"`
	Alias     string    `json:"alias"`
	CreatedAt time.Time `json:"created_at"`
}

type GetLetterResponse struct {
	LetterID       string      `json:"letter_id"`
	Role           LetterRole  `json:"role"`
	State          LetterState `json:"state"`
	OtherAlias     string      `json:"other_alias,omitempty"`
	FoldSeed       int64       `json:"fold_seed"`
	CreatedAt      time.Time   `json:"created_at"`
	ClaimExpiresAt *time.Time  `json:"claim_expires_at,omitempty"`
	OpenedAt       *time.Time  `json:"opened_at,omitempty"`
	RepliedAt      *time.Time  `json:"replied_at,omitempty"`
	Original       *Message    `json:"original,omitempty"`
	Reply          *Message    `json:"reply,omitempty"`
}

type OpenLetterRequest struct{}

type OpenLetterResponse struct {
	LetterID string    `json:"letter_id"`
	OpenedAt time.Time `json:"opened_at"`
	Original Message   `json:"original"`
}

type ReplyToLetterRequest struct {
	ReplyID string `json:"reply_id"`
	Body    string `json:"body"`
}

type ReplyToLetterResponse struct {
	LetterID  string    `json:"letter_id"`
	ReplyID   string    `json:"reply_id"`
	RepliedAt time.Time `json:"replied_at"`
}

type WithdrawLetterRequest struct{}

type WithdrawLetterResponse struct {
	LetterID    string    `json:"letter_id"`
	WithdrawnAt time.Time `json:"withdrawn_at"`
}

type ReportLetterRequest struct {
	ReportID string       `json:"report_id"`
	Target   ReportTarget `json:"target"`
	Reason   ReportReason `json:"reason"`
}

type ReportLetterResponse struct {
	ReportID  string    `json:"report_id"`
	CreatedAt time.Time `json:"created_at"`
}

type BlockLetterRequest struct{}

type BlockLetterResponse struct {
	LetterID  string    `json:"letter_id"`
	BlockedAt time.Time `json:"blocked_at"`
}

type ListKeepsakesRequest struct {
	Cursor string `json:"-"`
	Limit  int    `json:"-"`
}

type LetterSummary struct {
	LetterID       string      `json:"letter_id"`
	Role           LetterRole  `json:"role"`
	State          LetterState `json:"state"`
	OtherAlias     string      `json:"other_alias,omitempty"`
	FoldSeed       int64       `json:"fold_seed"`
	CreatedAt      time.Time   `json:"created_at"`
	ClaimExpiresAt *time.Time  `json:"claim_expires_at,omitempty"`
	OpenedAt       *time.Time  `json:"opened_at,omitempty"`
	RepliedAt      *time.Time  `json:"replied_at,omitempty"`
}

type ListKeepsakesResponse struct {
	Keepsakes  []LetterSummary `json:"keepsakes"`
	NextCursor string          `json:"next_cursor,omitempty"`
}

type DeleteKeepsakeRequest struct {
	LetterID string `json:"-"`
}

type DeleteKeepsakeResponse struct{}

type HealthResponse struct {
	Status string `json:"status"`
}

type ReadinessResponse struct {
	Status string `json:"status"`
}

type ModerationRequest struct {
	RequestID string            `json:"request_id"`
	Purpose   ModerationPurpose `json:"purpose"`
}

type ClaimNextReportRequest ModerationRequest

type ReviewReportRequest ModerationRequest

type EvidenceEnvelope struct {
	Ciphertext        []byte `json:"ciphertext"`
	Nonce             []byte `json:"nonce"`
	WrappedKey        []byte `json:"wrapped_key"`
	KMSKeyID          string `json:"kms_key_id"`
	EncryptionVersion int16  `json:"encryption_version"`
}

type ModerationReport struct {
	ReportID  string            `json:"report_id"`
	LetterID  string            `json:"letter_id"`
	Target    ReportTarget      `json:"target"`
	Reason    ReportReason      `json:"reason"`
	Purpose   ModerationPurpose `json:"purpose"`
	CreatedAt time.Time         `json:"created_at"`
	Evidence  EvidenceEnvelope  `json:"evidence"`
}

type ClaimNextReportResponse ModerationReport

type ReviewReportResponse ModerationReport

type CloseReportRequest struct {
	RequestID   string                `json:"request_id"`
	Purpose     ModerationPurpose     `json:"purpose"`
	Disposition ModerationDisposition `json:"disposition"`
}

type CloseReportResponse struct {
	ReportID        string                `json:"report_id"`
	Disposition     ModerationDisposition `json:"disposition"`
	ClosedAt        time.Time             `json:"closed_at"`
	EvidencePurgeAt time.Time             `json:"evidence_purge_at"`
	RecordPurgeAt   time.Time             `json:"record_purge_at"`
}
