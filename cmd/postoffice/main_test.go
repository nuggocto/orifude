package main

import (
	"context"
	"io"
	"log/slog"
	"net"
	"net/http"
	"testing"
	"time"
)

func TestServerStartsAndShutsDown(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(t.Context())
	done := make(chan error, 1)
	go func() {
		done <- serve(ctx, listener, http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			w.WriteHeader(http.StatusNoContent)
		}), slog.New(slog.NewTextHandler(io.Discard, nil)))
	}()

	response, err := http.Get("http://" + listener.Addr().String())
	if err != nil {
		cancel()
		t.Fatal(err)
	}
	response.Body.Close()
	if response.StatusCode != http.StatusNoContent {
		t.Fatalf("status = %d, want 204", response.StatusCode)
	}
	cancel()
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("server did not shut down")
	}
}

func TestLoadConfigRejectsInsecureExternalOrigins(t *testing.T) {
	values := map[string]string{
		"DATABASE_URL": "postgres://localhost/orifude", "LISTEN_ADDR": "127.0.0.1:8080",
		"PUBLIC_ORIGIN": "https://api.example", "MODERATION_ORIGIN": "https://moderation.example",
		"AWS_REGION":           "us-east-1",
		"MESSAGE_KMS_KEY_ARN":  "arn:aws:kms:us-east-1:123456789012:key/11111111-1111-1111-1111-111111111111",
		"EVIDENCE_KMS_KEY_ARN": "arn:aws:kms:us-east-1:123456789012:key/22222222-2222-2222-2222-222222222222",
		"AWS_ACCESS_KEY_ID":    "test", "AWS_SECRET_ACCESS_KEY": "test",
		"CF_ACCESS_ISSUER": "https://team.cloudflareaccess.com", "CF_ACCESS_AUDIENCE": "audience",
		"LATEST_TUI_VERSION": "v0.2.0", "LOG_LEVEL": "info",
	}
	for name, value := range values {
		t.Setenv(name, value)
	}
	if _, err := loadConfig(); err != nil {
		t.Fatalf("valid configuration: %v", err)
	}
	t.Setenv("PUBLIC_ORIGIN", "http://api.example")
	if _, err := loadConfig(); err == nil {
		t.Fatal("accepted insecure external origin")
	}
	t.Setenv("PUBLIC_ORIGIN", "http://127.0.0.1:8080")
	if _, err := loadConfig(); err != nil {
		t.Fatalf("loopback development origin: %v", err)
	}
}

func TestRunRejectsUnknownCommand(t *testing.T) {
	if err := run(t.Context(), []string{"unknown"}); err == nil {
		t.Fatal("accepted unknown command")
	}
}
