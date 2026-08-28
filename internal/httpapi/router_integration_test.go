//go:build integration

package httpapi

import (
	"bytes"
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/rsa"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/aws/aws-sdk-go-v2/service/kms"
	"github.com/aws/aws-sdk-go-v2/service/kms/types"
	"github.com/go-jose/go-jose/v4"
	"github.com/go-jose/go-jose/v4/jwt"
	"github.com/jackc/pgx/v5"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/database"
	"github.com/nuggocto/orifude/internal/envelope"
	"github.com/nuggocto/orifude/internal/postoffice"
)

const (
	httpMessageKeyARN  = "arn:aws:kms:us-east-1:123456789012:key/11111111-1111-1111-1111-111111111111"
	httpEvidenceKeyARN = "arn:aws:kms:us-east-1:123456789012:key/22222222-2222-2222-2222-222222222222"
)

func TestAPIJourney(t *testing.T) {
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

	accessKey := httpRSAKey(t)
	certs := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(w).Encode(jose.JSONWebKeySet{Keys: []jose.JSONWebKey{{
			Key: &accessKey.PublicKey, KeyID: "access", Algorithm: string(jose.RS256), Use: "sig",
		}}})
	}))
	t.Cleanup(certs.Close)
	access, err := NewAccessVerifier(certs.URL, "moderation-audience")
	if err != nil {
		t.Fatal(err)
	}

	server := httptest.NewUnstartedServer(nil)
	origin := "http://" + server.Listener.Addr().String()
	verifier, err := auth.NewVerifier(origin)
	if err != nil {
		t.Fatal(err)
	}
	fakeKMS := newHTTPKMS()
	cipher, err := envelope.New(fakeKMS, rand.Reader, httpMessageKeyARN, httpEvidenceKeyARN)
	if err != nil {
		t.Fatal(err)
	}
	serviceConfig := postoffice.DefaultConfig()
	serviceConfig.LatestTUIVersion = "v0.2.0-test"
	service, err := postoffice.New(db, verifier, cipher, serviceConfig)
	if err != nil {
		t.Fatal(err)
	}
	var logs bytes.Buffer
	handler, err := New(service, db, access, Config{
		Logger: slog.New(slog.NewJSONHandler(&logs, nil)), ModerationOrigin: origin,
	})
	if err != nil {
		t.Fatal(err)
	}
	server.Config.Handler = handler
	server.Start()
	t.Cleanup(server.Close)

	senderKey := httpDeviceKey(t)
	recipientKey := httpDeviceKey(t)
	senderRevocation := httpSecret(11)
	recipientRevocation := httpSecret(12)
	senderToken := registerHTTPIdentity(t, db, server, senderKey, "HTTP Sender", senderRevocation, 21)
	recipientToken := registerHTTPIdentity(t, db, server, recipientKey, "HTTP Recipient", recipientRevocation, 22)

	invalidLetterID := httpID(30)
	invalidUTF8 := append([]byte(`{"letter_id":"`+invalidLetterID+`","body":"`), 0xff)
	invalidUTF8 = append(invalidUTF8, []byte(`"}`)...)
	invalidBodies := [][]byte{
		invalidUTF8,
		[]byte(`{"letter_id":"` + invalidLetterID + `","body":"\ud800"}`),
	}
	for index, body := range invalidBodies {
		var lettersBefore, eventsBefore int
		if err := raw.QueryRow(ctx, `SELECT (SELECT count(*) FROM letters), (SELECT count(*) FROM rate_limit_events)`).Scan(&lettersBefore, &eventsBefore); err != nil {
			t.Fatal(err)
		}
		generatedBefore := fakeKMS.generated()
		response := httpCallRaw[api.ErrorResponse](t, server, http.MethodPost, "/v1/letters", body,
			senderToken, senderKey, fmt.Sprintf("invalid-unicode-%02d", index), nil, http.StatusBadRequest)
		if response.Error.Code != api.ErrorCodeInvalidRequest {
			t.Fatalf("invalid Unicode error = %q, want %q", response.Error.Code, api.ErrorCodeInvalidRequest)
		}
		var lettersAfter, eventsAfter int
		if err := raw.QueryRow(ctx, `SELECT (SELECT count(*) FROM letters), (SELECT count(*) FROM rate_limit_events)`).Scan(&lettersAfter, &eventsAfter); err != nil {
			t.Fatal(err)
		}
		if fakeKMS.generated() != generatedBefore || lettersAfter != lettersBefore || eventsAfter != eventsBefore {
			t.Fatalf("invalid Unicode reached letter persistence or KMS: KMS %d to %d, letters %d to %d, events %d to %d",
				generatedBefore, fakeKMS.generated(), lettersBefore, lettersAfter, eventsBefore, eventsAfter)
		}
	}

	replayProof := httpDPoPProof(t, senderKey, http.MethodGet, server.URL+"/v1/me", "journey-replay-proof", "", senderToken)
	replayHeaders := map[string]string{"DPoP": replayProof}
	httpCall[api.GetMeResponse](t, server, http.MethodGet, "/v1/me", nil, senderToken, senderKey, "unused-replay-jti", replayHeaders, http.StatusOK)
	replayed := httpCall[api.ErrorResponse](t, server, http.MethodGet, "/v1/me", nil, senderToken, senderKey, "unused-replay-jti", replayHeaders, http.StatusUnauthorized)
	if replayed.Error.Code != api.ErrorCodeDPoPReplay {
		t.Fatalf("replayed proof error = %q, want %q", replayed.Error.Code, api.ErrorCodeDPoPReplay)
	}

	letterID := httpID(31)
	released := httpCall[api.CreateLetterResponse](t, server, http.MethodPost, "/v1/letters", api.CreateLetterRequest{
		LetterID: letterID, Body: "journey original plaintext",
	}, senderToken, senderKey, "journey-send-0001", nil, http.StatusCreated)
	generatedAfterSend := fakeKMS.generated()
	releasedAgain := httpCall[api.CreateLetterResponse](t, server, http.MethodPost, "/v1/letters", api.CreateLetterRequest{
		LetterID: letterID, Body: "ignored retry plaintext",
	}, senderToken, senderKey, "journey-send-retry", nil, http.StatusCreated)
	if releasedAgain != released || fakeKMS.generated() != generatedAfterSend {
		t.Fatalf("send retry = %+v, want %+v; KMS calls %d to %d", releasedAgain, released, generatedAfterSend, fakeKMS.generated())
	}
	var letterRows, senderRateEvents int
	if err := raw.QueryRow(ctx, `
		SELECT (SELECT count(*) FROM letters WHERE id = $1),
		       (SELECT count(*) FROM rate_limit_events
		        WHERE identity_id = (SELECT id FROM identities WHERE alias = 'HTTP Sender'))
	`, letterID).Scan(&letterRows, &senderRateEvents); err != nil {
		t.Fatal(err)
	}
	if letterRows != 1 || senderRateEvents != 1 {
		t.Fatalf("send retry durable state = %d letters, %d sender rate events", letterRows, senderRateEvents)
	}
	claim := httpCall[api.ClaimLetterResponse](t, server, http.MethodPost, "/v1/letters/claim", api.ClaimLetterRequest{}, recipientToken, recipientKey, "journey-claim-0002", nil, http.StatusOK)
	if claim.LetterID != letterID {
		t.Fatalf("claimed letter = %q, want %q", claim.LetterID, letterID)
	}
	opened := httpCall[api.OpenLetterResponse](t, server, http.MethodPost, "/v1/letters/"+letterID+"/open", api.OpenLetterRequest{}, recipientToken, recipientKey, "journey-open-00003", nil, http.StatusOK)
	if opened.Original.Body != "journey original plaintext" {
		t.Fatalf("opened body = %q", opened.Original.Body)
	}
	replyID := httpID(32)
	replied := httpCall[api.ReplyToLetterResponse](t, server, http.MethodPost, "/v1/letters/"+letterID+"/reply", api.ReplyToLetterRequest{
		ReplyID: replyID, Body: "journey reply plaintext",
	}, recipientToken, recipientKey, "journey-reply-0004", nil, http.StatusCreated)
	generatedAfterReply := fakeKMS.generated()
	repliedAgain := httpCall[api.ReplyToLetterResponse](t, server, http.MethodPost, "/v1/letters/"+letterID+"/reply", api.ReplyToLetterRequest{
		ReplyID: replyID, Body: "ignored reply retry plaintext",
	}, recipientToken, recipientKey, "journey-reply-retry", nil, http.StatusCreated)
	if repliedAgain != replied || fakeKMS.generated() != generatedAfterReply {
		t.Fatalf("reply retry = %+v, want %+v; KMS calls %d to %d", repliedAgain, replied, generatedAfterReply, fakeKMS.generated())
	}
	completed := httpCall[api.GetLetterResponse](t, server, http.MethodGet, "/v1/letters/"+letterID, nil, senderToken, senderKey, "journey-read-00005", nil, http.StatusOK)
	if completed.Reply == nil || completed.Reply.Body != "journey reply plaintext" {
		t.Fatalf("completed keepsake = %+v", completed)
	}
	keepsakes := httpCall[api.ListKeepsakesResponse](t, server, http.MethodGet, "/v1/keepsakes", nil, recipientToken, recipientKey, "journey-keepsake-6", nil, http.StatusOK)
	if len(keepsakes.Keepsakes) != 1 || keepsakes.Keepsakes[0].LetterID != letterID {
		t.Fatalf("keepsakes = %+v", keepsakes)
	}
	httpCall[api.BlockLetterResponse](t, server, http.MethodPost, "/v1/letters/"+letterID+"/block", api.BlockLetterRequest{}, recipientToken, recipientKey, "journey-block-0007", nil, http.StatusOK)
	reportID := httpID(33)
	reported := httpCall[api.ReportLetterResponse](t, server, http.MethodPost, "/v1/letters/"+letterID+"/report", api.ReportLetterRequest{
		ReportID: reportID, Target: api.ReportTargetReply, Reason: api.ReportReasonThreats,
	}, senderToken, senderKey, "journey-report-008", nil, http.StatusCreated)
	generatedAfterReport := fakeKMS.generated()
	reportedAgain := httpCall[api.ReportLetterResponse](t, server, http.MethodPost, "/v1/letters/"+letterID+"/report", api.ReportLetterRequest{
		ReportID: reportID, Target: api.ReportTargetOriginal, Reason: api.ReportReasonSpamOrScams,
	}, senderToken, senderKey, "journey-report-retry", nil, http.StatusCreated)
	if reportedAgain != reported || fakeKMS.generated() != generatedAfterReport {
		t.Fatalf("report retry = %+v, want %+v; KMS calls %d to %d", reportedAgain, reported, generatedAfterReport, fakeKMS.generated())
	}
	var reportRows int
	if err := raw.QueryRow(ctx, `SELECT count(*) FROM reports WHERE id = $1`, reportID).Scan(&reportRows); err != nil {
		t.Fatal(err)
	}
	if err := raw.QueryRow(ctx, `
		SELECT count(*) FROM rate_limit_events
		WHERE identity_id = (SELECT id FROM identities WHERE alias = 'HTTP Sender')
	`).Scan(&senderRateEvents); err != nil {
		t.Fatal(err)
	}
	if reportRows != 1 || senderRateEvents != 2 {
		t.Fatalf("report retry durable state = %d reports, %d sender rate events", reportRows, senderRateEvents)
	}

	var bodyCiphertext, replyCiphertext, evidenceCiphertext []byte
	if err := raw.QueryRow(ctx, `SELECT body_ciphertext, reply_ciphertext FROM letters WHERE id = $1`, letterID).Scan(&bodyCiphertext, &replyCiphertext); err != nil {
		t.Fatal(err)
	}
	if err := raw.QueryRow(ctx, `SELECT evidence_ciphertext FROM reports WHERE id = $1`, reportID).Scan(&evidenceCiphertext); err != nil {
		t.Fatal(err)
	}
	if bytes.Contains(bodyCiphertext, []byte("journey original plaintext")) ||
		bytes.Contains(replyCiphertext, []byte("journey reply plaintext")) ||
		bytes.Contains(evidenceCiphertext, []byte("journey original plaintext")) ||
		bytes.Contains(evidenceCiphertext, []byte("journey reply plaintext")) {
		t.Fatal("PostgreSQL contains known journey plaintext")
	}

	accessToken := httpAccessToken(t, accessKey, certs.URL, time.Now())
	moderationHeaders := map[string]string{
		"Cf-Access-Jwt-Assertion": accessToken,
		"X-Orifude-Moderation":    string(api.ModerationPurposeReportedContentReview),
	}
	review := httpCall[api.ReviewReportResponse](t, server, http.MethodPost, "/moderation/v1/reports/"+reportID+"/review", api.ReviewReportRequest{
		RequestID: httpID(34), Purpose: api.ModerationPurposeReportedContentReview,
	}, "", nil, "", moderationHeaders, http.StatusOK)
	if review.ReportID != reportID || len(review.Evidence.Ciphertext) == 0 {
		t.Fatalf("review = %+v", review)
	}
	httpCall[api.CloseReportResponse](t, server, http.MethodPost, "/moderation/v1/reports/"+reportID+"/close", api.CloseReportRequest{
		RequestID: httpID(35), Purpose: api.ModerationPurposeReportedContentReview, Disposition: api.ModerationDispositionNoAction,
	}, "", nil, "", moderationHeaders, http.StatusOK)

	httpCall[struct{}](t, server, http.MethodDelete, "/v1/me", nil, senderToken, senderKey, "journey-delete-0010", nil, http.StatusNoContent)
	httpCall[struct{}](t, server, http.MethodPost, "/v1/identities/revoke", api.RevokeIdentityRequest{RevocationCredential: recipientRevocation}, "", nil, "", nil, http.StatusNoContent)
	var activeIdentities, reports, audits int
	if err := raw.QueryRow(ctx, `SELECT count(*) FROM identities WHERE deleted_at IS NULL`).Scan(&activeIdentities); err != nil {
		t.Fatal(err)
	}
	if err := raw.QueryRow(ctx, `SELECT count(*) FROM reports`).Scan(&reports); err != nil {
		t.Fatal(err)
	}
	if err := raw.QueryRow(ctx, `SELECT count(*) FROM moderation_audit`).Scan(&audits); err != nil {
		t.Fatal(err)
	}
	if activeIdentities != 0 || reports != 1 || audits != 2 {
		t.Fatalf("final state = %d active identities, %d reports, %d audits", activeIdentities, reports, audits)
	}
	for _, secret := range []string{
		"journey original plaintext", "journey reply plaintext", senderToken, recipientToken, accessToken,
		senderRevocation, recipientRevocation, httpSecret(21), httpSecret(22),
		base64.StdEncoding.EncodeToString(review.Evidence.Ciphertext), base64.StdEncoding.EncodeToString(review.Evidence.WrappedKey),
	} {
		if strings.Contains(logs.String(), secret) {
			t.Fatalf("access logs contain sensitive value %q", secret)
		}
	}
}

