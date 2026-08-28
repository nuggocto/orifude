//go:build integration

package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestRunPassesCanariesAndShutsDown(t *testing.T) {
	if os.Getenv("TEST_DATABASE_URL") == "" {
		t.Skip("TEST_DATABASE_URL is set by testdata/postgres/check.sh")
	}
	fake := &kmsHTTPServer{keys: make(map[string][]byte)}
	kmsServer := httptest.NewServer(fake)
	t.Cleanup(kmsServer.Close)

	probe, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	listenAddr := probe.Addr().String()
	probe.Close()
	origin := "http://" + listenAddr
	values := map[string]string{
		"DATABASE_URL": os.Getenv("TEST_DATABASE_URL"), "LISTEN_ADDR": listenAddr,
		"PUBLIC_ORIGIN": origin, "MODERATION_ORIGIN": origin, "AWS_REGION": "us-east-1",
		"MESSAGE_KMS_KEY_ARN":  "arn:aws:kms:us-east-1:123456789012:key/11111111-1111-1111-1111-111111111111",
		"EVIDENCE_KMS_KEY_ARN": "arn:aws:kms:us-east-1:123456789012:key/22222222-2222-2222-2222-222222222222",
		"AWS_ACCESS_KEY_ID":    "test", "AWS_SECRET_ACCESS_KEY": "test", "AWS_ENDPOINT_URL_KMS": kmsServer.URL,
		"AWS_EC2_METADATA_DISABLED": "true", "CF_ACCESS_ISSUER": "https://team.cloudflareaccess.com",
		"CF_ACCESS_AUDIENCE": "audience", "LATEST_TUI_VERSION": "v0.2.0-test", "LOG_LEVEL": "error",
	}
	for name, value := range values {
		t.Setenv(name, value)
	}

	ctx, cancel := context.WithCancel(t.Context())
	done := make(chan error, 1)
	go func() { done <- run(ctx, nil) }()
	client := &http.Client{Timeout: 200 * time.Millisecond}
	readyDeadline := time.NewTimer(10 * time.Second)
	ticker := time.NewTicker(10 * time.Millisecond)
	defer readyDeadline.Stop()
	defer ticker.Stop()
	for {
		select {
		case err := <-done:
			t.Fatalf("post office stopped before readiness: %v", err)
		case <-readyDeadline.C:
			cancel()
			t.Fatal("post office did not become ready")
		case <-ticker.C:
			response, err := client.Get(origin + "/readyz")
			if err != nil {
				continue
			}
			response.Body.Close()
			if response.StatusCode == http.StatusOK {
				cancel()
				goto shutdown
			}
		}
	}

shutdown:
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("post office did not shut down")
	}
	if fake.generates.Load() != 2 || fake.decrypts.Load() != 1 {
		t.Fatalf("startup KMS calls = %d generate, %d decrypt", fake.generates.Load(), fake.decrypts.Load())
	}
	if err := run(t.Context(), []string{"cleanup"}); err != nil {
		t.Fatalf("cleanup command: %v", err)
	}
	if fake.generates.Load() != 4 || fake.decrypts.Load() != 2 {
		t.Fatalf("server and cleanup KMS calls = %d generate, %d decrypt", fake.generates.Load(), fake.decrypts.Load())
	}
}

type kmsHTTPServer struct {
	mu        sync.Mutex
	keys      map[string][]byte
	generates atomic.Int32
	decrypts  atomic.Int32
}

func (s *kmsHTTPServer) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/x-amz-json-1.1")
	switch r.Header.Get("X-Amz-Target") {
	case "TrentService.GenerateDataKey":
		var request struct {
			KeyID string `json:"KeyId"`
		}
		if json.NewDecoder(r.Body).Decode(&request) != nil || request.KeyID == "" {
			http.Error(w, "invalid", http.StatusBadRequest)
			return
		}
		count := byte(s.generates.Add(1))
		plaintext := bytes.Repeat([]byte{count}, 32)
		wrapped := bytes.Repeat([]byte{count + 10}, 32)
		s.mu.Lock()
		s.keys[string(wrapped)] = plaintext
		s.mu.Unlock()
		_ = json.NewEncoder(w).Encode(map[string]any{"KeyId": request.KeyID, "Plaintext": plaintext, "CiphertextBlob": wrapped})
	case "TrentService.Decrypt":
		var request struct {
			KeyID      string `json:"KeyId"`
			Ciphertext []byte `json:"CiphertextBlob"`
		}
		if json.NewDecoder(r.Body).Decode(&request) != nil || request.KeyID == "" {
			http.Error(w, "invalid", http.StatusBadRequest)
			return
		}
		s.mu.Lock()
		plaintext := append([]byte(nil), s.keys[string(request.Ciphertext)]...)
		s.mu.Unlock()
		if len(plaintext) == 0 {
			http.Error(w, "unknown", http.StatusBadRequest)
			return
		}
		s.decrypts.Add(1)
		_ = json.NewEncoder(w).Encode(map[string]any{"KeyId": request.KeyID, "Plaintext": plaintext})
	default:
		http.Error(w, errors.New("unknown KMS operation").Error(), http.StatusBadRequest)
	}
}
