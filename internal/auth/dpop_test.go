package auth

import (
	"bytes"
	"crypto/ecdsa"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/go-jose/go-jose/v4"
)

var proofTime = time.Unix(1_800_000_000, 0)

func TestVerifyChallengeAndResourceProofs(t *testing.T) {
	key := testPrivateKey(t)
	thumbprint := testThumbprint(t, key)
	verifier, err := NewVerifier("https://api.orifude.com")
	if err != nil {
		t.Fatal(err)
	}

	nonce := opaqueValue(1)
	challengeClaims := validClaims("POST", "https://api.orifude.com/v1/identities", strings.Repeat("c", 16), proofTime)
	challengeClaims["nonce"] = nonce
	challengeProof := signProof(t, key, challengeClaims)
	first, err := verifier.VerifyChallenge(ChallengeProofParams{
		Proof: challengeProof, Method: "POST", EscapedPath: "/v1/identities", NonceHash: HashOpaque(nonce), KeyThumbprint: thumbprint, Now: proofTime,
	})
	if err != nil {
		t.Fatal(err)
	}
	second, err := verifier.VerifyChallenge(ChallengeProofParams{
		Proof: challengeProof, Method: "POST", EscapedPath: "/v1/identities", NonceHash: HashOpaque(nonce), KeyThumbprint: thumbprint, Now: proofTime,
	})
	if err != nil {
		t.Fatalf("replay-independent validation rejected a repeated proof: %v", err)
	}
	if first.JTIHash != second.JTIHash || first.JTIHash != sha256.Sum256([]byte(first.JTI)) {
		t.Fatal("JTI hash is not stable for persistence replay checks")
	}

	token := opaqueValue(2)
	resourceClaims := validClaims("GET", "https://api.orifude.com/v1/letters/a%2Fb", strings.Repeat("r", 16), proofTime)
	resourceClaims["ath"] = EncodeHash(HashOpaque(token))
	resourceProof := signProof(t, key, resourceClaims)
	if _, err := verifier.VerifyResource(ResourceProofParams{
		Proof: resourceProof, Method: "GET", EscapedPath: "/v1/letters/a%2Fb", AccessToken: token, KeyThumbprint: thumbprint, Now: proofTime,
	}); err != nil {
		t.Fatal(err)
	}
}

func TestVerifyProofRejectsUnsafeJOSEHeaders(t *testing.T) {
	key := testPrivateKey(t)
	thumbprint := testThumbprint(t, key)
	verifier, err := NewVerifier("https://api.orifude.com")
	if err != nil {
		t.Fatal(err)
	}
	claims := validClaims("POST", "https://api.orifude.com/v1/sessions", strings.Repeat("j", 16), proofTime)
	nonce := opaqueValue(3)
	claims["nonce"] = nonce
	valid := signProof(t, key, claims)
	jwk := dpopJWK(t, key)
	tests := []struct {
		name   string
		header string
	}{
		{"algorithm confusion", `{"typ":"dpop+jwt","alg":"HS256","jwk":` + jwk + `}`},
		{"remote jku", `{"typ":"dpop+jwt","alg":"ES256","jwk":` + jwk + `,"jku":"https://example.com/key"}`},
		{"remote x5u", `{"typ":"dpop+jwt","alg":"ES256","jwk":` + jwk + `,"x5u":"https://example.com/cert"}`},
		{"remote x5c", `{"typ":"dpop+jwt","alg":"ES256","jwk":` + jwk + `,"x5c":["certificate"]}`},
		{"private jwk", `{"typ":"dpop+jwt","alg":"ES256","jwk":` + strings.TrimSuffix(jwk, "}") + `,"d":"secret"}}`},
		{"duplicate header", `{"typ":"dpop+jwt","typ":"dpop+jwt","alg":"ES256","jwk":` + jwk + `}`},
		{"duplicate jwk", `{"typ":"dpop+jwt","alg":"ES256","jwk":` + strings.Replace(jwk, `"x":`, `"x":"duplicate","x":`, 1) + `}`},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			proof := replaceProtectedHeader(t, valid, test.header)
			_, err := verifier.VerifyChallenge(ChallengeProofParams{
				Proof: proof, Method: "POST", EscapedPath: "/v1/sessions", NonceHash: HashOpaque(nonce), KeyThumbprint: thumbprint, Now: proofTime,
			})
			if !errors.Is(err, ErrInvalidProof) {
				t.Fatalf("error = %v, want ErrInvalidProof", err)
			}
		})
	}
}

