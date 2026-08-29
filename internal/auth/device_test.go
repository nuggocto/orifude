package auth

import (
	"crypto/rand"
	"errors"
	"testing"
	"time"
)

func TestDeviceKeyRoundTripAndProofs(t *testing.T) {
	device, err := GenerateDeviceKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	encoded, err := device.MarshalPKCS8()
	if err != nil {
		t.Fatal(err)
	}
	loaded, err := ParseDeviceKey(encoded)
	if err != nil {
		t.Fatal(err)
	}
	if loaded.Thumbprint() != device.Thumbprint() {
		t.Fatal("loaded device key has a different thumbprint")
	}

	now := time.Unix(1_800_000_000, 0)
	prover, err := NewProver(loaded, "https://api.orifude.com", rand.Reader, func() time.Time { return now })
	if err != nil {
		t.Fatal(err)
	}
	verifier, err := NewVerifier("https://api.orifude.com")
	if err != nil {
		t.Fatal(err)
	}
	nonce, err := GenerateRevocationCredential(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	challengeProof, err := prover.ChallengeProof("POST", "/v1/sessions", nonce)
	if err != nil {
		t.Fatal(err)
	}
	verified, err := verifier.VerifyChallenge(ChallengeProofParams{
		Proof: challengeProof, Method: "POST", EscapedPath: "/v1/sessions",
		NonceHash: HashOpaque(nonce), KeyThumbprint: device.Thumbprint(), Now: now,
	})
	if err != nil || verified.JTI == "" {
		t.Fatalf("verify challenge proof: %+v, %v", verified, err)
	}

	token, _, err := NewAccessToken(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	first, err := prover.ResourceProof("GET", "/v1/me", token)
	if err != nil {
		t.Fatal(err)
	}
	second, err := prover.ResourceProof("GET", "/v1/me", token)
	if err != nil {
		t.Fatal(err)
	}
	if first == second {
		t.Fatal("two resource attempts reused a DPoP proof")
	}
	if _, err := verifier.VerifyResource(ResourceProofParams{
		Proof: first, Method: "GET", EscapedPath: "/v1/me", AccessToken: token,
		KeyThumbprint: device.Thumbprint(), Now: now,
	}); err != nil {
		t.Fatalf("verify resource proof: %v", err)
	}
}

func TestProverRejectsUnsafeOriginsAndBindings(t *testing.T) {
	device, err := GenerateDeviceKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	for _, origin := range []string{"http://example.com", "https://user@example.com", "https://example.com/path"} {
		if _, err := NewProver(device, origin, rand.Reader, time.Now); !errors.Is(err, ErrInvalidOrigin) {
			t.Fatalf("NewProver(%q) error = %v", origin, err)
		}
	}
	prover, err := NewProver(device, "https://api.orifude.com", rand.Reader, time.Now)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := prover.ChallengeProof("POST", "/v1/sessions", "not-a-secret"); !errors.Is(err, ErrInvalidProof) {
		t.Fatalf("invalid nonce error = %v", err)
	}
}
