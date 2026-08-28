package httpapi

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"net/netip"
	"strings"
	"testing"
	"time"

	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/postoffice"
)

type readyFunc func(context.Context) error

func (fn readyFunc) Ready(ctx context.Context) error { return fn(ctx) }

func TestRouterRegistersExactRoutes(t *testing.T) {
	router := testRouter(t, readyFunc(func(context.Context) error { return nil }), Config{})
	tests := []struct {
		method string
		path   string
		status int
	}{
		{http.MethodPost, "/v1/auth/challenges", http.StatusUnsupportedMediaType},
		{http.MethodPost, "/v1/identities", http.StatusUnauthorized},
		{http.MethodPost, "/v1/sessions", http.StatusUnauthorized},
		{http.MethodPost, "/v1/identities/revoke", http.StatusUnsupportedMediaType},
		{http.MethodGet, "/v1/me", http.StatusUnauthorized},
		{http.MethodDelete, "/v1/me", http.StatusUnauthorized},
		{http.MethodPost, "/v1/letters", http.StatusUnauthorized},
		{http.MethodPost, "/v1/letters/claim", http.StatusUnauthorized},
		{http.MethodGet, "/v1/letters/id", http.StatusUnauthorized},
		{http.MethodPost, "/v1/letters/id/open", http.StatusUnauthorized},
		{http.MethodPost, "/v1/letters/id/reply", http.StatusUnauthorized},
		{http.MethodPost, "/v1/letters/id/withdraw", http.StatusUnauthorized},
		{http.MethodPost, "/v1/letters/id/report", http.StatusUnauthorized},
		{http.MethodPost, "/v1/letters/id/block", http.StatusUnauthorized},
		{http.MethodGet, "/v1/keepsakes", http.StatusUnauthorized},
		{http.MethodDelete, "/v1/keepsakes/id", http.StatusUnauthorized},
		{http.MethodGet, "/healthz", http.StatusOK},
		{http.MethodGet, "/readyz", http.StatusOK},
		{http.MethodPost, "/moderation/v1/reports/next/claim", http.StatusForbidden},
		{http.MethodPost, "/moderation/v1/reports/id/review", http.StatusForbidden},
		{http.MethodPost, "/moderation/v1/reports/id/close", http.StatusForbidden},
	}
	for _, test := range tests {
		t.Run(test.method+" "+test.path, func(t *testing.T) {
			request := httptest.NewRequest(test.method, test.path, nil)
			response := httptest.NewRecorder()
			router.ServeHTTP(response, request)
			if response.Code != test.status {
				t.Fatalf("status = %d, want %d; body %s", response.Code, test.status, response.Body.String())
			}
		})
	}
	request := httptest.NewRequest(http.MethodGet, "/v1/letters/id/", nil)
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)
	if response.Code != http.StatusNotFound || response.Header().Get("Location") != "" {
		t.Fatalf("trailing slash response = %d Location %q", response.Code, response.Header().Get("Location"))
	}
}

func TestRouterRejectsInsecureExternalModerationOrigin(t *testing.T) {
	certs := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(w).Encode(map[string]any{"keys": []any{}})
	}))
	t.Cleanup(certs.Close)
	access, err := NewAccessVerifier(certs.URL, "audience")
	if err != nil {
		t.Fatal(err)
	}
	_, err = New(new(postoffice.Service), readyFunc(func(context.Context) error { return nil }), access, Config{ModerationOrigin: "http://moderation.example"})
	if err == nil {
		t.Fatal("router accepted insecure external moderation origin")
	}
}

func TestRouterRejectsInvalidJSONBeforeService(t *testing.T) {
	router := testRouter(t, readyFunc(func(context.Context) error { return nil }), Config{})
	tests := []struct {
		name        string
		contentType string
		body        string
		status      int
	}{
		{name: "content type", contentType: "text/plain", body: `{}`, status: http.StatusUnsupportedMediaType},
		{name: "unknown field", contentType: "application/json", body: `{"purpose":"registration","public_jwk":{},"extra":true}`, status: http.StatusBadRequest},
		{name: "duplicate field", contentType: "application/json", body: `{"purpose":"registration","purpose":"session","public_jwk":{}}`, status: http.StatusBadRequest},
		{name: "wrong field case", contentType: "application/json", body: `{"Purpose":"registration","public_jwk":{}}`, status: http.StatusBadRequest},
		{name: "case duplicate", contentType: "application/json", body: `{"purpose":"registration","Purpose":"session","public_jwk":{}}`, status: http.StatusBadRequest},
		{name: "null object", contentType: "application/json", body: `null`, status: http.StatusBadRequest},
		{name: "trailing value", contentType: "application/json", body: `{} {}`, status: http.StatusBadRequest},
		{name: "body limit", contentType: "application/json", body: `{"padding":"` + strings.Repeat("x", smallRequestBytes) + `"}`, status: http.StatusRequestEntityTooLarge},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			request := httptest.NewRequest(http.MethodPost, "/v1/auth/challenges", strings.NewReader(test.body))
			request.Header.Set("Content-Type", test.contentType)
			response := httptest.NewRecorder()
			router.ServeHTTP(response, request)
			if response.Code != test.status {
				t.Fatalf("status = %d, want %d; body %s", response.Code, test.status, response.Body.String())
			}
		})
	}
}

