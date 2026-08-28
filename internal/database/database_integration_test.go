//go:build integration

package database

import (
	"bytes"
	"context"
	"encoding/base64"
	"errors"
	"fmt"
	"os"
	"sync"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/nuggocto/orifude/internal/database/dbgen"
)

func TestPoolAndTransactions(t *testing.T) {
	db, ctx := openTestDB(t)

	if err := db.Ready(ctx); err != nil {
		t.Fatalf("ready database: %v", err)
	}
	if _, err := Open(ctx, os.Getenv("TEST_DATABASE_URL"), 0); err == nil {
		t.Fatal("Open accepted a zero-sized pool")
	}

	rollback := errors.New("rollback")
	err := db.InTx(ctx, func(q *dbgen.Queries) error {
		mustCreateIdentity(t, ctx, q, 1, "rollback-user")
		return rollback
	})
	if !errors.Is(err, rollback) {
		t.Fatalf("transaction error = %v, want rollback sentinel", err)
	}
	if _, err := db.Queries().GetActiveIdentityByThumbprint(ctx, repeatedByte(1, 32)); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("rolled-back identity lookup error = %v, want no rows", err)
	}

	if err := db.InTx(ctx, func(q *dbgen.Queries) error {
		mustCreateIdentity(t, ctx, q, 2, "committed-user")
		return nil
	}); err != nil {
		t.Fatalf("commit transaction: %v", err)
	}
	if _, err := db.Queries().GetActiveIdentityByThumbprint(ctx, repeatedByte(2, 32)); err != nil {
		t.Fatalf("committed identity lookup: %v", err)
	}
}

func TestMigrationConstraints(t *testing.T) {
	db, ctx := openTestDB(t)
	q := db.Queries()

	if _, err := q.CreateIdentity(ctx, dbgen.CreateIdentityParams{
		PublicKey:      repeatedByte(1, 64),
		KeyThumbprint:  repeatedByte(1, 32),
		RevocationHash: repeatedByte(2, 32),
		Alias:          text("bad-key"),
		AliasKey:       "bad-key",
	}); err == nil {
		t.Fatal("identity accepted a non-P-256 public key length")
	}

	identity := mustCreateIdentity(t, ctx, q, 3, "reserved-name")
	if available, err := q.AliasKeyAvailable(ctx, text("reserved-name")); err != nil || available {
		t.Fatalf("active alias availability = %v, %v", available, err)
	}
	if rows, err := q.ReserveIdentityAlias(ctx, identity.ID); err != nil || rows != 0 {
		t.Fatalf("reserve already permanent alias = %d, %v", rows, err)
	}
	if _, err := q.MarkIdentityDeleted(ctx, identity.ID); err != nil {
		t.Fatalf("delete identity: %v", err)
	}
	if _, err := q.CreateIdentity(ctx, dbgen.CreateIdentityParams{
		PublicKey:      publicKey(4),
		KeyThumbprint:  repeatedByte(4, 32),
		RevocationHash: repeatedByte(5, 32),
		Alias:          text("reserved-name"),
		AliasKey:       "reserved-name",
	}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("reserved alias reuse error = %v, want no rows", err)
	}

	sender := mustCreateIdentity(t, ctx, q, 5, "sender-five")
	letter := validLetter(sender, id('A'))
	if _, err := q.CreateLetter(ctx, letter); err != nil {
		t.Fatalf("create valid letter: %v", err)
	}

	badID := letter
	badID.ID = "AAAAAAAAAAAAAAAAAAAAAB"
	if _, err := q.CreateLetter(ctx, badID); err == nil {
		t.Fatal("letter accepted a noncanonical opaque ID")
	}
	badNonce := letter
	badNonce.ID = id('B')
	badNonce.BodyNonce = repeatedByte(1, 11)
	if _, err := q.CreateLetter(ctx, badNonce); err == nil {
		t.Fatal("letter accepted an invalid nonce length")
	}
	if _, err := db.pool.Exec(ctx, `UPDATE letters SET reply_id = $1 WHERE id = $2`, id('R'), letter.ID); err == nil {
		t.Fatal("letter accepted a partial reply envelope")
	}
	if _, err := db.pool.Exec(ctx, `UPDATE letters SET recipient_id = sender_id, recipient_alias = sender_alias, claimed_at = now(), claim_expires_at = now() + interval '1 hour' WHERE id = $1`, letter.ID); err == nil {
		t.Fatal("letter accepted its sender as recipient")
	}
	if _, err := db.pool.Exec(ctx, `INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $1)`, sender.ID); err == nil {
		t.Fatal("block accepted the same identity on both sides")
	}

	var plaintextColumns int
	if err := db.pool.QueryRow(ctx, `
		SELECT count(*) FROM information_schema.columns
		WHERE table_schema = 'public'
		  AND table_name IN ('letters', 'reports')
		  AND column_name IN ('body', 'reply_body', 'evidence_body', 'plaintext')
	`).Scan(&plaintextColumns); err != nil {
		t.Fatalf("inspect plaintext columns: %v", err)
	}
	if plaintextColumns != 0 {
		t.Fatalf("schema contains %d plaintext message columns", plaintextColumns)
	}
}

