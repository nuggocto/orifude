package postoffice

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/binary"
	"errors"
	"io"
	"sort"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/database/dbgen"
	"github.com/nuggocto/orifude/internal/envelope"
	"github.com/nuggocto/orifude/internal/textpolicy"
)

func (s *Service) SendLetter(ctx context.Context, principal Principal, request api.CreateLetterRequest) (api.CreateLetterResponse, error) {
	if !validID(request.LetterID) {
		return api.CreateLetterResponse{}, ErrInvalid
	}
	if err := textpolicy.ValidateBody(request.Body); err != nil {
		return api.CreateLetterResponse{}, errors.Join(ErrInvalid, err)
	}
	if existing, err := s.db.Queries().GetLetterForSender(ctx, dbgen.GetLetterForSenderParams{SenderID: principal.IdentityID, ID: request.LetterID}); err == nil {
		return createLetterResponse(existing), nil
	} else if !errors.Is(err, pgx.ErrNoRows) {
		return api.CreateLetterResponse{}, err
	}
	identity, err := s.db.Queries().GetIdentityByID(ctx, principal.IdentityID)
	if errors.Is(err, pgx.ErrNoRows) || err == nil && identity.DeletedAt.Valid {
		return api.CreateLetterResponse{}, ErrAuthentication
	}
	if err != nil {
		return api.CreateLetterResponse{}, err
	}
	if err := s.checkRate(ctx, s.db.Queries(), identity.ID, rateSend, 0, s.config.SendPerHour, 0); err != nil {
		return api.CreateLetterResponse{}, err
	}

	record, err := s.cipher.EncryptOriginal(ctx, request.LetterID, []byte(request.Body))
	if err != nil {
		return api.CreateLetterResponse{}, err
	}
	foldSeed, err := s.foldSeed()
	if err != nil {
		return api.CreateLetterResponse{}, err
	}

	var letter dbgen.Letter
	err = s.db.InTx(ctx, func(q *dbgen.Queries) error {
		identity, err := q.LockActiveIdentity(ctx, principal.IdentityID)
		if errors.Is(err, pgx.ErrNoRows) {
			return ErrAuthentication
		}
		if err != nil {
			return err
		}
		letter, err = q.GetLetterForSender(ctx, dbgen.GetLetterForSenderParams{SenderID: identity.ID, ID: request.LetterID})
		if err == nil {
			return nil
		}
		if !errors.Is(err, pgx.ErrNoRows) {
			return err
		}
		if err := s.checkRate(ctx, q, identity.ID, rateSend, 0, s.config.SendPerHour, 0); err != nil {
			return err
		}
		letter, err = q.CreateLetter(ctx, dbgen.CreateLetterParams{
			ID: request.LetterID, SenderID: identity.ID, SenderAlias: identity.Alias.String,
			BodyCiphertext: record.Ciphertext, BodyNonce: record.Nonce, BodyWrappedKey: record.WrappedKey,
			BodyKmsKeyID: record.KMSKeyARN, BodyEncryptionVersion: record.Version, FoldSeed: foldSeed,
		})
		if isUniqueViolation(err) {
			return ErrConflict
		}
		if err != nil {
			return err
		}
		_, err = q.RecordRateLimitEvent(ctx, dbgen.RecordRateLimitEventParams{IdentityID: identity.ID, Kind: rateSend})
		return err
	})
	if err != nil {
		return api.CreateLetterResponse{}, err
	}
	return createLetterResponse(letter), nil
}