func registerHTTPIdentity(t *testing.T, db *database.DB, server *httptest.Server, key *ecdsa.PrivateKey, alias, revocation string, seed byte) string {
	t.Helper()
	invite := httpSecret(seed)
	hash := auth.HashOpaque(invite)
	if _, err := db.Queries().CreateInvite(t.Context(), hash[:]); err != nil {
		t.Fatal(err)
	}
	challenge := httpCall[api.CreateChallengeResponse](t, server, http.MethodPost, "/v1/auth/challenges", api.CreateChallengeRequest{
		Purpose: api.ChallengePurposeRegistration, PublicJWK: httpPublicJWK(key),
	}, "", nil, "", nil, http.StatusCreated)
	revocationHash := auth.HashRevocationCredential(revocation)
	proof := httpDPoPProof(t, key, http.MethodPost, server.URL+"/v1/identities", fmt.Sprintf("register-proof-%03d", seed), challenge.Nonce, "")
	created := httpCall[api.CreateIdentityResponse](t, server, http.MethodPost, "/v1/identities", api.CreateIdentityRequest{
		ChallengeID: challenge.ChallengeID, Alias: alias, InviteCode: invite,
		RevocationHash: base64.RawURLEncoding.EncodeToString(revocationHash[:]),
	}, "", nil, "", map[string]string{"DPoP": proof}, http.StatusCreated)
	return created.AccessToken
}