func TestIdentitySessionReplayInviteAndLimitsQueries(t *testing.T) {
	db, ctx := openTestDB(t)
	q := db.Queries()
	identity := mustCreateIdentity(t, ctx, q, 6, "session-user")

	if got, err := q.GetIdentityByID(ctx, identity.ID); err != nil || got.ID != identity.ID {
		t.Fatalf("get identity = %d, %v", got.ID, err)
	}
	if got, err := q.GetActiveIdentityByPublicKey(ctx, identity.PublicKey); err != nil || got.ID != identity.ID {
		t.Fatalf("get identity by public key = %d, %v", got.ID, err)
	}
	if err := db.InTx(ctx, func(txq *dbgen.Queries) error {
		got, err := txq.GetIdentityByRevocationHashForUpdate(ctx, identity.RevocationHash)
		if err == nil && got.ID != identity.ID {
			return fmt.Errorf("revocation identity = %d, want %d", got.ID, identity.ID)
		}
		return err
	}); err != nil {
		t.Fatalf("get identity by revocation hash: %v", err)
	}
	if rows, err := q.TouchIdentity(ctx, identity.ID); err != nil || rows != 1 {
		t.Fatalf("touch identity = %d, %v", rows, err)
	}

	challengeID := id('C')
	challenge, err := q.CreateAuthChallenge(ctx, dbgen.CreateAuthChallengeParams{
		ID: challengeID, PublicKey: identity.PublicKey, KeyThumbprint: identity.KeyThumbprint,
		Purpose: 1, NonceHash: repeatedByte(7, 32),
	})
	if err != nil {
		t.Fatalf("create challenge: %v", err)
	}
	if err := db.InTx(ctx, func(txq *dbgen.Queries) error {
		locked, err := txq.GetAuthChallengeForUpdate(ctx, challengeID)
		if err != nil || locked.ID != challenge.ID {
			return fmt.Errorf("lock challenge: %w", err)
		}
		_, err = txq.ConsumeAuthChallenge(ctx, dbgen.ConsumeAuthChallengeParams{ID: challengeID, Purpose: 1})
		return err
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := q.ConsumeAuthChallenge(ctx, dbgen.ConsumeAuthChallengeParams{ID: challengeID, Purpose: 1}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("challenge replay error = %v, want no rows", err)
	}

	tokenHash := repeatedByte(8, 32)
	if _, err := q.CreateAccessSession(ctx, dbgen.CreateAccessSessionParams{
		TokenHash: tokenHash, IdentityID: identity.ID, KeyThumbprint: identity.KeyThumbprint,
	}); err != nil {
		t.Fatalf("create access session: %v", err)
	}
	if session, err := q.GetActiveAccessSession(ctx, tokenHash); err != nil || session.IdentityID != identity.ID {
		t.Fatalf("get access session = %d, %v", session.IdentityID, err)
	}
	rows, err := q.InsertDPoPReplay(ctx, dbgen.InsertDPoPReplayParams{
		JtiHash: repeatedByte(9, 32), SessionTokenHash: tokenHash,
	})
	if err != nil || rows != 1 {
		t.Fatalf("insert replay marker = %d, %v", rows, err)
	}
	if _, err := q.InsertDPoPReplay(ctx, dbgen.InsertDPoPReplayParams{
		JtiHash: repeatedByte(9, 32), SessionTokenHash: tokenHash,
	}); err == nil {
		t.Fatal("duplicate DPoP proof ID was accepted")
	}
	if _, err := db.pool.Exec(ctx, `UPDATE dpop_replays SET expires_at = now() - interval '1 second'`); err != nil {
		t.Fatal(err)
	}
	if rows, err := q.DeleteExpiredDPoPReplays(ctx); err != nil || rows != 1 {
		t.Fatalf("delete expired replay rows = %d, %v", rows, err)
	}
	rows, err = q.InsertDPoPReplay(ctx, dbgen.InsertDPoPReplayParams{
		JtiHash: repeatedByte(10, 32), SessionTokenHash: repeatedByte(99, 32),
	})
	if err != nil || rows != 0 {
		t.Fatalf("missing-session replay marker = %d, %v", rows, err)
	}

	inviteHash := repeatedByte(11, 32)
	if _, err := q.CreateInvite(ctx, inviteHash); err != nil {
		t.Fatalf("create invite: %v", err)
	}
	if err := db.InTx(ctx, func(txq *dbgen.Queries) error {
		if _, err := txq.GetInviteForUpdate(ctx, inviteHash); err != nil {
			return err
		}
		_, err := txq.RedeemInvite(ctx, dbgen.RedeemInviteParams{IdentityID: int8(identity.ID), TokenHash: inviteHash})
		return err
	}); err != nil {
		t.Fatalf("redeem invite: %v", err)
	}
	if _, err := q.RedeemInvite(ctx, dbgen.RedeemInviteParams{IdentityID: int8(identity.ID), TokenHash: inviteHash}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("invite second redemption error = %v, want no rows", err)
	}
	secondInvite := repeatedByte(12, 32)
	if _, err := q.CreateInvite(ctx, secondInvite); err != nil {
		t.Fatal(err)
	}
	if rows, err := q.RevokeInvite(ctx, secondInvite); err != nil || rows != 1 {
		t.Fatalf("revoke invite = %d, %v", rows, err)
	}

	if _, err := q.RecordRateLimitEvent(ctx, dbgen.RecordRateLimitEventParams{IdentityID: identity.ID, Kind: 2}); err != nil {
		t.Fatalf("record rate event: %v", err)
	}
	if _, err := q.RecordRateLimitEvent(ctx, dbgen.RecordRateLimitEventParams{IdentityID: identity.ID, Kind: 4}); err == nil {
		t.Fatal("rate event accepted an unknown kind")
	}
	if count, err := q.CountRateLimitEvents(ctx, dbgen.CountRateLimitEventsParams{
		IdentityID: identity.ID, Kind: 2, Since: timestamp(time.Now().Add(-time.Hour)),
	}); err != nil || count != 1 {
		t.Fatalf("rate event count = %d, %v", count, err)
	}
	if _, err := db.pool.Exec(ctx, `UPDATE rate_limit_events SET created_at = now() - interval '2 days'`); err != nil {
		t.Fatal(err)
	}
	if rows, err := q.DeleteOldRateLimitEvents(ctx, timestamp(time.Now().Add(-24*time.Hour))); err != nil || rows != 1 {
		t.Fatalf("delete old rate events = %d, %v", rows, err)
	}

	if rows, err := q.RevokeIdentitySessions(ctx, identity.ID); err != nil || rows != 1 {
		t.Fatalf("revoke sessions = %d, %v", rows, err)
	}
	if _, err := q.GetActiveAccessSession(ctx, tokenHash); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("revoked session lookup error = %v, want no rows", err)
	}
	if rows, err := q.DeleteExpiredAccessSessions(ctx); err != nil || rows != 1 {
		t.Fatalf("delete revoked sessions = %d, %v", rows, err)
	}
	if rows, err := q.DeleteExpiredAuthChallenges(ctx); err != nil || rows != 1 {
		t.Fatalf("delete used challenges = %d, %v", rows, err)
	}
	if rows, err := q.DeleteExpiredDPoPReplays(ctx); err != nil || rows != 0 {
		t.Fatalf("repeat replay cleanup = %d, %v", rows, err)
	}
}

func TestLetterClaimKeepsakeBlockAndReportQueries(t *testing.T) {
	db, ctx := openTestDB(t)
	q := db.Queries()
	sender := mustCreateIdentity(t, ctx, q, 20, "letter-sender")
	recipient := mustCreateIdentity(t, ctx, q, 21, "letter-reader")
	other := mustCreateIdentity(t, ctx, q, 22, "other-reader")

	letter := mustCreateLetter(t, ctx, q, sender, id('L'))
	if _, err := q.GetLetterForSender(ctx, dbgen.GetLetterForSenderParams{SenderID: sender.ID, ID: letter.ID}); err != nil {
		t.Fatalf("sender lookup: %v", err)
	}
	if _, err := q.GetLetterForSender(ctx, dbgen.GetLetterForSenderParams{SenderID: other.ID, ID: letter.ID}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("unrelated sender lookup error = %v, want no rows", err)
	}
	if _, err := q.SelectEligibleLetterForClaim(ctx, sender.ID); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("self claim selection error = %v, want no rows", err)
	}

	claimed, err := claimLetter(ctx, db, recipient.ID)
	if err != nil || claimed.ID != letter.ID {
		t.Fatalf("claim letter = %s, %v", claimed.ID, err)
	}
	if reused, err := claimLetter(ctx, db, recipient.ID); err != nil || reused.ID != letter.ID {
		t.Fatalf("reuse claim = %s, %v", reused.ID, err)
	}
	if _, err := q.GetLetterForRecipient(ctx, dbgen.GetLetterForRecipientParams{RecipientID: recipient.ID, ID: letter.ID}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("folded body lookup error = %v, want no rows", err)
	}
	if err := db.InTx(ctx, func(txq *dbgen.Queries) error {
		if _, err := txq.LockActiveIdentity(ctx, recipient.ID); err != nil {
			return err
		}
		if _, err := txq.LockLetterForRecipient(ctx, dbgen.LockLetterForRecipientParams{ID: letter.ID, RecipientID: int8(recipient.ID)}); err != nil {
			return err
		}
		_, err := txq.OpenLetter(ctx, dbgen.OpenLetterParams{ID: letter.ID, RecipientID: int8(recipient.ID)})
		return err
	}); err != nil {
		t.Fatalf("open letter: %v", err)
	}
	if _, err := q.GetLetterForRecipient(ctx, dbgen.GetLetterForRecipientParams{RecipientID: recipient.ID, ID: letter.ID}); err != nil {
		t.Fatalf("opened recipient lookup: %v", err)
	}

	replyID := id('Y')
	replied, err := q.AddLetterReply(ctx, dbgen.AddLetterReplyParams{
		ReplyID: text(replyID), ReplyCiphertext: repeatedByte(30, 17), ReplyNonce: repeatedByte(31, 12),
		ReplyWrappedKey: repeatedByte(32, 32), ReplyKmsKeyID: text("kms:reply"),
		ReplyEncryptionVersion: int2(1), ID: letter.ID, RecipientID: int8(recipient.ID),
	})
	if err != nil || replied.ReplyID.String != replyID {
		t.Fatalf("add reply = %q, %v", replied.ReplyID.String, err)
	}
	if _, err := q.GetLetterByReplyIDForRecipient(ctx, dbgen.GetLetterByReplyIDForRecipientParams{
		RecipientID: int8(recipient.ID), ReplyID: text(replyID),
	}); err != nil {
		t.Fatalf("reply idempotency lookup: %v", err)
	}
	if _, err := q.AddLetterReply(ctx, dbgen.AddLetterReplyParams{
		ReplyID: text(id('Z')), ReplyCiphertext: repeatedByte(33, 17), ReplyNonce: repeatedByte(34, 12),
		ReplyWrappedKey: repeatedByte(35, 32), ReplyKmsKeyID: text("kms:reply"),
		ReplyEncryptionVersion: int2(1), ID: letter.ID, RecipientID: int8(recipient.ID),
	}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("second reply error = %v, want no rows", err)
	}

	sent, err := q.ListSentKeepsakes(ctx, dbgen.ListSentKeepsakesParams{IdentityID: sender.ID, PageSize: 10})
	if err != nil || len(sent) != 1 {
		t.Fatalf("sent keepsakes = %d, %v", len(sent), err)
	}
	received, err := q.ListReceivedKeepsakes(ctx, dbgen.ListReceivedKeepsakesParams{IdentityID: int8(recipient.ID), PageSize: 10})
	if err != nil || len(received) != 1 {
		t.Fatalf("received keepsakes = %d, %v", len(received), err)
	}
	if page, err := q.ListSentKeepsakesAfter(ctx, dbgen.ListSentKeepsakesAfterParams{
		IdentityID: sender.ID, CursorCreatedAt: sent[0].CreatedAt, CursorID: sent[0].ID, PageSize: 10,
	}); err != nil || len(page) != 0 {
		t.Fatalf("sent cursor page = %d, %v", len(page), err)
	}
	if page, err := q.ListReceivedKeepsakesAfter(ctx, dbgen.ListReceivedKeepsakesAfterParams{
		IdentityID: int8(recipient.ID), CursorCreatedAt: received[0].CreatedAt, CursorID: received[0].ID, PageSize: 10,
	}); err != nil || len(page) != 0 {
		t.Fatalf("received cursor page = %d, %v", len(page), err)
	}

	if rows, err := q.CreateBlockFromLetter(ctx, dbgen.CreateBlockFromLetterParams{IdentityID: recipient.ID, LetterID: letter.ID}); err != nil || rows != 1 {
		t.Fatalf("create block = %d, %v", rows, err)
	}
	if _, err := q.GetBlock(ctx, dbgen.GetBlockParams{BlockerID: recipient.ID, BlockedID: sender.ID}); err != nil {
		t.Fatalf("get block: %v", err)
	}
	blockedLetter := mustCreateLetter(t, ctx, q, sender, id('M'))
	if _, err := q.SelectEligibleLetterForClaim(ctx, recipient.ID); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("blocked claim selection error = %v, want no rows", err)
	}
	if selected, err := q.SelectEligibleLetterForClaim(ctx, other.ID); err != nil || selected.ID != blockedLetter.ID {
		t.Fatalf("unblocked claim selection = %s, %v", selected.ID, err)
	}
	if _, err := q.AssignLetterClaim(ctx, dbgen.AssignLetterClaimParams{RecipientID: int8(other.ID), RecipientAlias: other.Alias, ID: blockedLetter.ID}); err != nil {
		t.Fatalf("assign unblocked letter: %v", err)
	}
	if _, err := q.OpenLetter(ctx, dbgen.OpenLetterParams{ID: blockedLetter.ID, RecipientID: int8(other.ID)}); err != nil {
		t.Fatalf("open unblocked letter: %v", err)
	}
	if _, err := q.RemoveReceivedKeepsake(ctx, dbgen.RemoveReceivedKeepsakeParams{ID: blockedLetter.ID, IdentityID: int8(other.ID)}); err != nil {
		t.Fatalf("remove received keepsake: %v", err)
	}
	if _, err := q.RemoveSentKeepsake(ctx, dbgen.RemoveSentKeepsakeParams{ID: blockedLetter.ID, IdentityID: sender.ID}); err != nil {
		t.Fatalf("remove sent keepsake: %v", err)
	}
	if rows, err := q.DeleteFullyRemovedLetter(ctx, blockedLetter.ID); err != nil || rows != 1 {
		t.Fatalf("delete fully removed keepsake = %d, %v", rows, err)
	}

	report := mustCreateReport(t, ctx, q, id('P'), letter.ID, recipient.ID, 1)
	if report.ReportedIdentityID != sender.ID {
		t.Fatalf("reported identity = %d, want %d", report.ReportedIdentityID, sender.ID)
	}
	if _, err := q.GetReportByIDForReporter(ctx, dbgen.GetReportByIDForReporterParams{ID: report.ID, ReporterID: recipient.ID}); err != nil {
		t.Fatalf("get report by id: %v", err)
	}
	if _, err := q.GetReportByLetterForReporter(ctx, dbgen.GetReportByLetterForReporterParams{LetterID: letter.ID, ReporterID: recipient.ID}); err != nil {
		t.Fatalf("get report by letter: %v", err)
	}
	if rows, err := q.HideReportedLetter(ctx, dbgen.HideReportedLetterParams{ReportID: report.ID, ReporterID: recipient.ID}); err != nil || rows != 1 {
		t.Fatalf("hide reported letter = %d, %v", rows, err)
	}
	if _, err := q.GetLetterForRecipient(ctx, dbgen.GetLetterForRecipientParams{RecipientID: recipient.ID, ID: letter.ID}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("hidden report lookup error = %v, want no rows", err)
	}
	if _, err := q.RemoveSentKeepsake(ctx, dbgen.RemoveSentKeepsakeParams{ID: letter.ID, IdentityID: sender.ID}); err != nil {
		t.Fatalf("remove reporter counterpart keepsake: %v", err)
	}
	if rows, err := q.DeleteFullyRemovedLetter(ctx, letter.ID); err != nil || rows != 1 {
		t.Fatalf("delete reported letter = %d, %v", rows, err)
	}
	if _, err := q.GetReportByIDForReporter(ctx, dbgen.GetReportByIDForReporterParams{ID: report.ID, ReporterID: recipient.ID}); err != nil {
		t.Fatalf("report did not survive letter deletion: %v", err)
	}

	withdrawn := mustCreateLetter(t, ctx, q, sender, id('W'))
	if err := db.InTx(ctx, func(txq *dbgen.Queries) error {
		if _, err := txq.LockActiveIdentity(ctx, sender.ID); err != nil {
			return err
		}
		if _, err := txq.LockLetterForSender(ctx, dbgen.LockLetterForSenderParams{ID: withdrawn.ID, SenderID: sender.ID}); err != nil {
			return err
		}
		_, err := txq.WithdrawLetter(ctx, dbgen.WithdrawLetterParams{ID: withdrawn.ID, SenderID: sender.ID})
		return err
	}); err != nil {
		t.Fatalf("withdraw letter: %v", err)
	}
}

