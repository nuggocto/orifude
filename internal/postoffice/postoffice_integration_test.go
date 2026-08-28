//go:build integration

package postoffice

import (
	"bytes"
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"errors"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/aws/aws-sdk-go-v2/service/kms"
	"github.com/aws/aws-sdk-go-v2/service/kms/types"
	"github.com/go-jose/go-jose/v4"
	"github.com/jackc/pgx/v5"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/database"
	"github.com/nuggocto/orifude/internal/database/dbgen"
	"github.com/nuggocto/orifude/internal/envelope"
)

const (
	testOrigin      = "https://api.orifude.test"
	messageKeyARN   = "arn:aws:kms:us-east-1:123456789012:key/11111111-1111-1111-1111-111111111111"
	evidenceKeyARN  = "arn:aws:kms:us-east-1:123456789012:key/22222222-2222-2222-2222-222222222222"
	testModerator   = "moderator-subject"
	testBody        = "a private integration letter"
	testReply       = "a private integration reply"
	testReportCause = api.ReportReasonHarassment
)

var integrationNow = time.Unix(1_800_000_000, 0).UTC()

func TestRegistrationSessionAndConcurrentReplay(t *testing.T) {
	service, db, raw, fake, ctx := openPostOffice(t)
	key := privateKey(t)
	invite := secret(11)
	inviteHash := auth.HashOpaque(invite)
	if _, err := db.Queries().CreateInvite(ctx, inviteHash[:]); err != nil {
		t.Fatal(err)
	}
	revocationCredential := secret(12)
	revocationHashValue := auth.HashRevocationCredential(revocationCredential)
	revocationHash := base64.RawURLEncoding.EncodeToString(revocationHashValue[:])

	register := func(alias string) (api.CreateIdentityResponse, error) {
		challenge, err := service.CreateChallenge(ctx, api.CreateChallengeRequest{
			Purpose: api.ChallengePurposeRegistration, PublicJWK: publicJWK(key),
		})
		if err != nil {
			return api.CreateIdentityResponse{}, err
		}
		proof := proof(t, key, "POST", testOrigin+"/v1/identities", "registration-proof", challenge.Nonce, "")
		return service.Register(ctx, api.CreateIdentityRequest{
			ChallengeID: challenge.ChallengeID, Alias: alias, InviteCode: invite, RevocationHash: revocationHash,
		}, proof)
	}
	registered, err := register("Maple Finch")
	if err != nil {
		t.Fatalf("register identity: %v", err)
	}
	attackerKey := privateKey(t)
	attackerChallenge, err := service.CreateChallenge(ctx, api.CreateChallengeRequest{
		Purpose: api.ChallengePurposeRegistration, PublicJWK: publicJWK(attackerKey),
	})
	if err != nil {
		t.Fatal(err)
	}
	attackerProof := proof(t, attackerKey, "POST", testOrigin+"/v1/identities", "registration-probe", attackerChallenge.Nonce, "")
	if _, err := service.Register(ctx, api.CreateIdentityRequest{
		ChallengeID: attackerChallenge.ChallengeID, Alias: "Maple Finch", InviteCode: secret(99), RevocationHash: revocationHash,
	}, attackerProof); !errors.Is(err, ErrInviteInvalid) || errors.Is(err, ErrIdentityConflict) {
		t.Fatalf("invalid-invite alias probe error = %v, want invite invalid", err)
	}
	retried, err := register("Maple Finch")
	if err != nil || retried.AccessToken == registered.AccessToken {
		t.Fatalf("idempotent registration retry = same token %t, error %v", retried.AccessToken == registered.AccessToken, err)
	}
	if _, err := register("a"); !errors.Is(err, ErrAliasInvalid) {
		t.Fatalf("invalid alias error = %v, want alias invalid", err)
	}
	if _, err := register("Changed Finch"); !errors.Is(err, ErrIdentityConflict) {
		t.Fatalf("changed registration error = %v, want conflict", err)
	}

	unknownChallenge, err := service.CreateChallenge(ctx, api.CreateChallengeRequest{Purpose: api.ChallengePurposeSession, PublicJWK: publicJWK(attackerKey)})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := service.CreateSession(ctx, api.CreateSessionRequest{ChallengeID: unknownChallenge.ChallengeID}, "invalid"); !errors.Is(err, ErrAuthentication) || errors.Is(err, auth.ErrInvalidProof) {
		t.Fatalf("unknown-key session error = %v, want authentication", err)
	}
	challenge, err := service.CreateChallenge(ctx, api.CreateChallengeRequest{Purpose: api.ChallengePurposeSession, PublicJWK: publicJWK(key)})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := service.CreateSession(ctx, api.CreateSessionRequest{ChallengeID: challenge.ChallengeID}, "invalid"); !errors.Is(err, ErrAuthentication) || errors.Is(err, auth.ErrInvalidProof) {
		t.Fatalf("known-key session error = %v, want authentication", err)
	}
	sessionProof := proof(t, key, "POST", testOrigin+"/v1/sessions", "session-proof-000", challenge.Nonce, "")
	session, err := service.CreateSession(ctx, api.CreateSessionRequest{ChallengeID: challenge.ChallengeID}, sessionProof)
	if err != nil {
		t.Fatalf("create session: %v", err)
	}
	if _, err := raw.Exec(ctx, `UPDATE identities SET last_seen_at = created_at WHERE alias = 'Maple Finch'`); err != nil {
		t.Fatal(err)
	}

	resourceProof := proof(t, key, "GET", testOrigin+"/v1/me", "resource-proof-1", "", session.AccessToken)
	principal, err := service.Authenticate(ctx, session.AccessToken, resourceProof, "GET", "/v1/me")
	if err != nil {
		t.Fatalf("authenticate: %v", err)
	}
	if _, err := service.Authenticate(ctx, session.AccessToken, resourceProof, "GET", "/v1/me"); !errors.Is(err, ErrReplay) {
		t.Fatalf("replayed proof error = %v, want replay", err)
	}
	identity, err := db.Queries().GetIdentityByID(ctx, principal.IdentityID)
	if err != nil || !identity.LastSeenAt.Time.After(identity.CreatedAt.Time) {
		t.Fatalf("last seen was not advanced: %v, %v", identity.LastSeenAt.Time, err)
	}

	concurrentProof := proof(t, key, "GET", testOrigin+"/v1/me", "resource-proof-2", "", session.AccessToken)
	start := make(chan struct{})
	errs := make(chan error, 2)
	for range 2 {
		go func() {
			<-start
			_, err := service.Authenticate(ctx, session.AccessToken, concurrentProof, "GET", "/v1/me")
			errs <- err
		}()
	}
	close(start)
	var successes, replays int
	for range 2 {
		err := <-errs
		switch {
		case err == nil:
			successes++
		case errors.Is(err, ErrReplay):
			replays++
		default:
			t.Fatalf("concurrent authentication error: %v", err)
		}
	}
	if successes != 1 || replays != 1 {
		t.Fatalf("concurrent replay outcomes = %d success, %d replay", successes, replays)
	}
	lockConn := connectPostgres(t, ctx)
	lockTx, err := lockConn.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer lockTx.Rollback(context.Background())
	if _, err := lockTx.Exec(ctx, `LOCK TABLE dpop_replays IN ACCESS EXCLUSIVE MODE`); err != nil {
		t.Fatal(err)
	}
	expiredProof := proof(t, key, "GET", testOrigin+"/v1/me", "resource-proof-3", "", session.AccessToken)
	expiredResult := make(chan error, 1)
	go func() {
		_, err := service.Authenticate(ctx, session.AccessToken, expiredProof, "GET", "/v1/me")
		expiredResult <- err
	}()
	waitForLockWaiters(t, ctx, raw, 1)
	sessionHash := auth.HashAccessToken(session.AccessToken)
	if _, err := raw.Exec(ctx, `
		UPDATE access_sessions
		SET created_at = now() - interval '15 minutes 1 second', expires_at = now() - interval '1 second'
		WHERE token_hash = $1
	`, sessionHash[:]); err != nil {
		t.Fatal(err)
	}
	if err := lockTx.Commit(ctx); err != nil {
		t.Fatal(err)
	}
	if err := <-expiredResult; !errors.Is(err, ErrSessionExpired) {
		t.Fatalf("expired session error = %v, want session expired", err)
	}
	if err := service.RevokeIdentity(ctx, revocationCredential); err != nil {
		t.Fatalf("revoke identity: %v", err)
	}
	if err := service.RevokeIdentity(ctx, revocationCredential); err != nil {
		t.Fatalf("idempotent revocation: %v", err)
	}
	if _, err := db.Queries().GetActiveIdentityByPublicKey(ctx, identity.PublicKey); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("revoked identity lookup error = %v, want no rows", err)
	}
	if fake.generateCalls != 0 {
		t.Fatalf("authentication called KMS %d times", fake.generateCalls)
	}
}

