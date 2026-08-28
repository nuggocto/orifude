package httpapi

import (
	"context"
	"crypto/rand"
	"crypto/rsa"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/go-jose/go-jose/v4"
	"github.com/go-jose/go-jose/v4/jwt"
)

type accessClaims struct {
	jwt.Claims
	Type string `json:"type"`
}

func TestAccessVerifierValidatesClaimsAndRefreshesUnknownKey(t *testing.T) {
	first := rsaKey(t)
	second := rsaKey(t)
	currentKey := first
	currentID := "first"
	var fetches atomic.Int32
	certs := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		fetches.Add(1)
		_ = json.NewEncoder(w).Encode(jose.JSONWebKeySet{Keys: []jose.JSONWebKey{{
			Key: &currentKey.PublicKey, KeyID: currentID, Algorithm: string(jose.RS256), Use: "sig",
		}}})
	}))
	defer certs.Close()

	now := time.Unix(1_800_000_000, 0).UTC()
	verifier, err := NewAccessVerifier(certs.URL, "moderation-audience")
	if err != nil {
		t.Fatal(err)
	}
	verifier.now = func() time.Time { return now }
	claims := validAccessClaims(certs.URL, now)

	subject, err := verifier.Verify(context.Background(), signAccessToken(t, first, "first", jose.RS256, claims))
	if err != nil || subject != "operator-123" {
		t.Fatalf("valid token subject = %q, error %v", subject, err)
	}
	if _, err := verifier.Verify(context.Background(), signAccessToken(t, first, "first", jose.RS256, claims)); err != nil || fetches.Load() != 1 {
		t.Fatalf("cached verification error = %v, cert fetches = %d", err, fetches.Load())
	}

	currentKey = second
	currentID = "second"
	if _, err := verifier.Verify(context.Background(), signAccessToken(t, second, "second", jose.RS256, claims)); err != nil || fetches.Load() != 2 {
		t.Fatalf("unknown-key refresh error = %v, cert fetches = %d", err, fetches.Load())
	}

	now = now.Add(time.Hour)
	claims = validAccessClaims(certs.URL, now)
	if _, err := verifier.Verify(context.Background(), signAccessToken(t, second, "second", jose.RS256, claims)); err != nil || fetches.Load() != 3 {
		t.Fatalf("expired cache verification error = %v, cert fetches = %d", err, fetches.Load())
	}
}

func TestAccessVerifierBoundsUnknownKeyRefreshes(t *testing.T) {
	key := rsaKey(t)
	var fetches atomic.Int32
	certs := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		fetches.Add(1)
		_ = json.NewEncoder(w).Encode(jose.JSONWebKeySet{Keys: []jose.JSONWebKey{{
			Key: &key.PublicKey, KeyID: "known", Algorithm: string(jose.RS256), Use: "sig",
		}}})
	}))
	defer certs.Close()

	now := time.Unix(1_800_000_000, 0).UTC()
	verifier, err := NewAccessVerifier(certs.URL, "moderation-audience")
	if err != nil {
		t.Fatal(err)
	}
	verifier.now = func() time.Time { return now }
	if _, err := verifier.key(t.Context(), "known"); err != nil {
		t.Fatal(err)
	}
	if _, err := verifier.key(t.Context(), "missing-first"); err != ErrAccessDenied {
		t.Fatalf("first unknown key error = %v, want access denied", err)
	}
	if _, err := verifier.key(t.Context(), "missing-second"); err != ErrAccessDenied {
		t.Fatalf("second unknown key error = %v, want access denied", err)
	}
	if fetches.Load() != 2 {
		t.Fatalf("cert fetches during backoff = %d, want 2", fetches.Load())
	}
	now = now.Add(accessRefreshBackoff)
	if _, err := verifier.key(t.Context(), "missing-third"); err != ErrAccessDenied {
		t.Fatalf("unknown key after backoff error = %v, want access denied", err)
	}
	if fetches.Load() != 3 {
		t.Fatalf("cert fetches after backoff = %d, want 3", fetches.Load())
	}
}