func (s *Service) ClaimLetter(ctx context.Context, principal Principal) (api.ClaimLetterResponse, error) {
	var letter dbgen.Letter
	err := s.db.InTx(ctx, func(q *dbgen.Queries) error {
		identity, err := q.LockActiveIdentity(ctx, principal.IdentityID)
		if errors.Is(err, pgx.ErrNoRows) {
			return ErrAuthentication
		}
		if err != nil {
			return err
		}
		letter, err = q.GetActiveClaimForUpdate(ctx, int8(identity.ID))
		if err == nil {
			return nil
		}
		if !errors.Is(err, pgx.ErrNoRows) {
			return err
		}
		if _, err := q.ReleaseExpiredClaimsForIdentity(ctx, int8(identity.ID)); err != nil {
			return err
		}
		if err := s.checkRate(ctx, q, identity.ID, rateClaim, s.config.ClaimCooldown, s.config.ClaimPerHour, s.config.ClaimPerDay); err != nil {
			return err
		}
		candidate, err := q.SelectEligibleLetterForClaim(ctx, identity.ID)
		if errors.Is(err, pgx.ErrNoRows) {
			return ErrNoLetters
		}
		if err != nil {
			return err
		}
		letter, err = q.AssignLetterClaim(ctx, dbgen.AssignLetterClaimParams{
			RecipientID: int8(identity.ID), RecipientAlias: identity.Alias, ID: candidate.ID,
		})
		if err != nil {
			return err
		}
		_, err = q.RecordRateLimitEvent(ctx, dbgen.RecordRateLimitEventParams{IdentityID: identity.ID, Kind: rateClaim})
		return err
	})
	if err != nil {
		return api.ClaimLetterResponse{}, err
	}
	return api.ClaimLetterResponse{
		LetterID: letter.ID, FoldSeed: letter.FoldSeed,
		CreatedAt: letter.CreatedAt.Time, ClaimExpiresAt: letter.ClaimExpiresAt.Time,
	}, nil
}

func (s *Service) GetLetter(ctx context.Context, principal Principal, letterID string) (api.GetLetterResponse, error) {
	if !validID(letterID) {
		return api.GetLetterResponse{}, ErrNotFound
	}
	for range 2 {
		letter, role, err := s.readableLetter(ctx, principal.IdentityID, letterID)
		if err != nil {
			return api.GetLetterResponse{}, err
		}
		original, err := s.cipher.DecryptOriginal(ctx, letter.ID, encrypted(letter, false))
		if err != nil {
			return api.GetLetterResponse{}, err
		}
		var reply []byte
		if letter.ReplyID.Valid {
			reply, err = s.cipher.DecryptReply(ctx, letter.ID, letter.ReplyID.String, encrypted(letter, true))
			if err != nil {
				clear(original)
				return api.GetLetterResponse{}, err
			}
		}
		var locked dbgen.Letter
		err = s.db.InTx(ctx, func(q *dbgen.Queries) error {
			if _, err := q.LockActiveIdentity(ctx, principal.IdentityID); err != nil {
				return ErrNotFound
			}
			locked, err = lockLetterForRole(ctx, q, principal.IdentityID, letterID, role)
			if err != nil || !roleHasAccess(locked, principal.IdentityID, role) {
				return ErrNotFound
			}
			if !sameReadableVersion(letter, locked) {
				return errRetryRead
			}
			return nil
		})
		if errors.Is(err, errRetryRead) {
			clear(original)
			clear(reply)
			continue
		}
		if err != nil {
			clear(original)
			clear(reply)
			return api.GetLetterResponse{}, err
		}
		response := letterResponse(locked, role, string(original), string(reply))
		clear(original)
		clear(reply)
		return response, nil
	}
	return api.GetLetterResponse{}, ErrConflict
}

