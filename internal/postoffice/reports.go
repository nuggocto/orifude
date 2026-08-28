package postoffice

import (
	"context"
	"errors"

	"github.com/jackc/pgx/v5"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/database/dbgen"
	"github.com/nuggocto/orifude/internal/envelope"
)

func (s *Service) ReportLetter(ctx context.Context, principal Principal, letterID string, request api.ReportLetterRequest) (api.ReportLetterResponse, error) {
	target, targetValue, ok := reportTarget(request.Target)
	reason, reasonOK := reportReason(request.Reason)
	if !validID(letterID) || !validID(request.ReportID) || !ok || !reasonOK {
		return api.ReportLetterResponse{}, ErrInvalid
	}
	if existing, err := s.db.Queries().GetReportByIDForReporter(ctx, dbgen.GetReportByIDForReporterParams{ID: request.ReportID, ReporterID: principal.IdentityID}); err == nil {
		if existing.LetterID != letterID {
			return api.ReportLetterResponse{}, ErrConflict
		}
		return reportResponse(existing), nil
	} else if !errors.Is(err, pgx.ErrNoRows) {
		return api.ReportLetterResponse{}, err
	}
	if _, err := s.db.Queries().GetReportByLetterForReporter(ctx, dbgen.GetReportByLetterForReporterParams{LetterID: letterID, ReporterID: principal.IdentityID}); err == nil {
		return api.ReportLetterResponse{}, ErrReportExists
	} else if !errors.Is(err, pgx.ErrNoRows) {
		return api.ReportLetterResponse{}, err
	}

	letter, role, err := s.reportableLetter(ctx, principal.IdentityID, letterID, request.Target)
	if err != nil {
		if !errors.Is(err, ErrNotFound) {
			return api.ReportLetterResponse{}, err
		}
		if existing, lookupErr := s.db.Queries().GetReportByIDForReporter(ctx, dbgen.GetReportByIDForReporterParams{ID: request.ReportID, ReporterID: principal.IdentityID}); lookupErr == nil {
			if existing.LetterID != letterID {
				return api.ReportLetterResponse{}, ErrConflict
			}
			return reportResponse(existing), nil
		} else if !errors.Is(lookupErr, pgx.ErrNoRows) {
			return api.ReportLetterResponse{}, lookupErr
		}
		if _, lookupErr := s.db.Queries().GetReportByLetterForReporter(ctx, dbgen.GetReportByLetterForReporterParams{LetterID: letterID, ReporterID: principal.IdentityID}); lookupErr == nil {
			return api.ReportLetterResponse{}, ErrReportExists
		} else if !errors.Is(lookupErr, pgx.ErrNoRows) {
			return api.ReportLetterResponse{}, lookupErr
		}
		return api.ReportLetterResponse{}, err
	}
	otherID := letter.SenderID
	if role == api.LetterRoleSender {
		otherID = letter.RecipientID.Int64
	}
	if err := s.checkRate(ctx, s.db.Queries(), principal.IdentityID, rateReport, 0, 0, s.config.ReportPerDay); err != nil {
		return api.ReportLetterResponse{}, err
	}
	var plaintext []byte
	if request.Target == api.ReportTargetOriginal {
		plaintext, err = s.cipher.DecryptOriginal(ctx, letterID, encrypted(letter, false))
	} else {
		plaintext, err = s.cipher.DecryptReply(ctx, letterID, letter.ReplyID.String, encrypted(letter, true))
	}
	if err != nil {
		return api.ReportLetterResponse{}, err
	}
	evidence, err := s.cipher.EncryptEvidence(ctx, request.ReportID, letterID, target, plaintext)
	clear(plaintext)
	if err != nil {
		return api.ReportLetterResponse{}, err
	}

	var report dbgen.Report
	err = s.db.InTx(ctx, func(q *dbgen.Queries) error {
		if err := lockActiveIdentityPair(ctx, q, principal.IdentityID, otherID); err != nil {
			if !errors.Is(err, pgx.ErrNoRows) {
				return err
			}
			report, err = q.GetReportByIDForReporter(ctx, dbgen.GetReportByIDForReporterParams{ID: request.ReportID, ReporterID: principal.IdentityID})
			if err == nil {
				if report.LetterID != letterID {
					return ErrConflict
				}
				return nil
			}
			if !errors.Is(err, pgx.ErrNoRows) {
				return err
			}
			return ErrNotFound
		}
		report, err = q.GetReportByIDForReporter(ctx, dbgen.GetReportByIDForReporterParams{ID: request.ReportID, ReporterID: principal.IdentityID})
		if err == nil {
			if report.LetterID != letterID {
				return ErrConflict
			}
			return nil
		}
		if !errors.Is(err, pgx.ErrNoRows) {
			return err
		}
		if _, err := q.GetReportByLetterForReporter(ctx, dbgen.GetReportByLetterForReporterParams{LetterID: letterID, ReporterID: principal.IdentityID}); err == nil {
			return ErrReportExists
		} else if !errors.Is(err, pgx.ErrNoRows) {
			return err
		}
		locked, err := lockLetterForRole(ctx, q, principal.IdentityID, letterID, role)
		if err != nil || !roleHasAccess(locked, principal.IdentityID, role) || !sameOriginalEnvelope(letter, locked) {
			return ErrNotFound
		}
		if request.Target == api.ReportTargetReply && (!locked.ReplyID.Valid || locked.ReplyID.String != letter.ReplyID.String || !sameReadableVersion(letter, locked)) {
			return ErrNotFound
		}
		if err := s.checkRate(ctx, q, principal.IdentityID, rateReport, 0, 0, s.config.ReportPerDay); err != nil {
			return err
		}
		report, err = q.CreateReport(ctx, dbgen.CreateReportParams{
			ID: request.ReportID, ReporterID: principal.IdentityID, Target: targetValue, Reason: reason,
			EvidenceCiphertext: evidence.Ciphertext, EvidenceNonce: evidence.Nonce,
			EvidenceWrappedKey: evidence.WrappedKey, EvidenceKmsKeyID: text(evidence.KMSKeyARN),
			EvidenceEncryptionVersion: int2(evidence.Version), LetterID: letterID,
		})
		if isUniqueViolation(err) {
			return ErrReportExists
		}
		if err != nil {
			return err
		}
		if _, err := q.CreateBlockFromLetter(ctx, dbgen.CreateBlockFromLetterParams{IdentityID: principal.IdentityID, LetterID: letterID}); err != nil {
			return err
		}
		rows, err := q.HideReportedLetter(ctx, dbgen.HideReportedLetterParams{ReportID: report.ID, ReporterID: principal.IdentityID})
		if err != nil {
			return err
		}
		if rows != 1 {
			return ErrNotFound
		}
		if _, err := q.DeleteFullyRemovedLetter(ctx, letterID); err != nil {
			return err
		}
		_, err = q.RecordRateLimitEvent(ctx, dbgen.RecordRateLimitEventParams{IdentityID: principal.IdentityID, Kind: rateReport})
		return err
	})
	if err != nil {
		return api.ReportLetterResponse{}, err
	}
	return reportResponse(report), nil
}