func TestClaimLocksPreventDuplicateDeliveryAndHoarding(t *testing.T) {
	db, ctx := openTestDB(t)
	q := db.Queries()
	senderA := mustCreateIdentity(t, ctx, q, 40, "sender-a")
	senderB := mustCreateIdentity(t, ctx, q, 41, "sender-b")
	recipient := mustCreateIdentity(t, ctx, q, 42, "single-reader")
	first := mustCreateLetter(t, ctx, q, senderA, id('D'))
	mustCreateLetter(t, ctx, q, senderB, id('E'))

	start := make(chan struct{})
	results := make(chan dbgen.Letter, 2)
	errs := make(chan error, 2)
	var wg sync.WaitGroup
	for range 2 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			<-start
			letter, err := claimLetter(ctx, db, recipient.ID)
			results <- letter
			errs <- err
		}()
	}
	close(start)
	wg.Wait()
	close(results)
	close(errs)
	for err := range errs {
		if err != nil {
			t.Fatalf("concurrent claim: %v", err)
		}
	}
	for result := range results {
		if result.ID != first.ID {
			t.Fatalf("concurrent claim returned %s, want existing %s", result.ID, first.ID)
		}
	}

	var claimedCount int
	if err := db.pool.QueryRow(ctx, `SELECT count(*) FROM letters WHERE recipient_id = $1 AND opened_at IS NULL`, recipient.ID).Scan(&claimedCount); err != nil {
		t.Fatal(err)
	}
	if claimedCount != 1 {
		t.Fatalf("recipient has %d unopened claims, want 1", claimedCount)
	}

	otherRecipient := mustCreateIdentity(t, ctx, q, 43, "skip-reader")
	thirdRecipient := mustCreateIdentity(t, ctx, q, 44, "last-reader")
	remaining := id('E')
	locked := make(chan struct{})
	release := make(chan struct{})
	firstErr := make(chan error, 1)
	go func() {
		firstErr <- db.InTx(ctx, func(txq *dbgen.Queries) error {
			if _, err := txq.LockActiveIdentity(ctx, otherRecipient.ID); err != nil {
				return err
			}
			selected, err := txq.SelectEligibleLetterForClaim(ctx, otherRecipient.ID)
			if err != nil {
				return err
			}
			if selected.ID != remaining {
				return fmt.Errorf("selected %s, want %s", selected.ID, remaining)
			}
			close(locked)
			<-release
			_, err = txq.AssignLetterClaim(ctx, dbgen.AssignLetterClaimParams{
				RecipientID: int8(otherRecipient.ID), RecipientAlias: otherRecipient.Alias, ID: selected.ID,
			})
			return err
		})
	}()
	<-locked
	if _, err := claimLetter(ctx, db, thirdRecipient.ID); !errors.Is(err, pgx.ErrNoRows) {
		close(release)
		t.Fatalf("SKIP LOCKED competing claim error = %v, want no rows", err)
	}
	close(release)
	if err := <-firstErr; err != nil {
		t.Fatalf("locked claim: %v", err)
	}
}

