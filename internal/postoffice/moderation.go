package postoffice

import (
	"context"
	"errors"
	"unicode"
	"unicode/utf8"

	"github.com/jackc/pgx/v5"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/database/dbgen"
)

const (
	auditActionReview int16 = 1
	auditActionClose  int16 = 2
	auditAuthorized   int16 = 1
	auditDenied       int16 = 2
)

func (s *Service) ClaimNextReport(ctx context.Context, moderatorSubject string, request api.ClaimNextReportRequest) (api.ClaimNextReportResponse, error) {
	report, err := s.reviewReport(ctx, moderatorSubject, api.ModerationRequest(request), "", true)
	return api.ClaimNextReportResponse(moderationReport(report)), err
}

func (s *Service) ReviewReport(ctx context.Context, moderatorSubject, reportID string, request api.ReviewReportRequest) (api.ReviewReportResponse, error) {
	report, err := s.reviewReport(ctx, moderatorSubject, api.ModerationRequest(request), reportID, false)
	return api.ReviewReportResponse(moderationReport(report)), err
}

func (s *Service) reviewReport(ctx context.Context, subject string, request api.ModerationRequest, reportID string, next bool) (dbgen.Report, error) {
	if !validModerator(subject) || !validID(request.RequestID) || request.Purpose != api.ModerationPurposeReportedContentReview || (!next && !validID(reportID)) {
		return dbgen.Report{}, ErrInvalid
	}
	for attempt := 0; attempt < 2; attempt++ {
		var report dbgen.Report
		var operationErr error
		err := s.db.InTx(ctx, func(q *dbgen.Queries) error {
			audit, err := q.GetModerationAuditByRequestID(ctx, request.RequestID)
			if err == nil {
				if audit.Action != auditActionReview || audit.ModeratorSubject != subject || audit.Outcome != auditAuthorized || (!next && audit.ReportID != reportID) {
					operationErr = ErrConflict
					return nil
				}
				report, operationErr = reviewOwnedReport(ctx, q, audit.ReportID, subject)
				return nil
			}
			if !errors.Is(err, pgx.ErrNoRows) {
				return err
			}

			if next {
				report, err = q.LockNextUnreviewedReport(ctx)
				if errors.Is(err, pgx.ErrNoRows) {
					operationErr = ErrNoReports
					return nil
				}
			} else {
				report, err = q.LockReportForReview(ctx, reportID)
				if errors.Is(err, pgx.ErrNoRows) {
					return auditDenial(ctx, q, request.RequestID, reportID, subject, ErrNotFound, &operationErr)
				}
			}
			if err != nil {
				return err
			}
			if report.ReviewedBy.Valid && report.ReviewedBy.String != subject {
				return auditDenial(ctx, q, request.RequestID, report.ID, subject, ErrConflict, &operationErr)
			}
			reportID := report.ID
			report, err = q.MarkReportReviewed(ctx, dbgen.MarkReportReviewedParams{ModeratorSubject: text(subject), ID: reportID})
			if errors.Is(err, pgx.ErrNoRows) {
				return auditDenial(ctx, q, request.RequestID, reportID, subject, ErrEvidenceExpired, &operationErr)
			}
			if err != nil {
				return err
			}
			_, err = q.CreateModerationAudit(ctx, dbgen.CreateModerationAuditParams{
				RequestID: request.RequestID, ReportID: report.ID, ModeratorSubject: subject,
				Action: auditActionReview, Outcome: auditAuthorized,
			})
			return err
		})
		if isUniqueViolation(err) {
			continue
		}
		if err != nil {
			return dbgen.Report{}, err
		}
		if operationErr != nil {
			return dbgen.Report{}, operationErr
		}
		return report, nil
	}
	return dbgen.Report{}, ErrConflict
}

func reviewOwnedReport(ctx context.Context, q *dbgen.Queries, reportID, subject string) (dbgen.Report, error) {
	report, err := q.LockReportForReview(ctx, reportID)
	if errors.Is(err, pgx.ErrNoRows) {
		return dbgen.Report{}, ErrNotFound
	}
	if err != nil {
		return dbgen.Report{}, err
	}
	if !report.ReviewedBy.Valid || report.ReviewedBy.String != subject {
		return dbgen.Report{}, ErrConflict
	}
	report, err = q.MarkReportReviewed(ctx, dbgen.MarkReportReviewedParams{ModeratorSubject: text(subject), ID: report.ID})
	if errors.Is(err, pgx.ErrNoRows) {
		return dbgen.Report{}, ErrEvidenceExpired
	}
	return report, err
}

func auditDenial(ctx context.Context, q *dbgen.Queries, requestID, reportID, subject string, denied error, operationErr *error) error {
	_, err := q.CreateModerationAudit(ctx, dbgen.CreateModerationAuditParams{
		RequestID: requestID, ReportID: reportID, ModeratorSubject: subject,
		Action: auditActionReview, Outcome: auditDenied,
	})
	if err == nil {
		*operationErr = denied
	}
	return err
}