func (s *Service) OpenLetter(ctx context.Context, principal Principal, letterID string) (api.OpenLetterResponse, error) {
	if !validID(letterID) {
		return api.OpenLetterResponse{}, ErrNotFound
	}
	letter, err := s.db.Queries().GetLetterForOpen(ctx, dbgen.GetLetterForOpenParams{RecipientID: principal.IdentityID, ID: letterID})
	if errors.Is(err, pgx.ErrNoRows) {
		expired, lookupErr := s.db.Queries().ExpiredClaimExistsForRecipient(ctx, dbgen.ExpiredClaimExistsForRecipientParams{RecipientID: principal.IdentityID, ID: letterID})
		if lookupErr != nil {
			return api.OpenLetterResponse{}, lookupErr
		}
		if expired {
			return api.OpenLetterResponse{}, ErrClaimExpired
		}
		return api.OpenLetterResponse{}, ErrNotFound
	}
	if err != nil {
		return api.OpenLetterResponse{}, err
	}
	plaintext, err := s.cipher.DecryptOriginal(ctx, letter.ID, encrypted(letter, false))
	if err != nil {
		return api.OpenLetterResponse{}, err
	}
	defer clear(plaintext)

	var opened dbgen.Letter
	err = s.db.InTx(ctx, func(q *dbgen.Queries) error {
		if _, err := q.LockActiveIdentity(ctx, principal.IdentityID); err != nil {
			return ErrNotFound
		}
		locked, err := q.LockLetterForRecipient(ctx, dbgen.LockLetterForRecipientParams{ID: letterID, RecipientID: int8(principal.IdentityID)})
		if err != nil || locked.RecipientRemovedAt.Valid || !sameOriginalEnvelope(letter, locked) {
			return ErrNotFound
		}
		if locked.OpenedAt.Valid {
			opened = locked
			return nil
		}
		opened, err = q.OpenLetter(ctx, dbgen.OpenLetterParams{ID: letterID, RecipientID: int8(principal.IdentityID)})
		if errors.Is(err, pgx.ErrNoRows) {
			return ErrClaimExpired
		}
		return err
	})
	if err != nil {
		return api.OpenLetterResponse{}, err
	}
	return api.OpenLetterResponse{
		LetterID: opened.ID, OpenedAt: opened.OpenedAt.Time,
		Original: api.Message{Body: string(plaintext), Alias: opened.SenderAlias, CreatedAt: opened.CreatedAt.Time},
	}, nil
}

func (s *Service) ReplyToLetter(ctx context.Context, principal Principal, letterID string, request api.ReplyToLetterRequest) (api.ReplyToLetterResponse, error) {
	if !validID(letterID) || !validID(request.ReplyID) {
		return api.ReplyToLetterResponse{}, ErrInvalid
	}
	if existing, err := s.db.Queries().GetLetterByReplyIDForRecipient(ctx, dbgen.GetLetterByReplyIDForRecipientParams{
		RecipientID: int8(principal.IdentityID), ReplyID: text(request.ReplyID),
	}); err == nil {
		if existing.ID != letterID {
			return api.ReplyToLetterResponse{}, ErrConflict
		}
		return replyResponse(existing), nil
	} else if !errors.Is(err, pgx.ErrNoRows) {
		return api.ReplyToLetterResponse{}, err
	}
	letter, err := s.db.Queries().GetLetterForRecipient(ctx, dbgen.GetLetterForRecipientParams{ID: letterID, RecipientID: principal.IdentityID})
	if errors.Is(err, pgx.ErrNoRows) {
		return api.ReplyToLetterResponse{}, ErrNotFound
	}
	if err != nil {
		return api.ReplyToLetterResponse{}, err
	}
	if letter.ReplyID.Valid {
		return api.ReplyToLetterResponse{}, ErrAlreadyReplied
	}
	if err := textpolicy.ValidateBody(request.Body); err != nil {
		return api.ReplyToLetterResponse{}, errors.Join(ErrInvalid, err)
	}
	record, err := s.cipher.EncryptReply(ctx, letterID, request.ReplyID, []byte(request.Body))
	if err != nil {
		return api.ReplyToLetterResponse{}, err
	}

	letter = dbgen.Letter{}
	err = s.db.InTx(ctx, func(q *dbgen.Queries) error {
		if _, err := q.LockActiveIdentity(ctx, principal.IdentityID); err != nil {
			return ErrNotFound
		}
		letter, err = q.LockLetterForRecipient(ctx, dbgen.LockLetterForRecipientParams{ID: letterID, RecipientID: int8(principal.IdentityID)})
		if err != nil || !letter.OpenedAt.Valid || letter.RecipientRemovedAt.Valid {
			return ErrNotFound
		}
		if letter.ReplyID.Valid {
			if letter.ReplyID.String == request.ReplyID {
				return nil
			}
			return ErrAlreadyReplied
		}
		letter, err = q.AddLetterReply(ctx, dbgen.AddLetterReplyParams{
			ReplyID: text(request.ReplyID), ReplyCiphertext: record.Ciphertext, ReplyNonce: record.Nonce,
			ReplyWrappedKey: record.WrappedKey, ReplyKmsKeyID: text(record.KMSKeyARN),
			ReplyEncryptionVersion: int2(record.Version), ID: letterID, RecipientID: int8(principal.IdentityID),
		})
		if isUniqueViolation(err) {
			return ErrConflict
		}
		return err
	})
	if err != nil {
		return api.ReplyToLetterResponse{}, err
	}
	return replyResponse(letter), nil
}