func TestAccessVerifierCoalescesRefreshWithoutBlockingCachedKeys(t *testing.T) {
	key := rsaKey(t)
	var fetches atomic.Int32
	refreshStarted := make(chan struct{})
	releaseRefresh := make(chan struct{})
	var releaseOnce sync.Once
	release := func() { releaseOnce.Do(func() { close(releaseRefresh) }) }
	certs := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		if fetches.Add(1) == 2 {
			close(refreshStarted)
			<-releaseRefresh
		}
		_ = json.NewEncoder(w).Encode(jose.JSONWebKeySet{Keys: []jose.JSONWebKey{{
			Key: &key.PublicKey, KeyID: "known", Algorithm: string(jose.RS256), Use: "sig",
		}}})
	}))
	defer certs.Close()
	defer release()

	verifier, err := NewAccessVerifier(certs.URL, "moderation-audience")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := verifier.key(t.Context(), "known"); err != nil {
		t.Fatal(err)
	}

	firstMiss := make(chan error, 1)
	go func() {
		_, err := verifier.key(t.Context(), "missing-first")
		firstMiss <- err
	}()
	select {
	case <-refreshStarted:
	case <-time.After(time.Second):
		t.Fatal("unknown-key refresh did not start")
	}

	cached := make(chan error, 1)
	go func() {
		_, err := verifier.key(t.Context(), "known")
		cached <- err
	}()
	select {
	case err := <-cached:
		if err != nil {
			t.Fatalf("cached key during refresh: %v", err)
		}
	case <-time.After(time.Second):
		t.Fatal("certificate refresh blocked a cached-key lookup")
	}

	const waiters = 8
	var group sync.WaitGroup
	errorsSeen := make(chan error, waiters)
	for index := range waiters {
		group.Add(1)
		go func() {
			defer group.Done()
			_, err := verifier.key(t.Context(), fmt.Sprintf("missing-%d", index))
			errorsSeen <- err
		}()
	}
	release()
	if err := <-firstMiss; err != ErrAccessDenied {
		t.Fatalf("first unknown key error = %v, want access denied", err)
	}
	group.Wait()
	close(errorsSeen)
	for err := range errorsSeen {
		if err != ErrAccessDenied {
			t.Fatalf("coalesced unknown key error = %v, want access denied", err)
		}
	}
	if fetches.Load() != 2 {
		t.Fatalf("coalesced cert fetches = %d, want 2", fetches.Load())
	}
}