func (s *Service) CloseReport(ctx context.Context, moderatorSubject, reportID string, request api.CloseReportRequest) (api.CloseReportResponse, error) {
	disposition, ok := moderationDisposition(request.Disposition)
	if !ok || !validModerator(moderatorSubject) || !validID(reportID) || !validID(request.RequestID) || request.Purpose != api.ModerationPurposeReportedContentReview {
		return api.CloseReportResponse{}, ErrInvalid
	}
	for attempt := 0; attempt < 2; attempt++ {
		var report dbgen.Report
		var operationErr error
		err := s.db.InTx(ctx, func(q *dbgen.Queries) error {
			audit, err := q.GetModerationAuditByRequestID(ctx, request.RequestID)
			if err == nil {
				if audit.Action != auditActionClose || audit.ReportID != reportID || audit.ModeratorSubject != moderatorSubject || audit.Outcome != auditAuthorized {
					operationErr = ErrConflict
					return nil
				}
			} else if !errors.Is(err, pgx.ErrNoRows) {
				return err
			}

			report, err = q.LockReportForClose(ctx, reportID)
			if errors.Is(err, pgx.ErrNoRows) {
				if audit.ID != 0 {
					operationErr = ErrNotFound
					return nil
				}
				return closeAuditDenial(ctx, q, request.RequestID, reportID, moderatorSubject, ErrNotFound, &operationErr)
			}
			if err != nil {
				return err
			}
			if !report.ReviewedAt.Valid {
				if audit.ID != 0 {
					operationErr = ErrReportNotReviewed
					return nil
				}
				return closeAuditDenial(ctx, q, request.RequestID, reportID, moderatorSubject, ErrReportNotReviewed, &operationErr)
			}
			if report.Disposition.Valid && report.Disposition.Int16 != disposition {
				if audit.ID != 0 {
					operationErr = ErrReportClosed
					return nil
				}
				return closeAuditDenial(ctx, q, request.RequestID, reportID, moderatorSubject, ErrReportClosed, &operationErr)
			}
			report, err = q.CloseReport(ctx, dbgen.CloseReportParams{Disposition: int2(disposition), ID: reportID})
			if err != nil {
				return err
			}
			if disposition == 3 {
				identity, err := q.LockIdentity(ctx, report.ReportedIdentityID)
				if err != nil {
					return err
				}
				if err := deleteIdentity(ctx, q, identity); err != nil {
					return err
				}
			}
			if audit.ID == 0 {
				_, err = q.CreateModerationAudit(ctx, dbgen.CreateModerationAuditParams{
					RequestID: request.RequestID, ReportID: reportID, ModeratorSubject: moderatorSubject,
					Action: auditActionClose, Outcome: auditAuthorized,
				})
			}
			return err
		})
		if isUniqueViolation(err) {
			continue
		}
		if err != nil {
			return api.CloseReportResponse{}, err
		}
		if operationErr != nil {
			return api.CloseReportResponse{}, operationErr
		}
		return closeReportResponse(report), nil
	}
	return api.CloseReportResponse{}, ErrConflict
}

func closeAuditDenial(ctx context.Context, q *dbgen.Queries, requestID, reportID, subject string, denied error, operationErr *error) error {
	_, err := q.CreateModerationAudit(ctx, dbgen.CreateModerationAuditParams{
		RequestID: requestID, ReportID: reportID, ModeratorSubject: subject,
		Action: auditActionClose, Outcome: auditDenied,
	})
	if err == nil {
		*operationErr = denied
	}
	return err
}

func moderationReport(report dbgen.Report) api.ModerationReport {
	if report.ID == "" {
		return api.ModerationReport{}
	}
	return api.ModerationReport{
		ReportID: report.ID, LetterID: report.LetterID, Target: apiReportTarget(report.Target),
		Reason: apiReportReason(report.Reason), Purpose: api.ModerationPurposeReportedContentReview,
		CreatedAt: report.CreatedAt.Time,
		Evidence: api.EvidenceEnvelope{
			Ciphertext: report.EvidenceCiphertext, Nonce: report.EvidenceNonce,
			WrappedKey: report.EvidenceWrappedKey, KMSKeyID: report.EvidenceKmsKeyID.String,
			EncryptionVersion: report.EvidenceEncryptionVersion.Int16,
		},
	}
}

func closeReportResponse(report dbgen.Report) api.CloseReportResponse {
	return api.CloseReportResponse{
		ReportID: report.ID, Disposition: apiDisposition(report.Disposition.Int16),
		ClosedAt: report.ClosedAt.Time, EvidencePurgeAt: report.EvidencePurgeAt.Time,
		RecordPurgeAt: report.RecordPurgeAt.Time,
	}
}

func validModerator(subject string) bool {
	if subject == "" || len(subject) > 512 || !utf8.ValidString(subject) {
		return false
	}
	for _, r := range subject {
		if unicode.IsControl(r) || unicode.In(r, unicode.Cf) {
			return false
		}
	}
	return true
}

func moderationDisposition(disposition api.ModerationDisposition) (int16, bool) {
	switch disposition {
	case api.ModerationDispositionNoAction:
		return 1, true
	case api.ModerationDispositionDuplicate:
		return 2, true
	case api.ModerationDispositionIdentityDisabled:
		return 3, true
	default:
		return 0, false
	}
}

func apiDisposition(disposition int16) api.ModerationDisposition {
	switch disposition {
	case 1:
		return api.ModerationDispositionNoAction
	case 2:
		return api.ModerationDispositionDuplicate
	case 3:
		return api.ModerationDispositionIdentityDisabled
	default:
		return ""
	}
}

func apiReportTarget(target int16) api.ReportTarget {
	if target == 1 {
		return api.ReportTargetOriginal
	}
	return api.ReportTargetReply
}

func apiReportReason(reason int16) api.ReportReason {
	reasons := [...]api.ReportReason{
		api.ReportReasonHarassment,
		api.ReportReasonHatefulContent,
		api.ReportReasonSexualContent,
		api.ReportReasonThreats,
		api.ReportReasonSpamOrScams,
		api.ReportReasonExposedPersonalInformation,
		api.ReportReasonOtherUnsafeContent,
	}
	if reason < 1 || int(reason) > len(reasons) {
		return ""
	}
	return reasons[reason-1]
}