func (s *Service) WithdrawLetter(ctx context.Context, principal Principal, letterID string) (api.WithdrawLetterResponse, error) {
	if !validID(letterID) {
		return api.WithdrawLetterResponse{}, ErrNotFound
	}
	var letter dbgen.Letter
	err := s.db.InTx(ctx, func(q *dbgen.Queries) error {
		if _, err := q.LockActiveIdentity(ctx, principal.IdentityID); err != nil {
			return ErrNotFound
		}
		var err error
		letter, err = q.LockLetterForSender(ctx, dbgen.LockLetterForSenderParams{ID: letterID, SenderID: principal.IdentityID})
		if err != nil || letter.SenderRemovedAt.Valid {
			return ErrNotFound
		}
		if letter.WithdrawnAt.Valid {
			return nil
		}
		if letter.RecipientID.Valid {
			return ErrConflict
		}
		letter, err = q.WithdrawLetter(ctx, dbgen.WithdrawLetterParams{ID: letterID, SenderID: principal.IdentityID})
		if errors.Is(err, pgx.ErrNoRows) {
			return ErrNotFound
		}
		return err
	})
	if err != nil {
		return api.WithdrawLetterResponse{}, err
	}
	return api.WithdrawLetterResponse{LetterID: letter.ID, WithdrawnAt: letter.WithdrawnAt.Time}, nil
}

func (s *Service) BlockLetter(ctx context.Context, principal Principal, letterID string) (api.BlockLetterResponse, error) {
	if !validID(letterID) {
		return api.BlockLetterResponse{}, ErrNotFound
	}
	letter, role, err := s.readableLetter(ctx, principal.IdentityID, letterID)
	if err != nil || !letter.OpenedAt.Valid || !roleHasAccess(letter, principal.IdentityID, role) {
		return api.BlockLetterResponse{}, ErrNotFound
	}
	otherID := letter.SenderID
	if role == api.LetterRoleSender {
		otherID = letter.RecipientID.Int64
	}
	var block dbgen.Block
	err = s.db.InTx(ctx, func(q *dbgen.Queries) error {
		if err := lockActiveIdentityPair(ctx, q, principal.IdentityID, otherID); err != nil {
			if !errors.Is(err, pgx.ErrNoRows) {
				return err
			}
			return ErrNotFound
		}
		letter, err := lockLetterForRole(ctx, q, principal.IdentityID, letterID, role)
		if err != nil || !letter.OpenedAt.Valid || !roleHasAccess(letter, principal.IdentityID, role) {
			return ErrNotFound
		}
		if _, err := q.CreateBlockFromLetter(ctx, dbgen.CreateBlockFromLetterParams{IdentityID: principal.IdentityID, LetterID: letterID}); err != nil {
			return err
		}
		block, err = q.GetBlock(ctx, dbgen.GetBlockParams{BlockerID: principal.IdentityID, BlockedID: otherID})
		return err
	})
	if err != nil {
		return api.BlockLetterResponse{}, err
	}
	return api.BlockLetterResponse{LetterID: letterID, BlockedAt: block.CreatedAt.Time}, nil
}