func TestLockedChallengeAndInviteCannotCrossExpiry(t *testing.T) {
	service, db, raw, _, ctx := openPostOffice(t)
	revocationHashValue := auth.HashRevocationCredential(secret(101))
	revocationHash := base64.RawURLEncoding.EncodeToString(revocationHashValue[:])

	challengeKey := privateKey(t)
	challengeInvite := secret(102)
	challengeInviteHash := auth.HashOpaque(challengeInvite)
	if _, err := db.Queries().CreateInvite(ctx, challengeInviteHash[:]); err != nil {
		t.Fatal(err)
	}
	challenge, err := service.CreateChallenge(ctx, api.CreateChallengeRequest{Purpose: api.ChallengePurposeRegistration, PublicJWK: publicJWK(challengeKey)})
	if err != nil {
		t.Fatal(err)
	}
	var challengeDeadline time.Time
	if err := raw.QueryRow(ctx, `
		WITH deadline AS (SELECT clock_timestamp() + interval '500 milliseconds' AS value)
		UPDATE auth_challenges
		SET created_at = deadline.value - interval '5 minutes', expires_at = deadline.value
		FROM deadline
		WHERE id = $1
		RETURNING expires_at
	`, challenge.ChallengeID).Scan(&challengeDeadline); err != nil {
		t.Fatal(err)
	}
	challengeLock := connectPostgres(t, ctx)
	challengeTx, err := challengeLock.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer challengeTx.Rollback(context.Background())
	if _, err := challengeTx.Exec(ctx, `SELECT 1 FROM auth_challenges WHERE id = $1 FOR UPDATE`, challenge.ChallengeID); err != nil {
		t.Fatal(err)
	}
	challengeProof := proof(t, challengeKey, "POST", testOrigin+"/v1/identities", "locked-challenge-proof", challenge.Nonce, "")
	challengeResult := make(chan error, 1)
	go func() {
		_, err := service.Register(ctx, api.CreateIdentityRequest{
			ChallengeID: challenge.ChallengeID, Alias: "Locked Challenge", InviteCode: challengeInvite, RevocationHash: revocationHash,
		}, challengeProof)
		challengeResult <- err
	}()
	waitForLockWaiters(t, ctx, raw, 1)
	waitPast(challengeDeadline)
	if err := challengeTx.Commit(ctx); err != nil {
		t.Fatal(err)
	}
	if err := <-challengeResult; !errors.Is(err, ErrAuthentication) {
		t.Fatalf("registration after locked challenge expiry = %v, want authentication", err)
	}

	inviteKey := privateKey(t)
	invite := secret(103)
	inviteHash := auth.HashOpaque(invite)
	if _, err := db.Queries().CreateInvite(ctx, inviteHash[:]); err != nil {
		t.Fatal(err)
	}
	inviteChallenge, err := service.CreateChallenge(ctx, api.CreateChallengeRequest{Purpose: api.ChallengePurposeRegistration, PublicJWK: publicJWK(inviteKey)})
	if err != nil {
		t.Fatal(err)
	}
	var inviteDeadline time.Time
	if err := raw.QueryRow(ctx, `
		WITH deadline AS (SELECT clock_timestamp() + interval '500 milliseconds' AS value)
		UPDATE invites
		SET created_at = deadline.value - interval '7 days', expires_at = deadline.value
		FROM deadline
		WHERE token_hash = $1
		RETURNING expires_at
	`, inviteHash[:]).Scan(&inviteDeadline); err != nil {
		t.Fatal(err)
	}
	inviteLock := connectPostgres(t, ctx)
	inviteTx, err := inviteLock.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer inviteTx.Rollback(context.Background())
	if _, err := inviteTx.Exec(ctx, `SELECT 1 FROM invites WHERE token_hash = $1 FOR UPDATE`, inviteHash[:]); err != nil {
		t.Fatal(err)
	}
	inviteProof := proof(t, inviteKey, "POST", testOrigin+"/v1/identities", "locked-invite-proof", inviteChallenge.Nonce, "")
	inviteResult := make(chan error, 1)
	go func() {
		_, err := service.Register(ctx, api.CreateIdentityRequest{
			ChallengeID: inviteChallenge.ChallengeID, Alias: "Locked Invite", InviteCode: invite, RevocationHash: revocationHash,
		}, inviteProof)
		inviteResult <- err
	}()
	waitForLockWaiters(t, ctx, raw, 1)
	waitPast(inviteDeadline)
	if err := inviteTx.Commit(ctx); err != nil {
		t.Fatal(err)
	}
	if err := <-inviteResult; !errors.Is(err, ErrInviteInvalid) {
		t.Fatalf("registration after locked invite expiry = %v, want invite invalid", err)
	}
	var identities int
	if err := raw.QueryRow(ctx, `SELECT count(*) FROM identities`).Scan(&identities); err != nil || identities != 0 {
		t.Fatalf("expired credential registrations = %d identities, %v", identities, err)
	}
}

