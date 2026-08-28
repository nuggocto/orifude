//go:build testtool

// Command tuitestpostoffice runs the production HTTP handler with synthetic
// encryption for local terminal recordings. It is excluded from release builds.
package main

import (
	"context"
	"crypto/rand"
	"crypto/rsa"
	"crypto/sha256"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/signal"
	"sync"
	"syscall"
	"time"

	"github.com/aws/aws-sdk-go-v2/service/kms"
	"github.com/aws/aws-sdk-go-v2/service/kms/types"
	"github.com/go-jose/go-jose/v4"
	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/database"
	"github.com/nuggocto/orifude/internal/envelope"
	"github.com/nuggocto/orifude/internal/httpapi"
	"github.com/nuggocto/orifude/internal/postoffice"
)

const (
	messageKeyARN  = "arn:aws:kms:us-east-1:123456789012:key/11111111-1111-1111-1111-111111111111"
	evidenceKeyARN = "arn:aws:kms:us-east-1:123456789012:key/22222222-2222-2222-2222-222222222222"
	testInvite     = "R0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0c"
)

func main() {
	databaseURL := os.Getenv("DATABASE_URL")
	listenAddress := os.Getenv("LISTEN_ADDR")
	publicOrigin := os.Getenv("PUBLIC_ORIGIN")
	if databaseURL == "" || listenAddress == "" || publicOrigin == "" {
		log.Fatal("DATABASE_URL, LISTEN_ADDR, and PUBLIC_ORIGIN are required")
	}
	ctx, cancel := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer cancel()
	db, err := database.Open(ctx, databaseURL, 8)
	if err != nil {
		log.Fatal(err)
	}
	defer db.Close()
	inviteHash := auth.HashOpaque(testInvite)
	if _, err := db.Queries().CreateInvite(ctx, inviteHash[:]); err != nil {
		log.Fatal(err)
	}
	verifier, err := auth.NewVerifier(publicOrigin)
	if err != nil {
		log.Fatal(err)
	}
	cipher, err := envelope.New(newSyntheticKMS(), rand.Reader, messageKeyARN, evidenceKeyARN)
	if err != nil {
		log.Fatal(err)
	}
	config := postoffice.DefaultConfig()
	config.LatestTUIVersion = "v0.3.1-test"
	service, err := postoffice.New(db, verifier, cipher, config)
	if err != nil {
		log.Fatal(err)
	}
	access, closeAccess := testAccessVerifier()
	defer closeAccess()
	handler, err := httpapi.New(service, db, access, httpapi.Config{
		Logger: slog.New(slog.NewTextHandler(io.Discard, nil)), ModerationOrigin: publicOrigin,
	})
	if err != nil {
		log.Fatal(err)
	}
	server := &http.Server{Addr: listenAddress, Handler: handler, ReadHeaderTimeout: 5 * time.Second}
	go func() {
		<-ctx.Done()
		shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer shutdownCancel()
		_ = server.Shutdown(shutdownCtx)
	}()
	fmt.Println("ready")
	if err := server.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Fatal(err)
	}
}

func testAccessVerifier() (*httpapi.AccessVerifier, func()) {
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		log.Fatal(err)
	}
	server := &http.Server{Addr: "127.0.0.1:0"}
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		log.Fatal(err)
	}
	server.Handler = http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(writer).Encode(jose.JSONWebKeySet{Keys: []jose.JSONWebKey{{
			Key: &key.PublicKey, KeyID: "test", Algorithm: string(jose.RS256), Use: "sig",
		}}})
	})
	go func() { _ = server.Serve(listener) }()
	verifier, err := httpapi.NewAccessVerifier("http://"+listener.Addr().String(), "test-audience")
	if err != nil {
		log.Fatal(err)
	}
	return verifier, func() { _ = server.Close() }
}

type syntheticKMS struct {
	mu      sync.Mutex
	counter uint64
	keys    map[string][]byte
}

func newSyntheticKMS() *syntheticKMS { return &syntheticKMS{keys: make(map[string][]byte)} }

func (k *syntheticKMS) GenerateDataKey(_ context.Context, input *kms.GenerateDataKeyInput, _ ...func(*kms.Options)) (*kms.GenerateDataKeyOutput, error) {
	if input.KeyId == nil || input.KeySpec != types.DataKeySpecAes256 {
		return nil, errors.New("invalid synthetic data-key request")
	}
	k.mu.Lock()
	defer k.mu.Unlock()
	k.counter++
	key := sha256.Sum256([]byte(fmt.Sprintf("%s:%d", *input.KeyId, k.counter)))
	wrapper := sha256.Sum256(append(key[:], byte(k.counter)))
	k.keys[string(wrapper[:])] = append([]byte(nil), key[:]...)
	return &kms.GenerateDataKeyOutput{KeyId: input.KeyId, Plaintext: key[:], CiphertextBlob: wrapper[:]}, nil
}

func (k *syntheticKMS) Decrypt(_ context.Context, input *kms.DecryptInput, _ ...func(*kms.Options)) (*kms.DecryptOutput, error) {
	if input.KeyId == nil {
		return nil, errors.New("invalid synthetic decrypt request")
	}
	k.mu.Lock()
	defer k.mu.Unlock()
	key := k.keys[string(input.CiphertextBlob)]
	if key == nil {
		return nil, errors.New("unknown synthetic wrapped key")
	}
	return &kms.DecryptOutput{KeyId: input.KeyId, Plaintext: append([]byte(nil), key...)}, nil
}