func (s *Service) ListKeepsakes(ctx context.Context, principal Principal, request api.ListKeepsakesRequest) (api.ListKeepsakesResponse, error) {
	limit := request.Limit
	if limit == 0 {
		limit = 20
	}
	if limit < 1 || limit > 100 {
		return api.ListKeepsakesResponse{}, ErrInvalid
	}
	cursor, err := decodeCursor(request.Cursor)
	if err != nil {
		return api.ListKeepsakesResponse{}, ErrInvalid
	}
	pageSize := int32(limit + 1)
	var sent, received []dbgen.Letter
	if cursor == nil {
		sent, err = s.db.Queries().ListSentKeepsakes(ctx, dbgen.ListSentKeepsakesParams{IdentityID: principal.IdentityID, PageSize: pageSize})
		if err == nil {
			received, err = s.db.Queries().ListReceivedKeepsakes(ctx, dbgen.ListReceivedKeepsakesParams{IdentityID: int8(principal.IdentityID), PageSize: pageSize})
		}
	} else {
		at := pgtype.Timestamptz{Time: cursor.createdAt, Valid: true}
		sent, err = s.db.Queries().ListSentKeepsakesAfter(ctx, dbgen.ListSentKeepsakesAfterParams{
			IdentityID: principal.IdentityID, CursorCreatedAt: at, CursorID: cursor.id, PageSize: pageSize,
		})
		if err == nil {
			received, err = s.db.Queries().ListReceivedKeepsakesAfter(ctx, dbgen.ListReceivedKeepsakesAfterParams{
				IdentityID: int8(principal.IdentityID), CursorCreatedAt: at, CursorID: cursor.id, PageSize: pageSize,
			})
		}
	}
	if err != nil {
		return api.ListKeepsakesResponse{}, err
	}

	type item struct {
		letter dbgen.Letter
		role   api.LetterRole
	}
	items := make([]item, 0, len(sent)+len(received))
	for _, letter := range sent {
		items = append(items, item{letter: letter, role: api.LetterRoleSender})
	}
	for _, letter := range received {
		items = append(items, item{letter: letter, role: api.LetterRoleRecipient})
	}
	sort.Slice(items, func(i, j int) bool {
		if items[i].letter.CreatedAt.Time.Equal(items[j].letter.CreatedAt.Time) {
			return items[i].letter.ID > items[j].letter.ID
		}
		return items[i].letter.CreatedAt.Time.After(items[j].letter.CreatedAt.Time)
	})
	more := len(items) > limit
	if more {
		items = items[:limit]
	}
	response := api.ListKeepsakesResponse{Keepsakes: make([]api.LetterSummary, 0, len(items))}
	for _, item := range items {
		response.Keepsakes = append(response.Keepsakes, letterSummary(item.letter, item.role))
	}
	if more {
		last := items[len(items)-1].letter
		response.NextCursor = encodeCursor(last.CreatedAt.Time, last.ID)
	}
	return response, nil
}

func (s *Service) DeleteKeepsake(ctx context.Context, principal Principal, letterID string) error {
	if !validID(letterID) {
		return ErrNotFound
	}
	return s.db.InTx(ctx, func(q *dbgen.Queries) error {
		if _, err := q.LockActiveIdentity(ctx, principal.IdentityID); err != nil {
			return ErrNotFound
		}
		letter, role, err := lockParticipantLetter(ctx, q, principal.IdentityID, letterID)
		if err != nil {
			return ErrNotFound
		}
		switch role {
		case api.LetterRoleSender:
			if !letter.SenderRemovedAt.Valid {
				if _, err := q.RemoveSentKeepsake(ctx, dbgen.RemoveSentKeepsakeParams{ID: letterID, IdentityID: principal.IdentityID}); err != nil {
					return err
				}
			}
		case api.LetterRoleRecipient:
			if !letter.OpenedAt.Valid {
				return ErrNotFound
			}
			if !letter.RecipientRemovedAt.Valid {
				if _, err := q.RemoveReceivedKeepsake(ctx, dbgen.RemoveReceivedKeepsakeParams{ID: letterID, IdentityID: int8(principal.IdentityID)}); err != nil {
					return err
				}
			}
		}
		_, err = q.DeleteFullyRemovedLetter(ctx, letterID)
		return err
	})
}

func (s *Service) checkRate(ctx context.Context, q *dbgen.Queries, identityID int64, kind int16, cooldown time.Duration, hour, day int32) error {
	allowed, err := q.RateLimitAllowed(ctx, dbgen.RateLimitAllowedParams{
		CooldownSeconds: int32(cooldown / time.Second), RateIdentityID: identityID,
		RateKind: kind, HourLimit: hour, DayLimit: day,
	})
	if err != nil {
		return err
	}
	if !allowed.Valid || !allowed.Bool {
		return ErrRateLimited
	}
	return nil
}