func TestLetterReportModerationAndDeletionJourney(t *testing.T) {
	service, db, _, fake, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 21, "Cedar Wren")
	recipient := seedIdentity(t, ctx, db, 22, "River Wren")
	unrelated := seedIdentity(t, ctx, db, 23, "Stone Wren")
	letterID := publicID('L')

	released, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody})
	if err != nil || released.State != api.LetterStateWaiting {
		t.Fatalf("send letter = %+v, %v", released, err)
	}
	generateAfterSend := fake.generated()
	retried, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: "different retry body"})
	if err != nil || retried.CreatedAt != released.CreatedAt || fake.generated() != generateAfterSend {
		t.Fatalf("send retry = %+v, calls %d, error %v", retried, fake.generated(), err)
	}
	if _, err := service.GetLetter(ctx, Principal{IdentityID: unrelated.ID}, letterID); !errors.Is(err, ErrNotFound) {
		t.Fatalf("unrelated read error = %v, want not found", err)
	}
	decryptBeforeUnauthorized := fake.decrypted()
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: unrelated.ID}, letterID); !errors.Is(err, ErrNotFound) {
		t.Fatalf("unrelated open error = %v, want not found", err)
	}
	if fake.decrypted() != decryptBeforeUnauthorized {
		t.Fatal("unauthorized open reached KMS")
	}

	claimed, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID})
	if err != nil || claimed.LetterID != letterID {
		t.Fatalf("claim = %+v, %v", claimed, err)
	}
	reused, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID})
	if err != nil || reused.LetterID != letterID {
		t.Fatalf("claim reuse = %+v, %v", reused, err)
	}
	opened, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID)
	if err != nil || opened.Original.Body != testBody {
		t.Fatalf("open = %+v, %v", opened, err)
	}
	openedAgain, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID)
	if err != nil || openedAgain.OpenedAt != opened.OpenedAt {
		t.Fatalf("idempotent open = %+v, %v", openedAgain, err)
	}
	generateBeforeUnauthorizedReply := fake.generated()
	if _, err := service.ReplyToLetter(ctx, Principal{IdentityID: unrelated.ID}, letterID, api.ReplyToLetterRequest{ReplyID: publicID('N'), Body: testReply}); !errors.Is(err, ErrNotFound) {
		t.Fatalf("unrelated reply error = %v, want not found", err)
	}
	if fake.generated() != generateBeforeUnauthorizedReply {
		t.Fatal("unauthorized reply reached KMS")
	}

	replyID := publicID('R')
	replied, err := service.ReplyToLetter(ctx, Principal{IdentityID: recipient.ID}, letterID, api.ReplyToLetterRequest{ReplyID: replyID, Body: testReply})
	if err != nil {
		t.Fatalf("reply: %v", err)
	}
	generateAfterReply := fake.generated()
	replyRetry, err := service.ReplyToLetter(ctx, Principal{IdentityID: recipient.ID}, letterID, api.ReplyToLetterRequest{ReplyID: replyID, Body: "different retry"})
	if err != nil || replyRetry.RepliedAt != replied.RepliedAt || fake.generated() != generateAfterReply {
		t.Fatalf("reply retry = %+v, calls %d, error %v", replyRetry, fake.generated(), err)
	}
	if _, err := service.ReplyToLetter(ctx, Principal{IdentityID: recipient.ID}, letterID, api.ReplyToLetterRequest{ReplyID: publicID('Z'), Body: testReply}); !errors.Is(err, ErrAlreadyReplied) {
		t.Fatalf("second reply error = %v, want already replied", err)
	}
	if fake.generated() != generateAfterReply {
		t.Fatal("rejected second reply reached KMS")
	}
	senderView, err := service.GetLetter(ctx, Principal{IdentityID: sender.ID}, letterID)
	if err != nil || senderView.Original.Body != testBody || senderView.Reply == nil || senderView.Reply.Body != testReply {
		t.Fatalf("sender view = %+v, %v", senderView, err)
	}
	sentKeepsakes, err := service.ListKeepsakes(ctx, Principal{IdentityID: sender.ID}, api.ListKeepsakesRequest{Limit: 10})
	if err != nil || len(sentKeepsakes.Keepsakes) != 1 || sentKeepsakes.Keepsakes[0].Role != api.LetterRoleSender {
		t.Fatalf("sender keepsakes = %+v, %v", sentKeepsakes, err)
	}
	receivedKeepsakes, err := service.ListKeepsakes(ctx, Principal{IdentityID: recipient.ID}, api.ListKeepsakesRequest{Limit: 10})
	if err != nil || len(receivedKeepsakes.Keepsakes) != 1 || receivedKeepsakes.Keepsakes[0].Role != api.LetterRoleRecipient {
		t.Fatalf("recipient keepsakes = %+v, %v", receivedKeepsakes, err)
	}
	unrelatedKeepsakes, err := service.ListKeepsakes(ctx, Principal{IdentityID: unrelated.ID}, api.ListKeepsakesRequest{Limit: 10})
	if err != nil || len(unrelatedKeepsakes.Keepsakes) != 0 {
		t.Fatalf("unrelated keepsakes = %+v, %v", unrelatedKeepsakes, err)
	}

	reportID := publicID('P')
	report, err := service.ReportLetter(ctx, Principal{IdentityID: recipient.ID}, letterID, api.ReportLetterRequest{
		ReportID: reportID, Target: api.ReportTargetOriginal, Reason: testReportCause,
	})
	if err != nil {
		t.Fatalf("report: %v", err)
	}
	generateAfterReport := fake.generated()
	reportRetry, err := service.ReportLetter(ctx, Principal{IdentityID: recipient.ID}, letterID, api.ReportLetterRequest{
		ReportID: reportID, Target: api.ReportTargetReply, Reason: api.ReportReasonSpamOrScams,
	})
	if err != nil || reportRetry.CreatedAt != report.CreatedAt || fake.generated() != generateAfterReport {
		t.Fatalf("report retry = %+v, calls %d, error %v", reportRetry, fake.generated(), err)
	}
	if _, err := service.GetLetter(ctx, Principal{IdentityID: recipient.ID}, letterID); !errors.Is(err, ErrNotFound) {
		t.Fatalf("reported letter remained visible: %v", err)
	}
	if err := service.DeleteKeepsake(ctx, Principal{IdentityID: sender.ID}, letterID); err != nil {
		t.Fatalf("delete sender keepsake: %v", err)
	}
	if _, err := db.Queries().GetReportByIDForReporter(ctx, dbgen.GetReportByIDForReporterParams{ID: reportID, ReporterID: recipient.ID}); err != nil {
		t.Fatalf("report did not survive letter purge: %v", err)
	}

	reviewRequest := api.ReviewReportRequest{RequestID: publicID('Q'), Purpose: api.ModerationPurposeReportedContentReview}
	reviewed, err := service.ReviewReport(ctx, testModerator, reportID, reviewRequest)
	if err != nil || reviewed.ReportID != reportID || len(reviewed.Evidence.Ciphertext) == 0 {
		t.Fatalf("review = %+v, %v", reviewed, err)
	}
	if _, err := db.Queries().GetModerationAuditByRequestID(ctx, reviewRequest.RequestID); err != nil {
		t.Fatalf("evidence returned without durable audit: %v", err)
	}
	reviewRetry, err := service.ReviewReport(ctx, testModerator, reportID, reviewRequest)
	if err != nil || !bytes.Equal(reviewRetry.Evidence.Ciphertext, reviewed.Evidence.Ciphertext) {
		t.Fatalf("review retry = %+v, %v", reviewRetry, err)
	}
	deniedRequest := api.ReviewReportRequest{RequestID: publicID('D'), Purpose: api.ModerationPurposeReportedContentReview}
	if _, err := service.ReviewReport(ctx, "different-moderator", reportID, deniedRequest); !errors.Is(err, ErrConflict) {
		t.Fatalf("review ownership error = %v, want conflict", err)
	}
	deniedAudit, err := db.Queries().GetModerationAuditByRequestID(ctx, deniedRequest.RequestID)
	if err != nil || deniedAudit.Outcome != auditDenied {
		t.Fatalf("denied review audit = %+v, %v", deniedAudit, err)
	}

	closeRequest := api.CloseReportRequest{
		RequestID: publicID('C'), Purpose: api.ModerationPurposeReportedContentReview,
		Disposition: api.ModerationDispositionIdentityDisabled,
	}
	closed, err := service.CloseReport(ctx, testModerator, reportID, closeRequest)
	if err != nil || closed.Disposition != api.ModerationDispositionIdentityDisabled {
		t.Fatalf("close = %+v, %v", closed, err)
	}
	closedAgain, err := service.CloseReport(ctx, testModerator, reportID, closeRequest)
	if err != nil || closedAgain.ClosedAt != closed.ClosedAt {
		t.Fatalf("close retry = %+v, %v", closedAgain, err)
	}
	if _, err := db.Queries().GetActiveIdentityByPublicKey(ctx, sender.PublicKey); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("identity-disabled sender lookup error = %v, want no rows", err)
	}
}

