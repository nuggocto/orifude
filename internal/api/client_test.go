package api

import (
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/nuggocto/orifude/internal/auth"
)

func TestDeviceClientCreatesAndRenewsSessionWithFreshProofs(t *testing.T) {
	device, err := auth.GenerateDeviceKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	token := base64.RawURLEncoding.EncodeToString(make([]byte, 32))
	nonce := base64.RawURLEncoding.EncodeToString(bytesOf(1, 32))
	var mu sync.Mutex
	var verifier *auth.Verifier
	var thumbprint [32]byte
	var sessions int
	var proofs []string

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("Cache-Control", "no-store")
		switch r.URL.Path {
		case "/v1/auth/challenges":
			var request CreateChallengeRequest
			if err := json.NewDecoder(r.Body).Decode(&request); err != nil {
				t.Error(err)
				w.WriteHeader(http.StatusBadRequest)
				return
			}
			encoded, _ := json.Marshal(request.PublicJWK)
			key, err := auth.ParsePublicJWK(encoded)
			if err != nil {
				t.Error(err)
				w.WriteHeader(http.StatusBadRequest)
				return
			}
			mu.Lock()
			thumbprint = key.Thumbprint
			mu.Unlock()
			w.WriteHeader(http.StatusCreated)
			_ = json.NewEncoder(w).Encode(CreateChallengeResponse{
				ChallengeID: base64.RawURLEncoding.EncodeToString(bytesOf(2, 16)), Nonce: nonce,
				ExpiresIn: 300, ServerTime: time.Now(),
			})
		case "/v1/sessions":
			proof := r.Header.Get("DPoP")
			mu.Lock()
			proofs = append(proofs, proof)
			currentThumbprint := thumbprint
			mu.Unlock()
			if _, err := verifier.VerifyChallenge(auth.ChallengeProofParams{
				Proof: proof, Method: http.MethodPost, EscapedPath: r.URL.EscapedPath(),
				NonceHash: auth.HashOpaque(nonce), KeyThumbprint: currentThumbprint, Now: time.Now(),
			}); err != nil {
				t.Errorf("verify session proof: %v", err)
				w.WriteHeader(http.StatusUnauthorized)
				return
			}
			mu.Lock()
			sessions++
			mu.Unlock()
			w.WriteHeader(http.StatusCreated)
			_ = json.NewEncoder(w).Encode(CreateSessionResponse{TokenType: TokenTypeDPoP, AccessToken: token, ExpiresIn: 900})
		case "/v1/me":
			proof := r.Header.Get("DPoP")
			mu.Lock()
			proofs = append(proofs, proof)
			currentSessions := sessions
			currentThumbprint := thumbprint
			mu.Unlock()
			if _, err := verifier.VerifyResource(auth.ResourceProofParams{
				Proof: proof, Method: http.MethodGet, EscapedPath: r.URL.EscapedPath(), AccessToken: token,
				KeyThumbprint: currentThumbprint, Now: time.Now(),
			}); err != nil {
				t.Errorf("verify resource proof: %v", err)
				w.WriteHeader(http.StatusUnauthorized)
				return
			}
			if currentSessions == 1 {
				w.WriteHeader(http.StatusUnauthorized)
				_ = json.NewEncoder(w).Encode(ErrorResponse{Error: APIError{Code: ErrorCodeSessionExpired, Message: "expired"}})
				return
			}
			_ = json.NewEncoder(w).Encode(GetMeResponse{Alias: "Maple Finch", LatestTUIVersion: "v0.3.0"})
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	t.Cleanup(server.Close)
	verifier, err = auth.NewVerifier(server.URL)
	if err != nil {
		t.Fatal(err)
	}
	client, err := NewClient(server.URL, server.Client())
	if err != nil {
		t.Fatal(err)
	}
	bound, err := client.ForDevice(device)
	if err != nil {
		t.Fatal(err)
	}
	me, err := bound.Me(t.Context())
	if err != nil || me.Alias != "Maple Finch" {
		t.Fatalf("Me = %+v, %v", me, err)
	}
	mu.Lock()
	defer mu.Unlock()
	if sessions != 2 {
		t.Fatalf("session creations = %d, want 2", sessions)
	}
	seen := make(map[string]struct{}, len(proofs))
	for _, proof := range proofs {
		if _, duplicate := seen[proof]; duplicate {
			t.Fatal("request reused a DPoP proof")
		}
		seen[proof] = struct{}{}
	}
}

func TestClientRejectsRedirectsOversizedAndMalformedResponses(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/redirect":
			http.Redirect(w, r, "/target", http.StatusFound)
		case "/huge":
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"status":"` + strings.Repeat("x", maxResponseBytes) + `"}`))
		case "/malformed":
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"status":`))
		case "/surrogate":
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"status":"\ud800"}`))
		case "/duplicate":
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"status":"ok","status":"ready"}`))
		case "/unknown":
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"status":"ok","extra":true}`))
		case "/wrongstatus":
			w.WriteHeader(http.StatusNoContent)
		case "/empty":
			w.Header().Set("Content-Type", "application/json")
		case "/error":
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusTooManyRequests)
			_ = json.NewEncoder(w).Encode(ErrorResponse{Error: APIError{Code: ErrorCodeRateLimited, Message: "later"}})
		default:
			t.Error("client followed a redirect")
			w.WriteHeader(http.StatusInternalServerError)
		}
	}))
	t.Cleanup(server.Close)
	client, err := NewClient(server.URL, server.Client())
	if err != nil {
		t.Fatal(err)
	}
	if _, err := doJSON[HealthResponse](t.Context(), client, http.MethodGet, "/redirect", http.StatusOK, nil, nil); !errors.Is(err, ErrProtocol) {
		t.Fatalf("redirect error = %v", err)
	}
	if _, err := doJSON[HealthResponse](t.Context(), client, http.MethodGet, "/huge", http.StatusOK, nil, nil); !errors.Is(err, ErrResponseTooLarge) {
		t.Fatalf("oversized response error = %v", err)
	}
	if _, err := doJSON[HealthResponse](t.Context(), client, http.MethodGet, "/malformed", http.StatusOK, nil, nil); !errors.Is(err, ErrProtocol) {
		t.Fatalf("malformed response error = %v", err)
	}
	for _, path := range []string{"/surrogate", "/duplicate", "/unknown"} {
		if _, err := doJSON[HealthResponse](t.Context(), client, http.MethodGet, path, http.StatusOK, nil, nil); !errors.Is(err, ErrProtocol) {
			t.Fatalf("%s response error = %v", path, err)
		}
	}
	for _, path := range []string{"/wrongstatus", "/empty"} {
		if _, err := doJSON[HealthResponse](t.Context(), client, http.MethodGet, path, http.StatusOK, nil, nil); !errors.Is(err, ErrProtocol) {
			t.Fatalf("%s response error = %v", path, err)
		}
	}
	_, err = doJSON[HealthResponse](t.Context(), client, http.MethodGet, "/error", http.StatusOK, nil, nil)
	var httpError *HTTPError
	if !errors.As(err, &httpError) || httpError.Status != http.StatusTooManyRequests || httpError.API.Code != ErrorCodeRateLimited {
		t.Fatalf("typed HTTP error = %#v, %v", httpError, err)
	}
}

func TestClientRejectsInsecureExternalOrigin(t *testing.T) {
	for _, origin := range []string{"http://example.com", "https://user@example.com", "https://example.com/path"} {
		if _, err := NewClient(origin, nil); !errors.Is(err, ErrClientConfig) {
			t.Fatalf("NewClient(%q) error = %v", origin, err)
		}
	}
}

func TestParticipantClientRouteAndDTOContract(t *testing.T) {
	deviceKey, err := auth.GenerateDeviceKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	token := base64.RawURLEncoding.EncodeToString(bytesOf(7, 32))
	nonce := base64.RawURLEncoding.EncodeToString(bytesOf(8, 32))
	now := time.Now().UTC().Truncate(time.Second)
	var mu sync.Mutex
	seen := make(map[string]int)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		seen[r.Method+" "+r.URL.RequestURI()]++
		mu.Unlock()
		w.Header().Set("Content-Type", "application/json")
		if r.URL.Path != "/v1/auth/challenges" && r.URL.Path != "/v1/identities" && r.URL.Path != "/v1/identities/revoke" {
			if r.Header.Get("Authorization") != "DPoP "+token || r.Header.Get("DPoP") == "" {
				t.Error("protected route omitted session proof")
			}
		}
		write := func(value any) { _ = json.NewEncoder(w).Encode(value) }
		switch r.Method + " " + r.URL.Path {
		case "POST /v1/auth/challenges":
			w.WriteHeader(http.StatusCreated)
			write(CreateChallengeResponse{ChallengeID: "challenge", Nonce: nonce, ExpiresIn: 300, ServerTime: now})
		case "POST /v1/identities":
			var request CreateIdentityRequest
			if err := json.NewDecoder(r.Body).Decode(&request); err != nil || request.Alias != "sora" {
				t.Errorf("registration request = %+v, %v", request, err)
			}
			w.WriteHeader(http.StatusCreated)
			write(CreateIdentityResponse{TokenType: TokenTypeDPoP, AccessToken: token, ExpiresIn: 900})
		case "POST /v1/identities/revoke":
			w.WriteHeader(http.StatusNoContent)
		case "GET /v1/me":
			write(GetMeResponse{Alias: "sora", LatestTUIVersion: "v0.3.1", Limits: Limits{BodyCodePoints: 2000}})
		case "POST /v1/letters":
			w.WriteHeader(http.StatusCreated)
			write(CreateLetterResponse{LetterID: "letter", State: LetterStateWaiting, FoldSeed: 42, CreatedAt: now, ExpiresAt: now.Add(time.Hour)})
		case "POST /v1/letters/claim":
			write(ClaimLetterResponse{LetterID: "letter", FoldSeed: 42, CreatedAt: now, ClaimExpiresAt: now.Add(time.Hour)})
		case "GET /v1/letters/letter":
			write(GetLetterResponse{LetterID: "letter", Role: LetterRoleRecipient, State: LetterStateOpened, OtherAlias: "mori", FoldSeed: 42, CreatedAt: now, Original: &Message{Alias: "mori", Body: "hello", CreatedAt: now}})
		case "POST /v1/letters/letter/open":
			write(OpenLetterResponse{LetterID: "letter", OpenedAt: now, Original: Message{Alias: "mori", Body: "hello", CreatedAt: now}})
		case "POST /v1/letters/letter/reply":
			w.WriteHeader(http.StatusCreated)
			write(ReplyToLetterResponse{LetterID: "letter", ReplyID: "reply", RepliedAt: now})
		case "POST /v1/letters/letter/withdraw":
			write(WithdrawLetterResponse{LetterID: "letter", WithdrawnAt: now})
		case "POST /v1/letters/letter/report":
			w.WriteHeader(http.StatusCreated)
			write(ReportLetterResponse{ReportID: "report", CreatedAt: now})
		case "POST /v1/letters/letter/block":
			write(BlockLetterResponse{LetterID: "letter", BlockedAt: now})
		case "GET /v1/keepsakes":
			if r.URL.Query().Get("cursor") != "cursor" || r.URL.Query().Get("limit") != "20" {
				t.Errorf("keepsake query = %q", r.URL.RawQuery)
			}
			write(ListKeepsakesResponse{Keepsakes: []LetterSummary{{LetterID: "letter", Role: LetterRoleSender, State: LetterStateReplied, FoldSeed: 42, CreatedAt: now}}, NextCursor: "next"})
		case "DELETE /v1/keepsakes/letter", "DELETE /v1/me":
			w.WriteHeader(http.StatusNoContent)
		default:
			t.Errorf("unexpected route %s %s", r.Method, r.URL.RequestURI())
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	t.Cleanup(server.Close)
	client, err := NewClient(server.URL, server.Client())
	if err != nil {
		t.Fatal(err)
	}
	device, err := client.ForDevice(deviceKey)
	if err != nil {
		t.Fatal(err)
	}
	challenge, err := device.CreateChallenge(t.Context(), ChallengePurposeRegistration)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := device.Register(t.Context(), challenge, CreateIdentityRequest{ChallengeID: challenge.ChallengeID, Alias: "sora", RevocationHash: "hash"}); err != nil {
		t.Fatal(err)
	}
	if me, err := device.Me(t.Context()); err != nil || me.Alias != "sora" {
		t.Fatalf("me = %+v, %v", me, err)
	}
	if response, err := device.CreateLetter(t.Context(), CreateLetterRequest{LetterID: "letter", Body: "hello"}); err != nil || response.LetterID != "letter" {
		t.Fatalf("create = %+v, %v", response, err)
	}
	if _, err := device.ClaimLetter(t.Context()); err != nil {
		t.Fatal(err)
	}
	if _, err := device.Letter(t.Context(), "letter"); err != nil {
		t.Fatal(err)
	}
	if _, err := device.OpenLetter(t.Context(), "letter"); err != nil {
		t.Fatal(err)
	}
	if _, err := device.Reply(t.Context(), "letter", ReplyToLetterRequest{ReplyID: "reply", Body: "reply"}); err != nil {
		t.Fatal(err)
	}
	if _, err := device.Withdraw(t.Context(), "letter"); err != nil {
		t.Fatal(err)
	}
	if _, err := device.Report(t.Context(), "letter", ReportLetterRequest{ReportID: "report", Target: ReportTargetOriginal, Reason: ReportReasonHarassment}); err != nil {
		t.Fatal(err)
	}
	if _, err := device.Block(t.Context(), "letter"); err != nil {
		t.Fatal(err)
	}
	if response, err := device.Keepsakes(t.Context(), "cursor", 20); err != nil || len(response.Keepsakes) != 1 || response.NextCursor != "next" {
		t.Fatalf("keepsakes = %+v, %v", response, err)
	}
	if err := device.DeleteKeepsake(t.Context(), "letter"); err != nil {
		t.Fatal(err)
	}
	if err := client.RevokeIdentity(t.Context(), "credential"); err != nil {
		t.Fatal(err)
	}
	if err := device.DeleteIdentity(t.Context()); err != nil {
		t.Fatal(err)
	}
	mu.Lock()
	defer mu.Unlock()
	if len(seen) != 15 {
		t.Fatalf("covered routes = %d, want 15: %#v", len(seen), seen)
	}
}

func bytesOf(value byte, count int) []byte {
	data := make([]byte, count)
	for i := range data {
		data[i] = value
	}
	return data
}