func (s *Service) foldSeed() (int64, error) {
	var seed int64
	err := s.randomCall(func(random io.Reader) error {
		var bytes [8]byte
		if _, err := io.ReadFull(random, bytes[:]); err != nil {
			return err
		}
		seed = int64(binary.BigEndian.Uint64(bytes[:]) & (1<<63 - 1))
		return nil
	})
	return seed, err
}

func (s *Service) readableLetter(ctx context.Context, identityID int64, letterID string) (dbgen.Letter, api.LetterRole, error) {
	letter, err := s.db.Queries().GetLetterForSender(ctx, dbgen.GetLetterForSenderParams{SenderID: identityID, ID: letterID})
	if err == nil {
		return letter, api.LetterRoleSender, nil
	}
	if !errors.Is(err, pgx.ErrNoRows) {
		return dbgen.Letter{}, "", err
	}
	letter, err = s.db.Queries().GetLetterForRecipient(ctx, dbgen.GetLetterForRecipientParams{RecipientID: identityID, ID: letterID})
	if errors.Is(err, pgx.ErrNoRows) {
		return dbgen.Letter{}, "", ErrNotFound
	}
	if err != nil {
		return dbgen.Letter{}, "", err
	}
	return letter, api.LetterRoleRecipient, nil
}

func lockLetterForRole(ctx context.Context, q *dbgen.Queries, identityID int64, letterID string, role api.LetterRole) (dbgen.Letter, error) {
	if role == api.LetterRoleSender {
		return q.LockLetterForSender(ctx, dbgen.LockLetterForSenderParams{ID: letterID, SenderID: identityID})
	}
	return q.LockLetterForRecipient(ctx, dbgen.LockLetterForRecipientParams{ID: letterID, RecipientID: int8(identityID)})
}

func lockParticipantLetter(ctx context.Context, q *dbgen.Queries, identityID int64, letterID string) (dbgen.Letter, api.LetterRole, error) {
	letter, err := q.LockLetterForSender(ctx, dbgen.LockLetterForSenderParams{ID: letterID, SenderID: identityID})
	if err == nil {
		return letter, api.LetterRoleSender, nil
	}
	if !errors.Is(err, pgx.ErrNoRows) {
		return dbgen.Letter{}, "", err
	}
	letter, err = q.LockLetterForRecipient(ctx, dbgen.LockLetterForRecipientParams{ID: letterID, RecipientID: int8(identityID)})
	if err != nil {
		return dbgen.Letter{}, "", err
	}
	return letter, api.LetterRoleRecipient, nil
}

func roleHasAccess(letter dbgen.Letter, identityID int64, role api.LetterRole) bool {
	if role == api.LetterRoleSender {
		return letter.SenderID == identityID && !letter.SenderRemovedAt.Valid
	}
	return letter.RecipientID.Valid && letter.RecipientID.Int64 == identityID && letter.OpenedAt.Valid && !letter.RecipientRemovedAt.Valid
}

func sameOriginalEnvelope(a, b dbgen.Letter) bool {
	return a.ID == b.ID && a.BodyEncryptionVersion == b.BodyEncryptionVersion && a.BodyKmsKeyID == b.BodyKmsKeyID &&
		bytes.Equal(a.BodyCiphertext, b.BodyCiphertext) && bytes.Equal(a.BodyNonce, b.BodyNonce) && bytes.Equal(a.BodyWrappedKey, b.BodyWrappedKey)
}

func sameReadableVersion(a, b dbgen.Letter) bool {
	return sameOriginalEnvelope(a, b) && a.ReplyID == b.ReplyID && a.ReplyKmsKeyID == b.ReplyKmsKeyID &&
		a.ReplyEncryptionVersion == b.ReplyEncryptionVersion && bytes.Equal(a.ReplyCiphertext, b.ReplyCiphertext) &&
		bytes.Equal(a.ReplyNonce, b.ReplyNonce) && bytes.Equal(a.ReplyWrappedKey, b.ReplyWrappedKey) &&
		a.OpenedAt == b.OpenedAt && a.WithdrawnAt == b.WithdrawnAt && a.SenderRemovedAt == b.SenderRemovedAt &&
		a.RecipientRemovedAt == b.RecipientRemovedAt && a.RecipientID == b.RecipientID
}