func TestAccessVerifierRejectsInvalidTokens(t *testing.T) {
	key := rsaKey(t)
	now := time.Unix(1_800_000_000, 0).UTC()
	certs := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(w).Encode(jose.JSONWebKeySet{Keys: []jose.JSONWebKey{{
			Key: &key.PublicKey, KeyID: "access", Algorithm: string(jose.RS256), Use: "sig",
		}}})
	}))
	defer certs.Close()
	verifier, err := NewAccessVerifier(certs.URL, "moderation-audience")
	if err != nil {
		t.Fatal(err)
	}
	verifier.now = func() time.Time { return now }

	tests := []struct {
		name      string
		algorithm jose.SignatureAlgorithm
		claims    accessClaims
	}{
		{name: "algorithm", algorithm: jose.RS512, claims: validAccessClaims(certs.URL, now)},
		{name: "type", algorithm: jose.RS256, claims: changeClaims(certs.URL, now, func(c *accessClaims) { c.Type = "org" })},
		{name: "issuer", algorithm: jose.RS256, claims: changeClaims(certs.URL, now, func(c *accessClaims) { c.Issuer = "https://other.test" })},
		{name: "audience", algorithm: jose.RS256, claims: changeClaims(certs.URL, now, func(c *accessClaims) { c.Audience = jwt.Audience{"other"} })},
		{name: "issued at missing", algorithm: jose.RS256, claims: changeClaims(certs.URL, now, func(c *accessClaims) { c.IssuedAt = nil })},
		{name: "issued in future", algorithm: jose.RS256, claims: changeClaims(certs.URL, now, func(c *accessClaims) { c.IssuedAt = jwt.NewNumericDate(now.Add(time.Second)) })},
		{name: "expired", algorithm: jose.RS256, claims: changeClaims(certs.URL, now, func(c *accessClaims) { c.Expiry = jwt.NewNumericDate(now.Add(-time.Second)) })},
		{name: "not before", algorithm: jose.RS256, claims: changeClaims(certs.URL, now, func(c *accessClaims) { c.NotBefore = jwt.NewNumericDate(now.Add(time.Second)) })},
		{name: "subject", algorithm: jose.RS256, claims: changeClaims(certs.URL, now, func(c *accessClaims) { c.Subject = "" })},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			token := signAccessToken(t, key, "access", test.algorithm, test.claims)
			if _, err := verifier.Verify(context.Background(), token); err != ErrAccessDenied {
				t.Fatalf("Verify error = %v, want access denied", err)
			}
		})
	}
	signer, err := jose.NewSigner(jose.SigningKey{Algorithm: jose.RS256, Key: key}, new(jose.SignerOptions).WithHeader("kid", "access"))
	if err != nil {
		t.Fatal(err)
	}
	payload := []byte(fmt.Sprintf(`{"iss":%q,"sub":"operator-123","aud":"moderation-audience","iat":%d,"exp":%d,"type":"app","type":"org"}`,
		certs.URL, now.Add(-time.Minute).Unix(), now.Add(time.Minute).Unix()))
	signed, err := signer.Sign(payload)
	if err != nil {
		t.Fatal(err)
	}
	duplicate, err := signed.CompactSerialize()
	if err != nil {
		t.Fatal(err)
	}
	if _, err := verifier.Verify(context.Background(), duplicate); err != ErrAccessDenied {
		t.Fatalf("duplicate claim error = %v, want access denied", err)
	}
}

func TestAccessVerifierFailsClosedOnCertFetchError(t *testing.T) {
	certs := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "unavailable", http.StatusServiceUnavailable)
	}))
	defer certs.Close()
	verifier, err := NewAccessVerifier(certs.URL, "audience")
	if err != nil {
		t.Fatal(err)
	}
	key := rsaKey(t)
	if _, err := verifier.Verify(context.Background(), signAccessToken(t, key, "missing", jose.RS256, validAccessClaims(certs.URL, time.Now()))); err != ErrAccessDenied {
		t.Fatalf("Verify error = %v, want access denied", err)
	}
}

func validAccessClaims(issuer string, now time.Time) accessClaims {
	return accessClaims{Claims: jwt.Claims{
		Issuer: issuer, Subject: "operator-123", Audience: jwt.Audience{"moderation-audience"},
		IssuedAt: jwt.NewNumericDate(now.Add(-time.Minute)), Expiry: jwt.NewNumericDate(now.Add(time.Minute)),
	}, Type: "app"}
}

func changeClaims(issuer string, now time.Time, change func(*accessClaims)) accessClaims {
	claims := validAccessClaims(issuer, now)
	change(&claims)
	return claims
}

func rsaKey(t *testing.T) *rsa.PrivateKey {
	t.Helper()
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	return key
}

func signAccessToken(t *testing.T, key *rsa.PrivateKey, kid string, algorithm jose.SignatureAlgorithm, claims accessClaims) string {
	t.Helper()
	signer, err := jose.NewSigner(jose.SigningKey{Algorithm: algorithm, Key: key}, new(jose.SignerOptions).WithHeader("kid", kid))
	if err != nil {
		t.Fatal(err)
	}
	token, err := jwt.Signed(signer).Claims(claims).Serialize()
	if err != nil {
		t.Fatal(err)
	}
	return token
}