func httpCall[T any](t *testing.T, server *httptest.Server, method, path string, body any, token string, key *ecdsa.PrivateKey, jti string, headers map[string]string, wantStatus int) T {
	t.Helper()
	var reader io.Reader
	hasBody := body != nil
	if body != nil {
		encoded, err := json.Marshal(body)
		if err != nil {
			t.Fatal(err)
		}
		reader = bytes.NewReader(encoded)
	}
	return doHTTPCall[T](t, server, method, path, reader, hasBody, token, key, jti, headers, wantStatus)
}

func httpCallRaw[T any](t *testing.T, server *httptest.Server, method, path string, body []byte, token string, key *ecdsa.PrivateKey, jti string, headers map[string]string, wantStatus int) T {
	t.Helper()
	return doHTTPCall[T](t, server, method, path, bytes.NewReader(body), true, token, key, jti, headers, wantStatus)
}

func doHTTPCall[T any](t *testing.T, server *httptest.Server, method, path string, body io.Reader, hasBody bool, token string, key *ecdsa.PrivateKey, jti string, headers map[string]string, wantStatus int) T {
	t.Helper()
	request, err := http.NewRequestWithContext(t.Context(), method, server.URL+path, body)
	if err != nil {
		t.Fatal(err)
	}
	if hasBody {
		request.Header.Set("Content-Type", "application/json")
	}
	if token != "" {
		request.Header.Set("Authorization", "DPoP "+token)
		request.Header.Set("DPoP", httpDPoPProof(t, key, method, server.URL+request.URL.EscapedPath(), jti, "", token))
	}
	for name, value := range headers {
		request.Header.Set(name, value)
	}
	response, err := server.Client().Do(request)
	if err != nil {
		t.Fatal(err)
	}
	defer response.Body.Close()
	data, err := io.ReadAll(io.LimitReader(response.Body, 1<<20))
	if err != nil {
		t.Fatal(err)
	}
	if response.StatusCode != wantStatus {
		t.Fatalf("%s %s status = %d, want %d: %s", method, path, response.StatusCode, wantStatus, data)
	}
	if response.Header.Get("Cache-Control") != "no-store" {
		t.Fatalf("%s %s omitted Cache-Control: no-store", method, path)
	}
	var value T
	if len(data) != 0 {
		if err := json.Unmarshal(data, &value); err != nil {
			t.Fatal(err)
		}
	}
	return value
}