func TestConcurrentClaimHasOneWinner(t *testing.T) {
	service, db, _, _, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 31, "Claim Sender")
	first := seedIdentity(t, ctx, db, 32, "Claim First")
	second := seedIdentity(t, ctx, db, 33, "Claim Second")
	letterID := publicID('A')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}

	start := make(chan struct{})
	type result struct {
		identity int64
		claim    api.ClaimLetterResponse
		err      error
	}
	results := make(chan result, 2)
	for _, identity := range []int64{first.ID, second.ID} {
		go func() {
			<-start
			claim, err := service.ClaimLetter(ctx, Principal{IdentityID: identity})
			results <- result{identity: identity, claim: claim, err: err}
		}()
	}
	close(start)
	var winner int64
	var noLetter int
	for range 2 {
		result := <-results
		if result.err == nil {
			winner = result.identity
			if result.claim.LetterID != letterID {
				t.Fatalf("winner claimed %s", result.claim.LetterID)
			}
		} else if errors.Is(result.err, ErrNoLetters) {
			noLetter++
		} else {
			t.Fatalf("claim error: %v", result.err)
		}
	}
	if winner == 0 || noLetter != 1 {
		t.Fatalf("claim outcomes winner=%d no-letter=%d", winner, noLetter)
	}
	letter, err := db.Queries().GetLetterForOpen(ctx, dbgen.GetLetterForOpenParams{RecipientID: winner, ID: letterID})
	if err != nil || letter.RecipientID.Int64 != winner {
		t.Fatalf("durable winner = %+v, %v", letter.RecipientID, err)
	}
}

func TestClaimRejectsLetterExpiringWhileIdentityLocked(t *testing.T) {
	service, db, raw, _, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 69, "Claim Expiry Sender")
	recipient := seedIdentity(t, ctx, db, 70, "Claim Expiry Recipient")
	letterID := publicID('v')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	var deadline time.Time
	if err := raw.QueryRow(ctx, `
		WITH deadline AS (SELECT clock_timestamp() + interval '500 milliseconds' AS value)
		UPDATE letters
		SET created_at = deadline.value - interval '7 days', expires_at = deadline.value
		FROM deadline
		WHERE id = $1
		RETURNING expires_at
	`, letterID).Scan(&deadline); err != nil {
		t.Fatal(err)
	}
	lockConn := connectPostgres(t, ctx)
	lockTx, err := lockConn.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer lockTx.Rollback(context.Background())
	if _, err := lockTx.Exec(ctx, `SELECT 1 FROM identities WHERE id = $1 FOR NO KEY UPDATE`, recipient.ID); err != nil {
		t.Fatal(err)
	}
	claimResult := make(chan error, 1)
	go func() {
		_, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID})
		claimResult <- err
	}()
	waitForLockWaiters(t, ctx, raw, 1)
	waitPast(deadline)
	if err := lockTx.Commit(ctx); err != nil {
		t.Fatal(err)
	}
	if err := <-claimResult; !errors.Is(err, ErrNoLetters) {
		t.Fatalf("claim after locked letter expiry = %v, want no letters", err)
	}
	var assigned bool
	if err := raw.QueryRow(ctx, `SELECT recipient_id IS NOT NULL FROM letters WHERE id = $1`, letterID).Scan(&assigned); err != nil || assigned {
		t.Fatalf("expired letter assigned = %t, %v", assigned, err)
	}
}

func TestConcurrentReciprocalBlocks(t *testing.T) {
	service, db, _, _, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 61, "Block Sender")
	recipient := seedIdentity(t, ctx, db, 62, "Block Recipient")
	letterID := publicID('B')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID); err != nil {
		t.Fatal(err)
	}

	start := make(chan struct{})
	errs := make(chan error, 2)
	for _, identityID := range []int64{sender.ID, recipient.ID} {
		go func() {
			<-start
			_, err := service.BlockLetter(ctx, Principal{IdentityID: identityID}, letterID)
			errs <- err
		}()
	}
	close(start)
	for range 2 {
		if err := <-errs; err != nil {
			t.Fatalf("reciprocal block: %v", err)
		}
	}
	if _, err := db.Queries().GetBlock(ctx, dbgen.GetBlockParams{BlockerID: sender.ID, BlockedID: recipient.ID}); err != nil {
		t.Fatalf("sender block: %v", err)
	}
	if _, err := db.Queries().GetBlock(ctx, dbgen.GetBlockParams{BlockerID: recipient.ID, BlockedID: sender.ID}); err != nil {
		t.Fatalf("recipient block: %v", err)
	}
}

func TestDeletionCannotLeaveANewBlock(t *testing.T) {
	service, db, raw, _, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 65, "Block Delete Sender")
	recipient := seedIdentity(t, ctx, db, 66, "Block Delete Recipient")
	letterID := publicID('t')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID); err != nil {
		t.Fatal(err)
	}
	lockConn := connectPostgres(t, ctx)
	lockTx, err := lockConn.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer lockTx.Rollback(context.Background())
	if _, err := lockTx.Exec(ctx, `SELECT 1 FROM letters WHERE id = $1 FOR UPDATE`, letterID); err != nil {
		t.Fatal(err)
	}
	deleteResult := make(chan error, 1)
	go func() { deleteResult <- service.DeleteIdentity(ctx, Principal{IdentityID: recipient.ID}) }()
	waitForLockWaiters(t, ctx, raw, 1)
	blockResult := make(chan error, 1)
	go func() {
		_, err := service.BlockLetter(ctx, Principal{IdentityID: sender.ID}, letterID)
		blockResult <- err
	}()
	waitForLockWaiters(t, ctx, raw, 2)
	if err := lockTx.Commit(ctx); err != nil {
		t.Fatal(err)
	}
	if err := <-deleteResult; err != nil {
		t.Fatalf("delete identity: %v", err)
	}
	if err := <-blockResult; !errors.Is(err, ErrNotFound) {
		t.Fatalf("block after deletion = %v, want not found", err)
	}
	var count int
	if err := raw.QueryRow(ctx, `SELECT count(*) FROM blocks WHERE blocker_id = $1 OR blocked_id = $1`, recipient.ID).Scan(&count); err != nil || count != 0 {
		t.Fatalf("blocks involving deleted identity = %d, %v", count, err)
	}
}

func TestConcurrentReportRetryReturnsFirstWrite(t *testing.T) {
	service, db, raw, _, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 63, "Report Retry Sender")
	recipient := seedIdentity(t, ctx, db, 64, "Report Retry Recipient")
	letterID := publicID('r')
	reportID := publicID('s')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID); err != nil {
		t.Fatal(err)
	}
	request := api.ReportLetterRequest{ReportID: reportID, Target: api.ReportTargetOriginal, Reason: api.ReportReasonThreats}
	start := make(chan struct{})
	type result struct {
		response api.ReportLetterResponse
		err      error
	}
	results := make(chan result, 2)
	for range 2 {
		go func() {
			<-start
			response, err := service.ReportLetter(ctx, Principal{IdentityID: recipient.ID}, letterID, request)
			results <- result{response: response, err: err}
		}()
	}
	close(start)
	var createdAt time.Time
	for range 2 {
		result := <-results
		if result.err != nil || result.response.ReportID != reportID {
			t.Fatalf("concurrent report = %+v, %v", result.response, result.err)
		}
		if createdAt.IsZero() {
			createdAt = result.response.CreatedAt
		} else if result.response.CreatedAt != createdAt {
			t.Fatalf("report retry times = %v and %v", createdAt, result.response.CreatedAt)
		}
	}
	var count int
	if err := raw.QueryRow(ctx, `SELECT count(*) FROM reports WHERE id = $1`, reportID).Scan(&count); err != nil || count != 1 {
		t.Fatalf("durable reports = %d, %v", count, err)
	}
}

