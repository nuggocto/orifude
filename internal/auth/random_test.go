package auth

import (
	"bytes"
	"crypto/sha256"
	"errors"
	"testing"
)

func TestRandomValuesUseInjectedReaderAndFixedLengths(t *testing.T) {
	source := make([]byte, publicIDBytes+secretBytes)
	for i := range source {
		source[i] = byte(i)
	}
	challenge, err := NewChallenge(bytes.NewReader(source))
	if err != nil {
		t.Fatal(err)
	}
	if len(challenge.ID) != 22 || len(challenge.Nonce) != 43 {
		t.Fatalf("id length = %d, nonce length = %d", len(challenge.ID), len(challenge.Nonce))
	}
	if challenge.NonceHash != sha256.Sum256([]byte(challenge.Nonce)) {
		t.Fatal("nonce hash does not hash the encoded nonce")
	}

	token, tokenHash, err := NewAccessToken(bytes.NewReader(bytes.Repeat([]byte{0xa5}, secretBytes)))
	if err != nil {
		t.Fatal(err)
	}
	if len(token) != 43 || tokenHash != sha256.Sum256([]byte(token)) {
		t.Fatal("access token contract mismatch")
	}
	if HashRevocationCredential("revocation") != sha256.Sum256([]byte("revocation")) {
		t.Fatal("revocation credential hash mismatch")
	}

	id, err := NewPublicID(bytes.NewReader(bytes.Repeat([]byte{0x5a}, publicIDBytes)))
	if err != nil {
		t.Fatal(err)
	}
	if len(id) != 22 {
		t.Fatalf("public ID length = %d, want 22", len(id))
	}
}

func TestRandomValuesRejectShortOrMissingReader(t *testing.T) {
	if _, err := NewPublicID(bytes.NewReader(make([]byte, publicIDBytes-1))); !errors.Is(err, ErrRandomSource) {
		t.Fatalf("short reader error = %v", err)
	}
	if _, _, err := NewAccessToken(nil); !errors.Is(err, ErrRandomSource) {
		t.Fatalf("nil reader error = %v", err)
	}
}