func (s *Service) reportableLetter(ctx context.Context, identityID int64, letterID string, target api.ReportTarget) (dbgen.Letter, api.LetterRole, error) {
	if target == api.ReportTargetOriginal {
		letter, err := s.db.Queries().GetLetterForRecipient(ctx, dbgen.GetLetterForRecipientParams{RecipientID: identityID, ID: letterID})
		if errors.Is(err, pgx.ErrNoRows) {
			return dbgen.Letter{}, "", ErrNotFound
		}
		return letter, api.LetterRoleRecipient, err
	}
	letter, err := s.db.Queries().GetLetterForSender(ctx, dbgen.GetLetterForSenderParams{SenderID: identityID, ID: letterID})
	if errors.Is(err, pgx.ErrNoRows) || err == nil && !letter.ReplyID.Valid {
		return dbgen.Letter{}, "", ErrNotFound
	}
	return letter, api.LetterRoleSender, err
}

func reportTarget(target api.ReportTarget) (envelope.Target, int16, bool) {
	switch target {
	case api.ReportTargetOriginal:
		return envelope.TargetOriginal, 1, true
	case api.ReportTargetReply:
		return envelope.TargetReply, 2, true
	default:
		return "", 0, false
	}
}

func reportReason(reason api.ReportReason) (int16, bool) {
	switch reason {
	case api.ReportReasonHarassment:
		return 1, true
	case api.ReportReasonHatefulContent:
		return 2, true
	case api.ReportReasonSexualContent:
		return 3, true
	case api.ReportReasonThreats:
		return 4, true
	case api.ReportReasonSpamOrScams:
		return 5, true
	case api.ReportReasonExposedPersonalInformation:
		return 6, true
	case api.ReportReasonOtherUnsafeContent:
		return 7, true
	default:
		return 0, false
	}
}

func reportResponse(report dbgen.Report) api.ReportLetterResponse {
	return api.ReportLetterResponse{ReportID: report.ID, CreatedAt: report.CreatedAt.Time}
}