func TestVerifyChallengeProofClaimBoundaries(t *testing.T) {
	key := testPrivateKey(t)
	thumbprint := testThumbprint(t, key)
	verifier, err := NewVerifier("https://api.orifude.com")
	if err != nil {
		t.Fatal(err)
	}
	nonce := opaqueValue(4)
	tests := []struct {
		name       string
		change     func(map[string]any)
		path       string
		nonceHash  [sha256.Size]byte
		thumbprint [sha256.Size]byte
		wantOK     bool
	}{
		{"minimum jti", func(c map[string]any) { c["jti"] = strings.Repeat("a", MinJTIBytes) }, "/v1/sessions", HashOpaque(nonce), thumbprint, true},
		{"maximum jti", func(c map[string]any) { c["jti"] = strings.Repeat("a", MaxJTIBytes) }, "/v1/sessions", HashOpaque(nonce), thumbprint, true},
		{"short jti", func(c map[string]any) { c["jti"] = strings.Repeat("a", MinJTIBytes-1) }, "/v1/sessions", HashOpaque(nonce), thumbprint, false},
		{"long jti", func(c map[string]any) { c["jti"] = strings.Repeat("a", MaxJTIBytes+1) }, "/v1/sessions", HashOpaque(nonce), thumbprint, false},
		{"control jti", func(c map[string]any) { c["jti"] = strings.Repeat("a", 15) + "\n" }, "/v1/sessions", HashOpaque(nonce), thumbprint, false},
		{"past skew boundary", func(c map[string]any) { c["iat"] = proofTime.Add(-MaxClockSkew).Unix() }, "/v1/sessions", HashOpaque(nonce), thumbprint, true},
		{"future skew boundary", func(c map[string]any) { c["iat"] = proofTime.Add(MaxClockSkew).Unix() }, "/v1/sessions", HashOpaque(nonce), thumbprint, true},
		{"too old", func(c map[string]any) { c["iat"] = proofTime.Add(-MaxClockSkew - time.Second).Unix() }, "/v1/sessions", HashOpaque(nonce), thumbprint, false},
		{"future", func(c map[string]any) { c["iat"] = proofTime.Add(MaxClockSkew + time.Second).Unix() }, "/v1/sessions", HashOpaque(nonce), thumbprint, false},
		{"wrong method", func(c map[string]any) { c["htm"] = "GET" }, "/v1/sessions", HashOpaque(nonce), thumbprint, false},
		{"query in htu", func(c map[string]any) { c["htu"] = "https://api.orifude.com/v1/sessions?x=1" }, "/v1/sessions", HashOpaque(nonce), thumbprint, false},
		{"escaped path differs", func(c map[string]any) { c["htu"] = "https://api.orifude.com/v1/a/b" }, "/v1/a%2Fb", HashOpaque(nonce), thumbprint, false},
		{"wrong nonce", func(c map[string]any) {}, "/v1/sessions", HashOpaque(opaqueValue(5)), thumbprint, false},
		{"malformed nonce", func(c map[string]any) { c["nonce"] = "short" }, "/v1/sessions", HashOpaque("short"), thumbprint, false},
		{"wrong thumbprint", func(c map[string]any) {}, "/v1/sessions", HashOpaque(nonce), [sha256.Size]byte{}, false},
		{"fractional iat", func(c map[string]any) { c["iat"] = float64(proofTime.Unix()) + 0.5 }, "/v1/sessions", HashOpaque(nonce), thumbprint, false},
		{"unknown claim", func(c map[string]any) { c["extra"] = true }, "/v1/sessions", HashOpaque(nonce), thumbprint, false},
		{"ath on challenge", func(c map[string]any) { delete(c, "nonce"); c["ath"] = "value" }, "/v1/sessions", HashOpaque(nonce), thumbprint, false},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			claims := validClaims("POST", "https://api.orifude.com"+test.path, strings.Repeat("j", 16), proofTime)
			claims["nonce"] = nonce
			test.change(claims)
			proof := signProof(t, key, claims)
			_, err := verifier.VerifyChallenge(ChallengeProofParams{
				Proof: proof, Method: "POST", EscapedPath: test.path, NonceHash: test.nonceHash, KeyThumbprint: test.thumbprint, Now: proofTime,
			})
			if (err == nil) != test.wantOK {
				t.Fatalf("error = %v, want success %t", err, test.wantOK)
			}
		})
	}
}

func TestVerifyProofRejectsProtocolBounds(t *testing.T) {
	key := testPrivateKey(t)
	thumbprint := testThumbprint(t, key)
	verifier, err := NewVerifier("https://api.orifude.com")
	if err != nil {
		t.Fatal(err)
	}
	nonce := opaqueValue(8)
	claims := validClaims("POST", "https://api.orifude.com/v1/sessions", strings.Repeat("j", 16), proofTime)
	claims["nonce"] = nonce
	proof := signProof(t, key, claims)
	params := ChallengeProofParams{
		Proof: proof, Method: "POST", EscapedPath: "/v1/sessions", NonceHash: HashOpaque(nonce), KeyThumbprint: thumbprint, Now: proofTime,
	}

	params.Method = "post"
	if _, err := verifier.VerifyChallenge(params); !errors.Is(err, ErrInvalidProof) {
		t.Fatalf("lowercase method error = %v", err)
	}
	params.Method = "POST"
	params.Proof = proof + strings.Repeat("x", MaxProofBytes)
	if _, err := verifier.VerifyChallenge(params); !errors.Is(err, ErrInvalidProof) {
		t.Fatalf("oversized proof error = %v", err)
	}
}