func TestRouterRejectsDuplicateCredentialHeaders(t *testing.T) {
	router := testRouter(t, readyFunc(func(context.Context) error { return nil }), Config{})
	for _, header := range []string{"Authorization", "DPoP", "Cf-Access-Jwt-Assertion"} {
		t.Run(header, func(t *testing.T) {
			request := httptest.NewRequest(http.MethodGet, "/healthz", nil)
			request.Header.Add(header, "first")
			request.Header.Add(header, "second")
			response := httptest.NewRecorder()
			router.ServeHTTP(response, request)
			if response.Code != http.StatusBadRequest {
				t.Fatalf("status = %d, want 400", response.Code)
			}
		})
	}
}

func TestStableRegistrationAndSessionErrorCodes(t *testing.T) {
	tests := []struct {
		err  error
		code api.ErrorCode
	}{
		{postoffice.ErrSessionExpired, api.ErrorCodeSessionExpired},
		{postoffice.ErrAliasInvalid, api.ErrorCodeAliasInvalid},
		{postoffice.ErrInviteInvalid, api.ErrorCodeInviteInvalid},
		{postoffice.ErrIdentityConflict, api.ErrorCodeIdentityConflict},
	}
	for _, test := range tests {
		response := httptest.NewRecorder()
		respondError(response, test.err)
		var body api.ErrorResponse
		if err := json.Unmarshal(response.Body.Bytes(), &body); err != nil {
			t.Fatal(err)
		}
		if body.Error.Code != test.code {
			t.Fatalf("%v mapped to %q, want %q", test.err, body.Error.Code, test.code)
		}
	}
}

func TestRouterHeadersLogsAndTimeout(t *testing.T) {
	var logs bytes.Buffer
	logger := slog.New(slog.NewJSONHandler(&logs, nil))
	ready := readyFunc(func(ctx context.Context) error {
		<-ctx.Done()
		return ctx.Err()
	})
	router := testRouter(t, ready, Config{Logger: logger, DatabaseTimeout: time.Millisecond})
	request := httptest.NewRequest(http.MethodGet, "/readyz?secret=must-not-log", nil)
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)
	if response.Code != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want 503; body %s", response.Code, response.Body.String())
	}
	if response.Header().Get("Content-Type") != "application/json" || response.Header().Get("X-Request-ID") == "" || response.Header().Get("X-Content-Type-Options") != "nosniff" {
		t.Fatalf("response headers = %v", response.Header())
	}
	if strings.Contains(logs.String(), "must-not-log") || !strings.Contains(logs.String(), `"route":"/readyz"`) || !strings.Contains(logs.String(), `"status":503`) {
		t.Fatalf("access log = %s", logs.String())
	}

	request = httptest.NewRequest(http.MethodGet, "/v1/me", nil)
	response = httptest.NewRecorder()
	router.ServeHTTP(response, request)
	if response.Header().Get("Cache-Control") != "no-store" || response.Header().Get("Access-Control-Allow-Origin") != "" {
		t.Fatalf("participant security headers = %v", response.Header())
	}
}

func TestModerationRejectsPreflightAndOriginMismatch(t *testing.T) {
	router := testRouter(t, readyFunc(func(context.Context) error { return nil }), Config{})
	request := httptest.NewRequest(http.MethodOptions, "/moderation/v1/reports/next/claim", nil)
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)
	if response.Code != http.StatusMethodNotAllowed || response.Header().Get("Access-Control-Allow-Origin") != "" {
		t.Fatalf("preflight response = %d, headers %v", response.Code, response.Header())
	}

	request = httptest.NewRequest(http.MethodPost, "http://wrong.test/moderation/v1/reports/next/claim", strings.NewReader(`{}`))
	request.Header.Set("Content-Type", "application/json")
	request.Header.Set("X-Orifude-Moderation", string(api.ModerationPurposeReportedContentReview))
	response = httptest.NewRecorder()
	router.ServeHTTP(response, request)
	if response.Code != http.StatusForbidden {
		t.Fatalf("origin mismatch status = %d, want 403", response.Code)
	}
}

func TestModerationTrustsForwardingOnlyFromConfiguredProxy(t *testing.T) {
	trusted := netip.MustParsePrefix("192.0.2.0/24")
	router := testRouter(t, readyFunc(func(context.Context) error { return nil }), Config{TrustedProxies: []netip.Prefix{trusted}})
	request := httptest.NewRequest(http.MethodPost, "http://internal/moderation/v1/reports/next/claim", strings.NewReader(`{}`))
	request.RemoteAddr = "192.0.2.10:1234"
	request.Header.Set("Content-Type", "application/json")
	request.Header.Set("X-Forwarded-Proto", "https")
	request.Header.Set("X-Forwarded-Host", "moderation.test")
	request.Header.Set("X-Orifude-Moderation", string(api.ModerationPurposeReportedContentReview))
	request.Header.Set("Cf-Access-Jwt-Assertion", "invalid")
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)
	if response.Code != http.StatusUnauthorized {
		t.Fatalf("trusted proxy status = %d, want 401", response.Code)
	}

	request.RemoteAddr = "198.51.100.10:1234"
	response = httptest.NewRecorder()
	router.ServeHTTP(response, request)
	if response.Code != http.StatusForbidden {
		t.Fatalf("untrusted proxy status = %d, want 403", response.Code)
	}
}

func testRouter(t *testing.T, ready Ready, config Config) http.Handler {
	t.Helper()
	certs := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(w).Encode(map[string]any{"keys": []any{}})
	}))
	t.Cleanup(certs.Close)
	access, err := NewAccessVerifier(certs.URL, "audience")
	if err != nil {
		t.Fatal(err)
	}
	config.ModerationOrigin = "https://moderation.test"
	if config.Logger == nil {
		config.Logger = slog.New(slog.NewTextHandler(io.Discard, nil))
	}
	router, err := New(new(postoffice.Service), ready, access, config)
	if err != nil {
		t.Fatal(err)
	}
	return router
}