func TestReplyReportAndNextReview(t *testing.T) {
	service, db, _, fake, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 24, "Reply Reporter")
	recipient := seedIdentity(t, ctx, db, 25, "Reply Writer")
	letterID := publicID('T')
	replyID := publicID('U')
	reportID := publicID('V')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ReplyToLetter(ctx, Principal{IdentityID: recipient.ID}, letterID, api.ReplyToLetterRequest{ReplyID: replyID, Body: testReply}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ReportLetter(ctx, Principal{IdentityID: sender.ID}, letterID, api.ReportLetterRequest{
		ReportID: reportID, Target: api.ReportTargetReply, Reason: api.ReportReasonThreats,
	}); err != nil {
		t.Fatalf("report reply: %v", err)
	}

	request := api.ClaimNextReportRequest{RequestID: publicID('W'), Purpose: api.ModerationPurposeReportedContentReview}
	reviewed, err := service.ClaimNextReport(ctx, testModerator, request)
	if err != nil || reviewed.ReportID != reportID || reviewed.Target != api.ReportTargetReply {
		t.Fatalf("next review = %+v, %v", reviewed, err)
	}
	if _, err := db.Queries().GetModerationAuditByRequestID(ctx, request.RequestID); err != nil {
		t.Fatalf("next review audit: %v", err)
	}
	decryptCalls := fake.decrypted()
	retried, err := service.ClaimNextReport(ctx, testModerator, request)
	if err != nil || !bytes.Equal(retried.Evidence.Ciphertext, reviewed.Evidence.Ciphertext) || fake.decrypted() != decryptCalls {
		t.Fatalf("next review retry = %+v, %v", retried, err)
	}
}

func TestReviewWaitingOnLockCannotCrossEvidenceDeadline(t *testing.T) {
	service, db, raw, _, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 81, "Deadline Sender")
	recipient := seedIdentity(t, ctx, db, 82, "Deadline Recipient")
	letterID := publicID('E')
	reportID := publicID('G')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ReportLetter(ctx, Principal{IdentityID: recipient.ID}, letterID, api.ReportLetterRequest{
		ReportID: reportID, Target: api.ReportTargetOriginal, Reason: api.ReportReasonThreats,
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ReviewReport(ctx, testModerator, reportID, api.ReviewReportRequest{
		RequestID: publicID('H'), Purpose: api.ModerationPurposeReportedContentReview,
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.CloseReport(ctx, testModerator, reportID, api.CloseReportRequest{
		RequestID: publicID('C'), Purpose: api.ModerationPurposeReportedContentReview, Disposition: api.ModerationDispositionNoAction,
	}); err != nil {
		t.Fatal(err)
	}

	var evidenceDeadline time.Time
	if err := raw.QueryRow(ctx, `SELECT clock_timestamp() + interval '300 milliseconds'`).Scan(&evidenceDeadline); err != nil {
		t.Fatal(err)
	}
	if _, err := raw.Exec(ctx, `
		UPDATE reports
		SET created_at = $2::timestamptz - interval '91 days',
		    reviewed_at = $2::timestamptz - interval '90 days 1 hour',
		    closed_at = $2::timestamptz - interval '90 days',
		    evidence_purge_at = $2,
		    record_purge_at = $2::timestamptz - interval '90 days' + interval '1 year'
		WHERE id = $1
	`, reportID, evidenceDeadline); err != nil {
		t.Fatal(err)
	}
	holder, err := raw.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer holder.Rollback(context.Background())
	if _, err := holder.Exec(ctx, `SELECT id FROM reports WHERE id = $1 FOR UPDATE`, reportID); err != nil {
		t.Fatal(err)
	}

	expiredRequestID := publicID('N')
	reviewErr := make(chan error, 1)
	go func() {
		_, err := service.ReviewReport(ctx, testModerator, reportID, api.ReviewReportRequest{
			RequestID: expiredRequestID, Purpose: api.ModerationPurposeReportedContentReview,
		})
		reviewErr <- err
	}()
	observer, err := pgx.Connect(ctx, os.Getenv("TEST_DATABASE_URL"))
	if err != nil {
		t.Fatal(err)
	}
	defer observer.Close(context.Background())
	waitCtx, cancelWait := context.WithTimeout(ctx, 2*time.Second)
	defer cancelWait()
	for {
		var blocked bool
		if err := observer.QueryRow(waitCtx, `
			SELECT EXISTS (
				SELECT 1 FROM pg_stat_activity
				WHERE wait_event_type = 'Lock' AND query LIKE '%LockReportForReview%'
			)
		`).Scan(&blocked); err != nil {
			t.Fatal(err)
		}
		if blocked {
			break
		}
		select {
		case <-waitCtx.Done():
			t.Fatal("review did not wait on the report lock")
		case <-time.After(5 * time.Millisecond):
		}
	}
	if wait := time.Until(evidenceDeadline.Add(20 * time.Millisecond)); wait > 0 {
		timer := time.NewTimer(wait)
		select {
		case <-timer.C:
		case <-ctx.Done():
			timer.Stop()
			t.Fatal(ctx.Err())
		}
	}
	if err := holder.Commit(ctx); err != nil {
		t.Fatal(err)
	}
	if err := <-reviewErr; !errors.Is(err, ErrEvidenceExpired) {
		t.Fatalf("review after evidence deadline = %v, want evidence expired", err)
	}
	audit, err := db.Queries().GetModerationAuditByRequestID(ctx, expiredRequestID)
	if err != nil || audit.ReportID != reportID || audit.Outcome != auditDenied {
		t.Fatalf("expired review audit = %+v, %v", audit, err)
	}
}

func TestClaimLimitsAndExpiredClaimRelease(t *testing.T) {
	service, db, raw, _, ctx := openPostOffice(t)
	firstSender := seedIdentity(t, ctx, db, 35, "Limit Sender One")
	secondSender := seedIdentity(t, ctx, db, 36, "Limit Sender Two")
	recipient := seedIdentity(t, ctx, db, 37, "Limit Recipient")
	firstID := publicID('F')
	secondID := publicID('S')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: firstSender.ID}, api.CreateLetterRequest{LetterID: firstID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.SendLetter(ctx, Principal{IdentityID: secondSender.ID}, api.CreateLetterRequest{LetterID: secondID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, firstID); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); !errors.Is(err, ErrRateLimited) {
		t.Fatalf("claim cooldown error = %v, want rate limited", err)
	}
	if _, err := raw.Exec(ctx, `UPDATE rate_limit_events SET created_at = now() - interval '16 minutes' WHERE identity_id = $1 AND kind = $2`, recipient.ID, rateClaim); err != nil {
		t.Fatal(err)
	}
	claim, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID})
	if err != nil || claim.LetterID != secondID {
		t.Fatalf("claim after cooldown = %+v, %v", claim, err)
	}
	if _, err := raw.Exec(ctx, `
		UPDATE letters
		SET created_at = now() - interval '2 days', expires_at = now() + interval '5 days',
		    claimed_at = now() - interval '25 hours', claim_expires_at = now() - interval '1 hour'
		WHERE id = $1
	`, secondID); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, secondID); !errors.Is(err, ErrClaimExpired) {
		t.Fatalf("expired open error = %v, want claim expired", err)
	}
	if _, err := raw.Exec(ctx, `UPDATE rate_limit_events SET created_at = now() - interval '16 minutes' WHERE identity_id = $1 AND kind = $2`, recipient.ID, rateClaim); err != nil {
		t.Fatal(err)
	}
	reclaimed, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID})
	if err != nil || reclaimed.LetterID != secondID || !reclaimed.ClaimExpiresAt.After(claim.ClaimExpiresAt) {
		t.Fatalf("expired claim release/reclaim = %+v, %v", reclaimed, err)
	}
}