func httpDeviceKey(t *testing.T) *ecdsa.PrivateKey {
	t.Helper()
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	return key
}

func httpPublicJWK(key *ecdsa.PrivateKey) api.PublicJWK {
	return api.PublicJWK{
		KeyType: "EC", Curve: "P-256", Algorithm: "ES256",
		X: base64.RawURLEncoding.EncodeToString(key.X.FillBytes(make([]byte, 32))),
		Y: base64.RawURLEncoding.EncodeToString(key.Y.FillBytes(make([]byte, 32))),
	}
}

func httpDPoPProof(t *testing.T, key *ecdsa.PrivateKey, method, uri, jti, nonce, token string) string {
	t.Helper()
	claims := map[string]any{"htm": method, "htu": uri, "iat": time.Now().Unix(), "jti": jti}
	if nonce != "" {
		claims["nonce"] = nonce
	} else {
		hash := auth.HashAccessToken(token)
		claims["ath"] = auth.EncodeHash(hash)
	}
	options := new(jose.SignerOptions).WithType("dpop+jwt")
	options.EmbedJWK = true
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
	proof, err := signed.CompactSerialize()
	if err != nil {
		t.Fatal(err)
	}
	return proof
}

func httpAccessToken(t *testing.T, key *rsa.PrivateKey, issuer string, now time.Time) string {
	t.Helper()
	signer, err := jose.NewSigner(jose.SigningKey{Algorithm: jose.RS256, Key: key}, new(jose.SignerOptions).WithHeader("kid", "access"))
	if err != nil {
		t.Fatal(err)
	}
	token, err := jwt.Signed(signer).Claims(struct {
		jwt.Claims
		Type string `json:"type"`
	}{Claims: jwt.Claims{
		Issuer: issuer, Subject: "operator-123", Audience: jwt.Audience{"moderation-audience"},
		IssuedAt: jwt.NewNumericDate(now.Add(-time.Minute)), Expiry: jwt.NewNumericDate(now.Add(time.Minute)),
	}, Type: "app"}).Serialize()
	if err != nil {
		t.Fatal(err)
	}
	return token
}

