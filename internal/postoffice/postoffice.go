package postoffice

import (
	"context"
	"crypto/rand"
	"encoding/base64"
	"errors"
	"io"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/database"
	"github.com/nuggocto/orifude/internal/database/dbgen"
	"github.com/nuggocto/orifude/internal/envelope"
)

var (
	ErrInvalid           = errors.New("postoffice: invalid input")
	ErrAuthentication    = errors.New("postoffice: authentication failed")
	ErrSessionExpired    = errors.New("postoffice: session expired")
	ErrIdentityConflict  = errors.New("postoffice: identity conflict")
	ErrInviteInvalid     = errors.New("postoffice: invite invalid")
	ErrAliasInvalid      = errors.New("postoffice: alias invalid")
	ErrNotFound          = errors.New("postoffice: not found")
	ErrConflict          = errors.New("postoffice: conflict")
	ErrReplay            = errors.New("postoffice: DPoP replay")
	ErrRateLimited       = errors.New("postoffice: rate limited")
	ErrNoLetters         = errors.New("postoffice: no letter available")
	ErrClaimExpired      = errors.New("postoffice: claim expired")
	ErrAlreadyReplied    = errors.New("postoffice: letter already replied")
	ErrReportExists      = errors.New("postoffice: report already exists")
	ErrEvidenceExpired   = errors.New("postoffice: evidence expired")
	ErrReportNotReviewed = errors.New("postoffice: report not reviewed")
	ErrReportClosed      = errors.New("postoffice: report already closed")
	ErrNoReports         = errors.New("postoffice: no report available")
	errRetryRead         = errors.New("postoffice: retry body read")
)

const (
	rateSend   int16 = 1
	rateClaim  int16 = 2
	rateReport int16 = 3
)

type Config struct {
	Random           io.Reader
	Now              func() time.Time
	InviteRequired   bool
	LatestTUIVersion string
	SendPerHour      int32
	ClaimCooldown    time.Duration
	ClaimPerHour     int32
	ClaimPerDay      int32
	ReportPerDay     int32
	RateRetention    time.Duration
}

func DefaultConfig() Config {
	return Config{
		Random:         rand.Reader,
		Now:            time.Now,
		InviteRequired: true,
		SendPerHour:    10,
		ClaimCooldown:  15 * time.Minute,
		ClaimPerHour:   3,
		ClaimPerDay:    8,
		ReportPerDay:   20,
		RateRetention:  24 * time.Hour,
	}
}

type Service struct {
	db       *database.DB
	verifier *auth.Verifier
	cipher   *envelope.Cipher
	config   Config
	randomMu sync.Mutex
}

type Principal struct {
	IdentityID int64
}

func New(db *database.DB, verifier *auth.Verifier, cipher *envelope.Cipher, config Config) (*Service, error) {
	if db == nil || verifier == nil || cipher == nil || config.Random == nil || config.Now == nil ||
		config.SendPerHour < 0 || config.ClaimPerHour < 0 || config.ClaimPerDay < 0 || config.ReportPerDay < 0 ||
		!validSeconds(config.ClaimCooldown) || !validSeconds(config.RateRetention) || config.RateRetention <= 0 {
		return nil, ErrInvalid
	}
	return &Service{db: db, verifier: verifier, cipher: cipher, config: config}, nil
}

func validSeconds(duration time.Duration) bool {
	return duration >= 0 && duration%time.Second == 0 && duration/time.Second <= 1<<31-1
}

func (s *Service) randomCall(call func(io.Reader) error) error {
	s.randomMu.Lock()
	defer s.randomMu.Unlock()
	return call(s.config.Random)
}

func validID(value string) bool {
	if len(value) != envelope.IDLength {
		return false
	}
	decoded, err := base64.RawURLEncoding.Strict().DecodeString(value)
	return err == nil && len(decoded) == 16
}

func decodeSecret(value string) ([]byte, bool) {
	decoded, err := base64.RawURLEncoding.Strict().DecodeString(value)
	return decoded, err == nil && len(decoded) == 32 && base64.RawURLEncoding.EncodeToString(decoded) == value
}

func text(value string) pgtype.Text {
	return pgtype.Text{String: value, Valid: true}
}

func int8(value int64) pgtype.Int8 {
	return pgtype.Int8{Int64: value, Valid: true}
}

func int2(value int16) pgtype.Int2 {
	return pgtype.Int2{Int16: value, Valid: true}
}

func lockActiveIdentityPair(ctx context.Context, q *dbgen.Queries, first, second int64) error {
	if second < first {
		first, second = second, first
	}
	if _, err := q.LockActiveIdentity(ctx, first); err != nil {
		return err
	}
	_, err := q.LockActiveIdentity(ctx, second)
	return err
}

func encrypted(record dbgen.Letter, reply bool) envelope.Envelope {
	if reply {
		return envelope.Envelope{
			Ciphertext: record.ReplyCiphertext,
			Nonce:      record.ReplyNonce,
			WrappedKey: record.ReplyWrappedKey,
			KMSKeyARN:  record.ReplyKmsKeyID.String,
			Version:    record.ReplyEncryptionVersion.Int16,
		}
	}
	return envelope.Envelope{
		Ciphertext: record.BodyCiphertext,
		Nonce:      record.BodyNonce,
		WrappedKey: record.BodyWrappedKey,
		KMSKeyARN:  record.BodyKmsKeyID,
		Version:    record.BodyEncryptionVersion,
	}
}

func isUniqueViolation(err error) bool {
	var pgErr *pgconn.PgError
	return errors.As(err, &pgErr) && pgErr.Code == "23505"
}