func TestOpenRejectsClaimExpiringWhileLocked(t *testing.T) {
	service, db, raw, _, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 67, "Expiry Lock Sender")
	recipient := seedIdentity(t, ctx, db, 68, "Expiry Lock Recipient")
	letterID := publicID('u')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	var deadline time.Time
	if err := raw.QueryRow(ctx, `
		WITH deadline AS (SELECT clock_timestamp() + interval '500 milliseconds' AS value)
		UPDATE letters
		SET created_at = deadline.value - interval '1 day',
		    expires_at = deadline.value + interval '6 days',
		    claimed_at = deadline.value - interval '24 hours',
		    claim_expires_at = deadline.value
		FROM deadline
		WHERE id = $1
		RETURNING claim_expires_at
	`, letterID).Scan(&deadline); err != nil {
		t.Fatal(err)
	}
	lockConn := connectPostgres(t, ctx)
	lockTx, err := lockConn.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer lockTx.Rollback(context.Background())
	if _, err := lockTx.Exec(ctx, `SELECT 1 FROM letters WHERE id = $1 FOR UPDATE`, letterID); err != nil {
		t.Fatal(err)
	}
	openResult := make(chan error, 1)
	go func() {
		_, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID)
		openResult <- err
	}()
	waitForLockWaiters(t, ctx, raw, 1)
	if wait := time.Until(deadline.Add(20 * time.Millisecond)); wait > 0 {
		time.Sleep(wait)
	}
	if err := lockTx.Commit(ctx); err != nil {
		t.Fatal(err)
	}
	if err := <-openResult; !errors.Is(err, ErrClaimExpired) {
		t.Fatalf("open after locked claim expiry = %v, want claim expired", err)
	}
	var opened bool
	if err := raw.QueryRow(ctx, `SELECT opened_at IS NOT NULL FROM letters WHERE id = $1`, letterID).Scan(&opened); err != nil || opened {
		t.Fatalf("expired claim opened = %t, %v", opened, err)
	}
}

func TestRejectedWritesDoNotReachKMS(t *testing.T) {
	service, db, raw, fake, ctx := openPostOffice(t)
	limitedSender := seedIdentity(t, ctx, db, 71, "Limited Sender")
	for range 10 {
		if _, err := db.Queries().RecordRateLimitEvent(ctx, dbgen.RecordRateLimitEventParams{IdentityID: limitedSender.ID, Kind: rateSend}); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := service.SendLetter(ctx, Principal{IdentityID: limitedSender.ID}, api.CreateLetterRequest{LetterID: publicID('K'), Body: testBody}); !errors.Is(err, ErrRateLimited) {
		t.Fatalf("limited send error = %v, want rate limited", err)
	}
	if fake.generated() != 0 {
		t.Fatal("rate-limited send reached KMS")
	}

	firstSender := seedIdentity(t, ctx, db, 72, "KMS Sender One")
	secondSender := seedIdentity(t, ctx, db, 73, "KMS Sender Two")
	recipient := seedIdentity(t, ctx, db, 74, "KMS Recipient")
	firstID := publicID('I')
	secondID := publicID('J')
	for _, letter := range []struct {
		id       string
		senderID int64
	}{{firstID, firstSender.ID}, {secondID, secondSender.ID}} {
		if _, err := service.SendLetter(ctx, Principal{IdentityID: letter.senderID}, api.CreateLetterRequest{LetterID: letter.id, Body: testBody}); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, firstID); err != nil {
		t.Fatal(err)
	}
	sharedReplyID := publicID('M')
	if _, err := service.ReplyToLetter(ctx, Principal{IdentityID: recipient.ID}, firstID, api.ReplyToLetterRequest{ReplyID: sharedReplyID, Body: testReply}); err != nil {
		t.Fatal(err)
	}
	if _, err := raw.Exec(ctx, `UPDATE rate_limit_events SET created_at = now() - interval '16 minutes' WHERE identity_id = $1 AND kind = $2`, recipient.ID, rateClaim); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, secondID); err != nil {
		t.Fatal(err)
	}
	generateBeforeConflict := fake.generated()
	if _, err := service.ReplyToLetter(ctx, Principal{IdentityID: recipient.ID}, secondID, api.ReplyToLetterRequest{ReplyID: sharedReplyID, Body: testReply}); !errors.Is(err, ErrConflict) {
		t.Fatalf("reused reply ID error = %v, want conflict", err)
	}
	if fake.generated() != generateBeforeConflict {
		t.Fatal("reused reply ID reached KMS")
	}

	for range 20 {
		if _, err := db.Queries().RecordRateLimitEvent(ctx, dbgen.RecordRateLimitEventParams{IdentityID: recipient.ID, Kind: rateReport}); err != nil {
			t.Fatal(err)
		}
	}
	generateBeforeReport := fake.generated()
	decryptBeforeReport := fake.decrypted()
	if _, err := service.ReportLetter(ctx, Principal{IdentityID: recipient.ID}, secondID, api.ReportLetterRequest{
		ReportID: publicID('O'), Target: api.ReportTargetOriginal, Reason: api.ReportReasonHarassment,
	}); !errors.Is(err, ErrRateLimited) {
		t.Fatalf("limited report error = %v, want rate limited", err)
	}
	if fake.generated() != generateBeforeReport || fake.decrypted() != decryptBeforeReport {
		t.Fatal("rate-limited report reached KMS")
	}
}

func TestDeletionWinsWhileKMSIsOutsideTransaction(t *testing.T) {
	service, db, _, fake, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 41, "Delete Sender")
	fake.generateStarted = make(chan struct{})
	fake.generateRelease = make(chan struct{})

	sendErr := make(chan error, 1)
	go func() {
		_, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: publicID('X'), Body: testBody})
		sendErr <- err
	}()
	<-fake.generateStarted
	if err := service.DeleteIdentity(ctx, Principal{IdentityID: sender.ID}); err != nil {
		t.Fatalf("delete while KMS blocked: %v", err)
	}
	close(fake.generateRelease)
	if err := <-sendErr; !errors.Is(err, ErrAuthentication) {
		t.Fatalf("send after deletion error = %v, want authentication", err)
	}
	if _, err := db.Queries().GetLetterForSender(ctx, dbgen.GetLetterForSenderParams{SenderID: sender.ID, ID: publicID('X')}); !errors.Is(err, pgx.ErrNoRows) {
		t.Fatalf("send committed after deletion: %v", err)
	}
}

func TestDeletionWinsWhileOpenDecrypts(t *testing.T) {
	service, db, raw, fake, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 91, "Open Delete Sender")
	recipient := seedIdentity(t, ctx, db, 92, "Open Delete Recipient")
	letterID := publicID('a')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	fake.decryptStarted = make(chan struct{})
	fake.decryptRelease = make(chan struct{})
	openErr := make(chan error, 1)
	go func() {
		_, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID)
		openErr <- err
	}()
	<-fake.decryptStarted
	if err := service.DeleteIdentity(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	close(fake.decryptRelease)
	if err := <-openErr; !errors.Is(err, ErrNotFound) {
		t.Fatalf("open after deletion = %v, want not found", err)
	}
	var opened, assigned bool
	if err := raw.QueryRow(ctx, `SELECT opened_at IS NOT NULL, recipient_id IS NOT NULL FROM letters WHERE id = $1`, letterID).Scan(&opened, &assigned); err != nil {
		t.Fatal(err)
	}
	if opened || assigned {
		t.Fatalf("deleted recipient left opened=%t assigned=%t", opened, assigned)
	}
}