func TestModerationClaimsCloseAndRetention(t *testing.T) {
	db, ctx := openTestDB(t)
	q := db.Queries()
	insertReportFixture(t, ctx, db, id('1'), id('a'), 101, 201)
	insertReportFixture(t, ctx, db, id('2'), id('b'), 102, 202)

	locked := make(chan string, 1)
	release := make(chan struct{})
	firstErr := make(chan error, 1)
	go func() {
		firstErr <- db.InTx(ctx, func(txq *dbgen.Queries) error {
			report, err := txq.LockNextUnreviewedReport(ctx)
			if err != nil {
				return err
			}
			if _, err := txq.MarkReportReviewed(ctx, dbgen.MarkReportReviewedParams{ModeratorSubject: text("moderator-one"), ID: report.ID}); err != nil {
				return err
			}
			locked <- report.ID
			<-release
			return nil
		})
	}()
	firstID := <-locked

	var secondID string
	if err := db.InTx(ctx, func(txq *dbgen.Queries) error {
		report, err := txq.LockNextUnreviewedReport(ctx)
		if err != nil {
			return err
		}
		secondID = report.ID
		_, err = txq.MarkReportReviewed(ctx, dbgen.MarkReportReviewedParams{ModeratorSubject: text("moderator-two"), ID: report.ID})
		return err
	}); err != nil {
		close(release)
		t.Fatalf("second moderation claim: %v", err)
	}
	close(release)
	if err := <-firstErr; err != nil {
		t.Fatalf("first moderation claim: %v", err)
	}
	if firstID == secondID {
		t.Fatalf("concurrent moderation claims both selected %s", firstID)
	}

	if _, err := q.GetReviewableReportForOwner(ctx, dbgen.GetReviewableReportForOwnerParams{ID: firstID, ModeratorSubject: text("moderator-one")}); err != nil {
		t.Fatalf("review owner lookup: %v", err)
	}
	if _, err := q.MarkReportReviewed(ctx, dbgen.MarkReportReviewedParams{ModeratorSubject: text("another-owner"), ID: firstID}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("review ownership conflict error = %v, want no rows", err)
	}

	audit, err := q.CreateModerationAudit(ctx, dbgen.CreateModerationAuditParams{
		RequestID: id('Q'), ReportID: firstID, ModeratorSubject: "moderator-one", Action: 1, Outcome: 1,
	})
	if err != nil {
		t.Fatalf("create moderation audit: %v", err)
	}
	if got, err := q.GetModerationAuditByRequestID(ctx, audit.RequestID); err != nil || got.ReportID != firstID {
		t.Fatalf("audit retry lookup = %s, %v", got.ReportID, err)
	}

	if err := db.InTx(ctx, func(txq *dbgen.Queries) error {
		if _, err := txq.LockReportForClose(ctx, firstID); err != nil {
			return err
		}
		_, err := txq.CloseReport(ctx, dbgen.CloseReportParams{Disposition: int2(3), ID: firstID})
		return err
	}); err != nil {
		t.Fatalf("close report: %v", err)
	}
	closed, err := q.CloseReport(ctx, dbgen.CloseReportParams{Disposition: int2(3), ID: firstID})
	if err != nil {
		t.Fatalf("idempotent close: %v", err)
	}
	if _, err := q.CloseReport(ctx, dbgen.CloseReportParams{Disposition: int2(1), ID: firstID}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("changed close disposition error = %v, want no rows", err)
	}
	if disabledID, err := q.GetDisabledIdentityFromReport(ctx, firstID); err != nil || disabledID != closed.ReportedIdentityID {
		t.Fatalf("disabled identity = %d, %v", disabledID, err)
	}

	if _, err := db.pool.Exec(ctx, `
		UPDATE reports
		SET created_at = now() - interval '200 days',
		    reviewed_at = now() - interval '100 days',
		    closed_at = now() - interval '90 days',
		    evidence_purge_at = now(),
		    record_purge_at = now() - interval '90 days' + interval '1 year'
		WHERE id = $1
	`, firstID); err != nil {
		t.Fatalf("make evidence due: %v", err)
	}
	if _, err := q.GetReviewableReportForOwner(ctx, dbgen.GetReviewableReportForOwnerParams{ID: firstID, ModeratorSubject: text("moderator-one")}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("expired evidence review error = %v, want no rows", err)
	}

	rowLocked := make(chan struct{})
	rowRelease := make(chan struct{})
	lockErr := make(chan error, 1)
	go func() {
		lockErr <- db.InTx(ctx, func(txq *dbgen.Queries) error {
			if _, err := txq.LockReportForReview(ctx, firstID); err != nil {
				return err
			}
			close(rowLocked)
			<-rowRelease
			return nil
		})
	}()
	<-rowLocked
	if _, err := q.LockNextReportForEvidencePurge(ctx); !errors.Is(err, pgx.ErrNoRows) {
		close(rowRelease)
		t.Fatalf("purge selected review-locked report: %v", err)
	}
	close(rowRelease)
	if err := <-lockErr; err != nil {
		t.Fatalf("review row lock: %v", err)
	}

	if err := db.InTx(ctx, func(txq *dbgen.Queries) error {
		report, err := txq.LockNextReportForEvidencePurge(ctx)
		if err != nil {
			return err
		}
		if report.ID != firstID {
			return fmt.Errorf("purge selected %s, want %s", report.ID, firstID)
		}
		purged, err := txq.PurgeReportEvidence(ctx, report.ID)
		if err == nil && (purged.EvidencePurgedAt.Valid == false || purged.EvidenceCiphertext != nil) {
			return errors.New("purge left evidence fields")
		}
		return err
	}); err != nil {
		t.Fatalf("purge evidence: %v", err)
	}
	if _, err := db.pool.Exec(ctx, `
		UPDATE reports
		SET created_at = now() - interval '3 years',
		    reviewed_at = now() - interval '2 years',
		    closed_at = now() - interval '1 year',
		    evidence_purge_at = now() - interval '1 year' + interval '90 days',
		    record_purge_at = now()
		WHERE id = $1
	`, firstID); err != nil {
		t.Fatalf("make report due: %v", err)
	}
	if ids, err := q.DeleteExpiredReports(ctx, 10); err != nil || len(ids) != 1 || ids[0] != firstID {
		t.Fatalf("delete expired reports = %v, %v", ids, err)
	}

	if _, err := db.pool.Exec(ctx, `UPDATE moderation_audit SET created_at = now() - interval '2 years', purge_at = now() - interval '1 year' WHERE id = $1`, audit.ID); err != nil {
		t.Fatalf("age moderation audit: %v", err)
	}
	if ids, err := q.DeleteExpiredModerationAudit(ctx, 10); err != nil || len(ids) != 1 || ids[0] != audit.ID {
		t.Fatalf("delete expired audits = %v, %v", ids, err)
	}
}