func TestVerifyProofClassifiesClockSkew(t *testing.T) {
	key := testPrivateKey(t)
	thumbprint := testThumbprint(t, key)
	verifier, err := NewVerifier("https://api.orifude.com")
	if err != nil {
		t.Fatal(err)
	}
	nonce := opaqueValue(9)
	claims := validClaims("POST", "https://api.orifude.com/v1/sessions", strings.Repeat("j", 16), proofTime.Add(MaxClockSkew+time.Second))
	claims["nonce"] = nonce
	_, err = verifier.VerifyChallenge(ChallengeProofParams{
		Proof: signProof(t, key, claims), Method: "POST", EscapedPath: "/v1/sessions", NonceHash: HashOpaque(nonce), KeyThumbprint: thumbprint, Now: proofTime,
	})
	if !errors.Is(err, ErrInvalidProof) || !errors.Is(err, ErrProofClockSkew) {
		t.Fatalf("error = %v, want invalid proof and clock skew", err)
	}
}

func TestVerifyResourceProofRequiresATHAndOmitsNonce(t *testing.T) {
	key := testPrivateKey(t)
	thumbprint := testThumbprint(t, key)
	verifier, err := NewVerifier("https://api.orifude.com")
	if err != nil {
		t.Fatal(err)
	}
	token := opaqueValue(6)
	tests := []struct {
		name     string
		change   func(map[string]any)
		useToken string
		wantOK   bool
	}{
		{"valid", func(map[string]any) {}, token, true},
		{"wrong token", func(map[string]any) {}, opaqueValue(7), false},
		{"malformed token", func(map[string]any) {}, "short", false},
		{"nonce present", func(c map[string]any) { c["nonce"] = "nonce" }, token, false},
		{"nonce replaces ath", func(c map[string]any) { delete(c, "ath"); c["nonce"] = "nonce" }, token, false},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			claims := validClaims("GET", "https://api.orifude.com/v1/me", strings.Repeat("j", 16), proofTime)
			claims["ath"] = EncodeHash(HashOpaque(token))
			test.change(claims)
			proof := signProof(t, key, claims)
			_, err := verifier.VerifyResource(ResourceProofParams{
				Proof: proof, Method: "GET", EscapedPath: "/v1/me", AccessToken: test.useToken, KeyThumbprint: thumbprint, Now: proofTime,
			})
			if (err == nil) != test.wantOK {
				t.Fatalf("error = %v, want success %t", err, test.wantOK)
			}
		})
	}
}

func TestNewVerifierRejectsNonOrigins(t *testing.T) {
	for _, origin := range []string{"", "api.orifude.com", "ftp://api.orifude.com", "http://api.orifude.com", "https://user@api.orifude.com", "https://api.orifude.com/path", "https://api.orifude.com?query"} {
		if _, err := NewVerifier(origin); !errors.Is(err, ErrInvalidOrigin) {
			t.Fatalf("origin %q error = %v", origin, err)
		}
	}
	if _, err := NewVerifier("http://127.0.0.1:8080"); err != nil {
		t.Fatalf("loopback development origin: %v", err)
	}
}

func validClaims(method, uri, jti string, now time.Time) map[string]any {
	return map[string]any{
		"htm": method,
		"htu": uri,
		"iat": now.Unix(),
		"jti": jti,
	}
}

func signProof(t *testing.T, key *ecdsa.PrivateKey, claims map[string]any) string {
	t.Helper()
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
	proof, err := signed.CompactSerialize()
	if err != nil {
		t.Fatal(err)
	}
	return proof
}

func testThumbprint(t *testing.T, key *ecdsa.PrivateKey) [sha256.Size]byte {
	t.Helper()
	parsed, err := ParsePublicJWK(registrationJWK(t, key))
	if err != nil {
		t.Fatal(err)
	}
	return parsed.Thumbprint
}

func dpopJWK(t *testing.T, key *ecdsa.PrivateKey) string {
	t.Helper()
	raw, err := json.Marshal(jose.JSONWebKey{Key: &key.PublicKey})
	if err != nil {
		t.Fatal(err)
	}
	return string(raw)
}

func opaqueValue(fill byte) string {
	return base64.RawURLEncoding.EncodeToString(bytes.Repeat([]byte{fill}, secretBytes))
}