func TestDeletionWinsWhileBodyReadDecrypts(t *testing.T) {
	service, db, _, fake, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 93, "Read Delete Sender")
	recipient := seedIdentity(t, ctx, db, 94, "Read Delete Recipient")
	letterID := publicID('b')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID); err != nil {
		t.Fatal(err)
	}
	fake.decryptStarted = make(chan struct{})
	fake.decryptRelease = make(chan struct{})
	readErr := make(chan error, 1)
	go func() {
		_, err := service.GetLetter(ctx, Principal{IdentityID: recipient.ID}, letterID)
		readErr <- err
	}()
	<-fake.decryptStarted
	if err := service.DeleteIdentity(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	close(fake.decryptRelease)
	if err := <-readErr; !errors.Is(err, ErrNotFound) {
		t.Fatalf("body read after deletion = %v, want not found", err)
	}
}

func TestDeletionWinsWhileReplyEncrypts(t *testing.T) {
	service, db, raw, fake, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 95, "Reply Delete Sender")
	recipient := seedIdentity(t, ctx, db, 96, "Reply Delete Recipient")
	letterID := publicID('c')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID); err != nil {
		t.Fatal(err)
	}
	fake.generateStarted = make(chan struct{})
	fake.generateRelease = make(chan struct{})
	replyErr := make(chan error, 1)
	go func() {
		_, err := service.ReplyToLetter(ctx, Principal{IdentityID: recipient.ID}, letterID, api.ReplyToLetterRequest{ReplyID: publicID('d'), Body: testReply})
		replyErr <- err
	}()
	<-fake.generateStarted
	if err := service.DeleteIdentity(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	close(fake.generateRelease)
	if err := <-replyErr; !errors.Is(err, ErrNotFound) {
		t.Fatalf("reply after deletion = %v, want not found", err)
	}
	var replied bool
	if err := raw.QueryRow(ctx, `SELECT reply_id IS NOT NULL FROM letters WHERE id = $1`, letterID).Scan(&replied); err != nil {
		t.Fatal(err)
	}
	if replied {
		t.Fatal("reply committed after recipient deletion")
	}
}

func TestDeletionWinsWhileEvidenceEncrypts(t *testing.T) {
	service, db, raw, fake, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 97, "Report Delete Sender")
	recipient := seedIdentity(t, ctx, db, 98, "Report Delete Recipient")
	letterID := publicID('e')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID); err != nil {
		t.Fatal(err)
	}
	fake.generateStarted = make(chan struct{})
	fake.generateRelease = make(chan struct{})
	reportErr := make(chan error, 1)
	go func() {
		_, err := service.ReportLetter(ctx, Principal{IdentityID: recipient.ID}, letterID, api.ReportLetterRequest{
			ReportID: publicID('f'), Target: api.ReportTargetOriginal, Reason: api.ReportReasonThreats,
		})
		reportErr <- err
	}()
	<-fake.generateStarted
	if err := service.DeleteIdentity(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	close(fake.generateRelease)
	if err := <-reportErr; !errors.Is(err, ErrNotFound) {
		t.Fatalf("report after deletion = %v, want not found", err)
	}
	var reports int
	if err := raw.QueryRow(ctx, `SELECT count(*) FROM reports`).Scan(&reports); err != nil {
		t.Fatal(err)
	}
	if reports != 0 {
		t.Fatal("report committed after reporter deletion")
	}
}

func TestReportedIdentityDeletionWinsWhileEvidenceEncrypts(t *testing.T) {
	service, db, raw, fake, ctx := openPostOffice(t)
	sender := seedIdentity(t, ctx, db, 99, "Reported Delete Sender")
	recipient := seedIdentity(t, ctx, db, 100, "Reported Recipient")
	letterID := publicID('g')
	if _, err := service.SendLetter(ctx, Principal{IdentityID: sender.ID}, api.CreateLetterRequest{LetterID: letterID, Body: testBody}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.ClaimLetter(ctx, Principal{IdentityID: recipient.ID}); err != nil {
		t.Fatal(err)
	}
	if _, err := service.OpenLetter(ctx, Principal{IdentityID: recipient.ID}, letterID); err != nil {
		t.Fatal(err)
	}
	fake.generateStarted = make(chan struct{})
	fake.generateRelease = make(chan struct{})
	reportResult := make(chan error, 1)
	go func() {
		_, err := service.ReportLetter(ctx, Principal{IdentityID: recipient.ID}, letterID, api.ReportLetterRequest{
			ReportID: publicID('h'), Target: api.ReportTargetOriginal, Reason: api.ReportReasonThreats,
		})
		reportResult <- err
	}()
	<-fake.generateStarted
	if err := service.DeleteIdentity(ctx, Principal{IdentityID: sender.ID}); err != nil {
		t.Fatal(err)
	}
	close(fake.generateRelease)
	if err := <-reportResult; !errors.Is(err, ErrNotFound) {
		t.Fatalf("report after reported identity deletion = %v, want not found", err)
	}
	var reports, blocks int
	if err := raw.QueryRow(ctx, `SELECT count(*) FROM reports`).Scan(&reports); err != nil {
		t.Fatal(err)
	}
	if err := raw.QueryRow(ctx, `SELECT count(*) FROM blocks WHERE blocker_id = $1 OR blocked_id = $1`, sender.ID).Scan(&blocks); err != nil {
		t.Fatal(err)
	}
	if reports != 0 || blocks != 0 {
		t.Fatalf("post-deletion report state = %d reports, %d blocks", reports, blocks)
	}
}

func TestCleanupIsBoundedAndCoversRetentions(t *testing.T) {
	service, db, raw, _, ctx := openPostOffice(t)
	identity := seedIdentity(t, ctx, db, 51, "Cleanup User")
	for _, value := range []byte{'1', '2'} {
		_, err := raw.Exec(ctx, `
			INSERT INTO auth_challenges (id, public_key, key_thumbprint, purpose, nonce_hash, created_at, expires_at, used_at)
			VALUES ($1, $2, $3, 1, $4, now() - interval '10 minutes', now() - interval '5 minutes', now() - interval '6 minutes')
		`, publicID(value), publicKey(value), bytes.Repeat([]byte{value}, 32), bytes.Repeat([]byte{value + 1}, 32))
		if err != nil {
			t.Fatal(err)
		}
	}
	if _, err := raw.Exec(ctx, `INSERT INTO rate_limit_events (identity_id, kind, created_at) VALUES ($1, 1, now() - interval '2 days')`, identity.ID); err != nil {
		t.Fatal(err)
	}
	tokenHash := bytes.Repeat([]byte{71}, 32)
	if _, err := raw.Exec(ctx, `
		INSERT INTO access_sessions (token_hash, identity_id, key_thumbprint, created_at, expires_at)
		VALUES ($1, $2, $3, now() - interval '20 minutes', now() - interval '5 minutes')
	`, tokenHash, identity.ID, identity.KeyThumbprint); err != nil {
		t.Fatal(err)
	}
	if _, err := raw.Exec(ctx, `
		INSERT INTO dpop_replays (session_token_hash, jti_hash, expires_at)
		VALUES ($1, $2, now() - interval '5 minutes')
	`, tokenHash, bytes.Repeat([]byte{72}, 32)); err != nil {
		t.Fatal(err)
	}
	if _, err := raw.Exec(ctx, `UPDATE identities SET created_at = now() - interval '2 years', last_seen_at = now() - interval '366 days' WHERE id = $1`, identity.ID); err != nil {
		t.Fatal(err)
	}

	first, err := service.Cleanup(ctx, 1)
	if err != nil {
		t.Fatal(err)
	}
	if first.Challenges != 1 || first.Sessions != 1 || first.Replays != 1 || first.Identities != 1 || first.RateEvents != 1 {
		t.Fatalf("first bounded cleanup = %+v", first)
	}
	second, err := service.Cleanup(ctx, 1)
	if err != nil {
		t.Fatal(err)
	}
	if second.Challenges != 1 || second.Identities != 0 || second.RateEvents != 0 {
		t.Fatalf("second bounded cleanup = %+v", second)
	}
}

func openPostOffice(t *testing.T) (*Service, *database.DB, *pgx.Conn, *syntheticKMS, context.Context) {
	t.Helper()
	databaseURL := os.Getenv("TEST_DATABASE_URL")
	if databaseURL == "" {
		t.Skip("TEST_DATABASE_URL is set by testdata/postgres/check.sh")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	t.Cleanup(cancel)
	raw, err := pgx.Connect(ctx, databaseURL)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = raw.Close(context.Background()) })
	if _, err := raw.Exec(ctx, `
		TRUNCATE moderation_audit, reports, blocks, rate_limit_events, dpop_replays,
		access_sessions, auth_challenges, letters, invites, identities,
		alias_reservations RESTART IDENTITY CASCADE
	`); err != nil {
		t.Fatal(err)
	}
	db, err := database.Open(ctx, databaseURL, 20)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(db.Close)
	verifier, err := auth.NewVerifier(testOrigin)
	if err != nil {
		t.Fatal(err)
	}
	fake := &syntheticKMS{}
	cipher, err := envelope.New(fake, rand.Reader, messageKeyARN, evidenceKeyARN)
	if err != nil {
		t.Fatal(err)
	}
	config := DefaultConfig()
	config.Now = func() time.Time { return integrationNow }
	service, err := New(db, verifier, cipher, config)
	if err != nil {
		t.Fatal(err)
	}
	return service, db, raw, fake, ctx
}

