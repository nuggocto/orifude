package postoffice

import (
	"context"
	"errors"

	"github.com/jackc/pgx/v5"
	"github.com/nuggocto/orifude/internal/database/dbgen"
)

type CleanupResult struct {
	Challenges     int
	Sessions       int
	Replays        int
	Claims         int
	WaitingLetters int
	Withdrawn      int
	Identities     int
	Evidence       int
	Reports        int
	Audits         int
	RateEvents     int
}

func (s *Service) Cleanup(ctx context.Context, batchSize int32) (CleanupResult, error) {
	if batchSize < 1 || batchSize > 10_000 {
		return CleanupResult{}, ErrInvalid
	}
	q := s.db.Queries()
	var result CleanupResult
	challenges, err := q.DeleteExpiredAuthChallengesBatch(ctx, batchSize)
	if err != nil {
		return result, err
	}
	result.Challenges = len(challenges)
	replays, err := q.DeleteExpiredDPoPReplaysBatch(ctx, batchSize)
	if err != nil {
		return result, err
	}
	result.Replays = len(replays)
	sessions, err := q.DeleteExpiredAccessSessionsBatch(ctx, batchSize)
	if err != nil {
		return result, err
	}
	result.Sessions = len(sessions)
	claims, err := q.ReleaseExpiredClaims(ctx, batchSize)
	if err != nil {
		return result, err
	}
	result.Claims = len(claims)
	waiting, err := q.DeleteExpiredWaitingLetters(ctx, batchSize)
	if err != nil {
		return result, err
	}
	result.WaitingLetters = len(waiting)
	withdrawn, err := q.DeleteExpiredWithdrawnLetters(ctx, batchSize)
	if err != nil {
		return result, err
	}
	result.Withdrawn = len(withdrawn)

	for result.Identities < int(batchSize) {
		err := s.db.InTx(ctx, func(q *dbgen.Queries) error {
			identity, err := q.LockNextInactiveIdentity(ctx)
			if err != nil {
				return err
			}
			return deleteIdentity(ctx, q, identity)
		})
		if errors.Is(err, pgx.ErrNoRows) {
			break
		}
		if err != nil {
			return result, err
		}
		result.Identities++
	}
	for result.Evidence < int(batchSize) {
		err := s.db.InTx(ctx, func(q *dbgen.Queries) error {
			report, err := q.LockNextReportForEvidencePurge(ctx)
			if err != nil {
				return err
			}
			_, err = q.PurgeReportEvidence(ctx, report.ID)
			return err
		})
		if errors.Is(err, pgx.ErrNoRows) {
			break
		}
		if err != nil {
			return result, err
		}
		result.Evidence++
	}
	reports, err := q.DeleteExpiredReports(ctx, batchSize)
	if err != nil {
		return result, err
	}
	result.Reports = len(reports)
	audits, err := q.DeleteExpiredModerationAudit(ctx, batchSize)
	if err != nil {
		return result, err
	}
	result.Audits = len(audits)
	rateEvents, err := q.DeleteOldRateLimitEventsBatch(ctx, dbgen.DeleteOldRateLimitEventsBatchParams{
		RetentionSeconds: int32(s.config.RateRetention.Seconds()), BatchSize: batchSize,
	})
	if err != nil {
		return result, err
	}
	result.RateEvents = len(rateEvents)
	return result, nil
}