func TestCleanupAndIdentityDeletionQueries(t *testing.T) {
	db, ctx := openTestDB(t)
	q := db.Queries()
	sender := mustCreateIdentity(t, ctx, q, 60, "cleanup-sender")
	recipient := mustCreateIdentity(t, ctx, q, 61, "cleanup-reader")

	expiredClaim := mustCreateLetter(t, ctx, q, sender, id('X'))
	if _, err := q.AssignLetterClaim(ctx, dbgen.AssignLetterClaimParams{RecipientID: int8(recipient.ID), RecipientAlias: recipient.Alias, ID: expiredClaim.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := db.pool.Exec(ctx, `UPDATE letters SET created_at = now() - interval '2 days', expires_at = now() + interval '5 days', claimed_at = now() - interval '25 hours', claim_expires_at = now() - interval '1 hour' WHERE id = $1`, expiredClaim.ID); err != nil {
		t.Fatal(err)
	}
	if ids, err := q.ReleaseExpiredClaims(ctx, 10); err != nil || len(ids) != 1 || ids[0] != expiredClaim.ID {
		t.Fatalf("release expired claims = %v, %v", ids, err)
	}
	orphanedClaim := mustCreateLetter(t, ctx, q, sender, id('O'))
	if _, err := q.AssignLetterClaim(ctx, dbgen.AssignLetterClaimParams{RecipientID: int8(recipient.ID), RecipientAlias: recipient.Alias, ID: orphanedClaim.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := db.pool.Exec(ctx, `UPDATE letters SET created_at = now() - interval '2 days', expires_at = now() + interval '5 days', claimed_at = now() - interval '25 hours', claim_expires_at = now() - interval '1 hour', sender_removed_at = now() WHERE id = $1`, orphanedClaim.ID); err != nil {
		t.Fatal(err)
	}
	if ids, err := q.ReleaseExpiredClaims(ctx, 10); err != nil || len(ids) != 1 || ids[0] != orphanedClaim.ID {
		t.Fatalf("purge orphaned expired claims = %v, %v", ids, err)
	}
	var orphanedCount int
	if err := db.pool.QueryRow(ctx, `SELECT count(*) FROM letters WHERE id = $1`, orphanedClaim.ID).Scan(&orphanedCount); err != nil || orphanedCount != 0 {
		t.Fatalf("orphaned expired claim count = %d, %v", orphanedCount, err)
	}

	identityClaim := mustCreateLetter(t, ctx, q, sender, id('I'))
	if _, err := q.AssignLetterClaim(ctx, dbgen.AssignLetterClaimParams{RecipientID: int8(recipient.ID), RecipientAlias: recipient.Alias, ID: identityClaim.ID}); err != nil {
		t.Fatal(err)
	}
	if rows, err := q.ReleaseIdentityUnopenedClaims(ctx, int8(recipient.ID)); err != nil || rows != 1 {
		t.Fatalf("release identity claims = %d, %v", rows, err)
	}

	expiredWaiting := mustCreateLetter(t, ctx, q, sender, id('G'))
	if _, err := db.pool.Exec(ctx, `UPDATE letters SET created_at = now() - interval '8 days', expires_at = now() - interval '1 day' WHERE id = $1`, expiredWaiting.ID); err != nil {
		t.Fatal(err)
	}
	if _, err := q.GetLetterForSender(ctx, dbgen.GetLetterForSenderParams{SenderID: sender.ID, ID: expiredWaiting.ID}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("expired waiting sender read error = %v, want no rows", err)
	}
	if sent, err := q.ListSentKeepsakes(ctx, dbgen.ListSentKeepsakesParams{IdentityID: sender.ID, PageSize: 100}); err != nil {
		t.Fatal(err)
	} else {
		for _, letter := range sent {
			if letter.ID == expiredWaiting.ID {
				t.Fatal("expired waiting letter remained in sent keepsakes")
			}
		}
	}
	if ids, err := q.DeleteExpiredWaitingLetters(ctx, 10); err != nil || len(ids) != 1 || ids[0] != expiredWaiting.ID {
		t.Fatalf("delete expired waiting letters = %v, %v", ids, err)
	}

	waiting := mustCreateLetter(t, ctx, q, sender, id('H'))
	if rows, err := q.DeleteIdentityWaitingLetters(ctx, sender.ID); err != nil || rows < 1 {
		t.Fatalf("delete identity waiting letters = %d, %v", rows, err)
	}
	if _, err := q.GetLetterForSender(ctx, dbgen.GetLetterForSenderParams{SenderID: sender.ID, ID: waiting.ID}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("deleted waiting letter lookup error = %v", err)
	}

	withdrawn := mustCreateLetter(t, ctx, q, recipient, id('V'))
	if _, err := q.WithdrawLetter(ctx, dbgen.WithdrawLetterParams{ID: withdrawn.ID, SenderID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := db.pool.Exec(ctx, `UPDATE letters SET created_at = now() - interval '8 days', withdrawn_at = now() - interval '7 days', expires_at = now() - interval '1 day' WHERE id = $1`, withdrawn.ID); err != nil {
		t.Fatal(err)
	}
	if ids, err := q.DeleteExpiredWithdrawnLetters(ctx, 10); err != nil || len(ids) != 1 || ids[0] != withdrawn.ID {
		t.Fatalf("delete expired withdrawn letters = %v, %v", ids, err)
	}

	shared := mustCreateLetter(t, ctx, q, recipient, id('S'))
	if _, err := q.AssignLetterClaim(ctx, dbgen.AssignLetterClaimParams{RecipientID: int8(sender.ID), RecipientAlias: sender.Alias, ID: shared.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := q.OpenLetter(ctx, dbgen.OpenLetterParams{ID: shared.ID, RecipientID: int8(sender.ID)}); err != nil {
		t.Fatal(err)
	}
	if rows, err := q.CreateBlockFromLetter(ctx, dbgen.CreateBlockFromLetterParams{IdentityID: sender.ID, LetterID: shared.ID}); err != nil || rows != 1 {
		t.Fatalf("create deletion fixture block = %d, %v", rows, err)
	}

	if _, err := db.pool.Exec(ctx, `UPDATE identities SET created_at = now() - interval '2 years', last_seen_at = now() - interval '366 days' WHERE id = $1`, sender.ID); err != nil {
		t.Fatal(err)
	}
	if err := db.InTx(ctx, func(txq *dbgen.Queries) error {
		inactive, err := txq.LockNextInactiveIdentity(ctx)
		if err != nil {
			return err
		}
		if inactive.ID != sender.ID {
			return fmt.Errorf("inactive identity = %d, want %d", inactive.ID, sender.ID)
		}
		if _, err := txq.ReserveIdentityAlias(ctx, sender.ID); err != nil {
			return err
		}
		if _, err := txq.DeleteIdentityBlocks(ctx, sender.ID); err != nil {
			return err
		}
		if _, err := txq.RemoveIdentityKeepsakes(ctx, sender.ID); err != nil {
			return err
		}
		if _, err := txq.RevokeIdentitySessions(ctx, sender.ID); err != nil {
			return err
		}
		_, err = txq.MarkIdentityDeleted(ctx, sender.ID)
		return err
	}); err != nil {
		t.Fatalf("delete inactive identity: %v", err)
	}
	if _, err := q.LockActiveIdentity(ctx, sender.ID); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("deleted identity lock error = %v, want no rows", err)
	}
	var removed bool
	if err := db.pool.QueryRow(ctx, `SELECT recipient_removed_at IS NOT NULL FROM letters WHERE id = $1`, shared.ID).Scan(&removed); err != nil || !removed {
		t.Fatalf("deleted identity keepsake removal = %v, %v", removed, err)
	}
	if _, err := q.GetBlock(ctx, dbgen.GetBlockParams{BlockerID: sender.ID, BlockedID: recipient.ID}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("deleted identity block lookup error = %v, want no rows", err)
	}
}

func claimLetter(ctx context.Context, db *DB, identityID int64) (dbgen.Letter, error) {
	var claimed dbgen.Letter
	err := db.InTx(ctx, func(q *dbgen.Queries) error {
		identity, err := q.LockActiveIdentity(ctx, identityID)
		if err != nil {
			return err
		}
		claimed, err = q.GetActiveClaimForUpdate(ctx, int8(identityID))
		if err == nil {
			return nil
		}
		if !errors.Is(err, pgx.ErrNoRows) {
			return err
		}
		if _, err := q.ReleaseExpiredClaimsForIdentity(ctx, int8(identityID)); err != nil {
			return err
		}
		candidate, err := q.SelectEligibleLetterForClaim(ctx, identityID)
		if err != nil {
			return err
		}
		claimed, err = q.AssignLetterClaim(ctx, dbgen.AssignLetterClaimParams{
			RecipientID: int8(identityID), RecipientAlias: identity.Alias, ID: candidate.ID,
		})
		return err
	})
	return claimed, err
}

func openTestDB(t *testing.T) (*DB, context.Context) {
	t.Helper()
	databaseURL := os.Getenv("TEST_DATABASE_URL")
	if databaseURL == "" {
		t.Skip("TEST_DATABASE_URL is set by testdata/postgres/check.sh")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	t.Cleanup(cancel)
	db, err := Open(ctx, databaseURL, 12)
	if err != nil {
		t.Fatalf("open test database: %v", err)
	}
	t.Cleanup(db.Close)
	if _, err := db.pool.Exec(ctx, `
		TRUNCATE moderation_audit, reports, blocks, rate_limit_events, dpop_replays,
		access_sessions, auth_challenges, letters, invites, identities,
		alias_reservations RESTART IDENTITY CASCADE
	`); err != nil {
		t.Fatalf("reset test database: %v", err)
	}
	return db, ctx
}

func mustCreateIdentity(t *testing.T, ctx context.Context, q *dbgen.Queries, seed byte, alias string) dbgen.Identity {
	t.Helper()
	identity, err := q.CreateIdentity(ctx, dbgen.CreateIdentityParams{
		PublicKey: publicKey(seed), KeyThumbprint: repeatedByte(seed, 32),
		RevocationHash: repeatedByte(seed+1, 32), Alias: text(alias), AliasKey: alias,
	})
	if err != nil {
		t.Fatalf("create identity %q: %v", alias, err)
	}
	return identity
}

func mustCreateLetter(t *testing.T, ctx context.Context, q *dbgen.Queries, sender dbgen.Identity, letterID string) dbgen.Letter {
	t.Helper()
	letter, err := q.CreateLetter(ctx, validLetter(sender, letterID))
	if err != nil {
		t.Fatalf("create letter %s: %v", letterID, err)
	}
	return letter
}

func validLetter(sender dbgen.Identity, letterID string) dbgen.CreateLetterParams {
	return dbgen.CreateLetterParams{
		ID: letterID, SenderID: sender.ID, SenderAlias: sender.Alias.String,
		BodyCiphertext: repeatedByte(70, 17), BodyNonce: repeatedByte(71, 12),
		BodyWrappedKey: repeatedByte(72, 32), BodyKmsKeyID: "kms:message",
		BodyEncryptionVersion: 1, FoldSeed: 1,
	}
}

func mustCreateReport(t *testing.T, ctx context.Context, q *dbgen.Queries, reportID, letterID string, reporterID int64, target int16) dbgen.Report {
	t.Helper()
	report, err := q.CreateReport(ctx, dbgen.CreateReportParams{
		ID: reportID, ReporterID: reporterID, Target: target, Reason: 1,
		EvidenceCiphertext: repeatedByte(80, 17), EvidenceNonce: repeatedByte(81, 12),
		EvidenceWrappedKey: repeatedByte(82, 32), EvidenceKmsKeyID: text("kms:evidence"),
		EvidenceEncryptionVersion: int2(1), LetterID: letterID,
	})
	if err != nil {
		t.Fatalf("create report %s: %v", reportID, err)
	}
	return report
}

func insertReportFixture(t *testing.T, ctx context.Context, db *DB, reportID, letterID string, reporterID, reportedID int64) {
	t.Helper()
	_, err := db.pool.Exec(ctx, `
		INSERT INTO reports (
			id, letter_id, reporter_id, reported_identity_id, target, reason,
			evidence_ciphertext, evidence_nonce, evidence_wrapped_key,
			evidence_kms_key_id, evidence_encryption_version
		) VALUES ($1, $2, $3, $4, 1, 1, $5, $6, $7, 'kms:evidence', 1)
	`, reportID, letterID, reporterID, reportedID, repeatedByte(90, 17), repeatedByte(91, 12), repeatedByte(92, 32))
	if err != nil {
		t.Fatalf("insert report fixture: %v", err)
	}
}

func publicKey(seed byte) []byte {
	key := repeatedByte(seed, 65)
	key[0] = 4
	return key
}

func repeatedByte(value byte, count int) []byte {
	return bytes.Repeat([]byte{value}, count)
}

func id(value byte) string {
	return base64.RawURLEncoding.EncodeToString(bytes.Repeat([]byte{value}, 16))
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

func timestamp(value time.Time) pgtype.Timestamptz {
	return pgtype.Timestamptz{Time: value, Valid: true}
}