func connectPostgres(t *testing.T, ctx context.Context) *pgx.Conn {
	t.Helper()
	conn, err := pgx.Connect(ctx, os.Getenv("TEST_DATABASE_URL"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = conn.Close(context.Background()) })
	return conn
}

func waitForLockWaiters(t *testing.T, ctx context.Context, conn *pgx.Conn, want int) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		var count int
		if err := conn.QueryRow(ctx, `SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock'`).Scan(&count); err != nil {
			t.Fatal(err)
		}
		if count >= want {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("timed out waiting for %d database lock waiters", want)
}

func waitPast(deadline time.Time) {
	if wait := time.Until(deadline.Add(20 * time.Millisecond)); wait > 0 {
		time.Sleep(wait)
	}
}

func seedIdentity(t *testing.T, ctx context.Context, db *database.DB, value byte, alias string) dbgen.Identity {
	t.Helper()
	identity, err := db.Queries().CreateIdentity(ctx, dbgen.CreateIdentityParams{
		PublicKey: publicKey(value), KeyThumbprint: bytes.Repeat([]byte{value}, 32),
		RevocationHash: bytes.Repeat([]byte{value + 1}, 32), Alias: text(alias), AliasKey: strings.ToLower(alias),
	})
	if err != nil {
		t.Fatalf("seed identity %q: %v", alias, err)
	}
	return identity
}

func privateKey(t *testing.T) *ecdsa.PrivateKey {
	t.Helper()
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	return key
}

func publicJWK(key *ecdsa.PrivateKey) api.PublicJWK {
	return api.PublicJWK{
		KeyType: "EC", Curve: "P-256",
		X:         base64.RawURLEncoding.EncodeToString(key.X.FillBytes(make([]byte, 32))),
		Y:         base64.RawURLEncoding.EncodeToString(key.Y.FillBytes(make([]byte, 32))),
		Algorithm: "ES256",
	}
}

func proof(t *testing.T, key *ecdsa.PrivateKey, method, uri, jti, nonce, token string) string {
	t.Helper()
	claims := map[string]any{"htm": method, "htu": uri, "iat": integrationNow.Unix(), "jti": jti}
	if nonce != "" {
		claims["nonce"] = nonce
	} else {
		hash := auth.HashAccessToken(token)
		claims["ath"] = auth.EncodeHash(hash)
	}
	options := &jose.SignerOptions{EmbedJWK: true}
	options.WithType("dpop+jwt")
	signer, err := jose.NewSigner(jose.SigningKey{Algorithm: jose.ES256, Key: key}, options)
	if err != nil {
		t.Fatal(err)
	}
	payload, err := json.Marshal(claims)
	if err != nil {
		t.Fatal(err)
	}
	signed, err := signer.Sign(payload)
	if err != nil {
		t.Fatal(err)
	}
	compact, err := signed.CompactSerialize()
	if err != nil {
		t.Fatal(err)
	}
	return compact
}

func publicKey(value byte) []byte {
	key := bytes.Repeat([]byte{value}, 65)
	key[0] = 4
	return key
}

func publicID(value byte) string {
	return base64.RawURLEncoding.EncodeToString(bytes.Repeat([]byte{value}, 16))
}

func secret(value byte) string {
	return base64.RawURLEncoding.EncodeToString(bytes.Repeat([]byte{value}, 32))
}

type syntheticKMS struct {
	mu              sync.Mutex
	counter         uint64
	generateCalls   int
	decryptCalls    int
	generateStarted chan struct{}
	generateRelease chan struct{}
	generateOnce    sync.Once
	decryptStarted  chan struct{}
	decryptRelease  chan struct{}
	decryptOnce     sync.Once
}

func (s *syntheticKMS) GenerateDataKey(ctx context.Context, input *kms.GenerateDataKeyInput, _ ...func(*kms.Options)) (*kms.GenerateDataKeyOutput, error) {
	if input.KeySpec != types.DataKeySpecAes256 || input.KeyId == nil {
		return nil, errors.New("invalid synthetic GenerateDataKey input")
	}
	s.mu.Lock()
	s.counter++
	s.generateCalls++
	var counter [8]byte
	binary.BigEndian.PutUint64(counter[:], s.counter)
	key := sha256.Sum256(append([]byte(*input.KeyId), counter[:]...))
	s.mu.Unlock()
	if s.generateStarted != nil {
		s.generateOnce.Do(func() { close(s.generateStarted) })
		select {
		case <-s.generateRelease:
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}
	return &kms.GenerateDataKeyOutput{KeyId: input.KeyId, Plaintext: key[:], CiphertextBlob: append([]byte(nil), key[:]...)}, nil
}

func (s *syntheticKMS) Decrypt(ctx context.Context, input *kms.DecryptInput, _ ...func(*kms.Options)) (*kms.DecryptOutput, error) {
	if input.KeyId == nil || len(input.CiphertextBlob) != envelope.DataKeyBytes {
		return nil, errors.New("invalid synthetic Decrypt input")
	}
	s.mu.Lock()
	s.decryptCalls++
	s.mu.Unlock()
	if s.decryptStarted != nil {
		s.decryptOnce.Do(func() { close(s.decryptStarted) })
		select {
		case <-s.decryptRelease:
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}
	return &kms.DecryptOutput{KeyId: input.KeyId, Plaintext: append([]byte(nil), input.CiphertextBlob...)}, nil
}

func (s *syntheticKMS) generated() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.generateCalls
}

func (s *syntheticKMS) decrypted() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.decryptCalls
}