func createLetterResponse(letter dbgen.Letter) api.CreateLetterResponse {
	return api.CreateLetterResponse{
		LetterID: letter.ID, State: letterState(letter), FoldSeed: letter.FoldSeed,
		CreatedAt: letter.CreatedAt.Time, ExpiresAt: letter.ExpiresAt.Time,
	}
}

func replyResponse(letter dbgen.Letter) api.ReplyToLetterResponse {
	return api.ReplyToLetterResponse{LetterID: letter.ID, ReplyID: letter.ReplyID.String, RepliedAt: letter.RepliedAt.Time}
}

func letterResponse(letter dbgen.Letter, role api.LetterRole, original, reply string) api.GetLetterResponse {
	response := api.GetLetterResponse{
		LetterID: letter.ID, Role: role, State: letterState(letter), FoldSeed: letter.FoldSeed,
		CreatedAt: letter.CreatedAt.Time,
		Original:  &api.Message{Body: original, Alias: letter.SenderAlias, CreatedAt: letter.CreatedAt.Time},
	}
	if role == api.LetterRoleSender {
		response.OtherAlias = letter.RecipientAlias.String
	} else {
		response.OtherAlias = letter.SenderAlias
	}
	response.ClaimExpiresAt = timePointer(letter.ClaimExpiresAt)
	response.OpenedAt = timePointer(letter.OpenedAt)
	response.RepliedAt = timePointer(letter.RepliedAt)
	if letter.ReplyID.Valid {
		response.Reply = &api.Message{Body: reply, Alias: letter.RecipientAlias.String, CreatedAt: letter.RepliedAt.Time}
	}
	return response
}

func letterSummary(letter dbgen.Letter, role api.LetterRole) api.LetterSummary {
	summary := api.LetterSummary{
		LetterID: letter.ID, Role: role, State: letterState(letter), FoldSeed: letter.FoldSeed,
		CreatedAt: letter.CreatedAt.Time, ClaimExpiresAt: timePointer(letter.ClaimExpiresAt),
		OpenedAt: timePointer(letter.OpenedAt), RepliedAt: timePointer(letter.RepliedAt),
	}
	if role == api.LetterRoleSender {
		summary.OtherAlias = letter.RecipientAlias.String
	} else {
		summary.OtherAlias = letter.SenderAlias
	}
	return summary
}

func letterState(letter dbgen.Letter) api.LetterState {
	switch {
	case letter.WithdrawnAt.Valid:
		return api.LetterStateWithdrawn
	case letter.ReplyID.Valid:
		return api.LetterStateReplied
	case letter.OpenedAt.Valid:
		return api.LetterStateOpened
	case letter.RecipientID.Valid:
		return api.LetterStateClaimed
	default:
		return api.LetterStateWaiting
	}
}

func timePointer(value pgtype.Timestamptz) *time.Time {
	if !value.Valid {
		return nil
	}
	time := value.Time
	return &time
}

type keepsakeCursor struct {
	createdAt time.Time
	id        string
}

func encodeCursor(createdAt time.Time, id string) string {
	value := make([]byte, 8+envelope.IDLength)
	binary.BigEndian.PutUint64(value, uint64(createdAt.UnixMicro()))
	copy(value[8:], id)
	return base64.RawURLEncoding.EncodeToString(value)
}

func decodeCursor(encoded string) (*keepsakeCursor, error) {
	if encoded == "" {
		return nil, nil
	}
	value, err := base64.RawURLEncoding.Strict().DecodeString(encoded)
	if err != nil || len(value) != 8+envelope.IDLength || !validID(string(value[8:])) {
		return nil, ErrInvalid
	}
	return &keepsakeCursor{createdAt: time.UnixMicro(int64(binary.BigEndian.Uint64(value[:8]))), id: string(value[8:])}, nil
}