func httpRSAKey(t *testing.T) *rsa.PrivateKey {
	t.Helper()
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	return key
}

func httpID(value byte) string {
	return base64.RawURLEncoding.EncodeToString(bytes.Repeat([]byte{value}, 16))
}

func httpSecret(value byte) string {
	return base64.RawURLEncoding.EncodeToString(bytes.Repeat([]byte{value}, 32))
}

type httpKMS struct {
	mu      sync.Mutex
	counter uint64
	keys    map[string][]byte
}

func newHTTPKMS() *httpKMS {
	return &httpKMS{keys: make(map[string][]byte)}
}

func (k *httpKMS) generated() uint64 {
	k.mu.Lock()
	defer k.mu.Unlock()
	return k.counter
}

func (k *httpKMS) GenerateDataKey(_ context.Context, input *kms.GenerateDataKeyInput, _ ...func(*kms.Options)) (*kms.GenerateDataKeyOutput, error) {
	if input.KeyId == nil || input.KeySpec != types.DataKeySpecAes256 {
		return nil, errors.New("invalid GenerateDataKey request")
	}
	k.mu.Lock()
	defer k.mu.Unlock()
	k.counter++
	key := sha256.Sum256([]byte(fmt.Sprintf("%s:%d", *input.KeyId, k.counter)))
	wrapper := sha256.Sum256(append(key[:], byte(k.counter)))
	k.keys[string(wrapper[:])] = append([]byte(nil), key[:]...)
	return &kms.GenerateDataKeyOutput{KeyId: input.KeyId, Plaintext: key[:], CiphertextBlob: wrapper[:]}, nil
}

func (k *httpKMS) Decrypt(_ context.Context, input *kms.DecryptInput, _ ...func(*kms.Options)) (*kms.DecryptOutput, error) {
	if input.KeyId == nil {
		return nil, errors.New("invalid Decrypt request")
	}
	k.mu.Lock()
	defer k.mu.Unlock()
	key := k.keys[string(input.CiphertextBlob)]
	if key == nil {
		return nil, errors.New("unknown wrapped key")
	}
	return &kms.DecryptOutput{KeyId: input.KeyId, Plaintext: append([]byte(nil), key...)}, nil
}
