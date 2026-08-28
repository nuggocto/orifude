package auth

import (
	"crypto"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"errors"
	"strings"
	"testing"

	"github.com/go-jose/go-jose/v4"
)

func TestParsePublicJWKValidatesP256AndThumbprint(t *testing.T) {
	privateKey := testPrivateKey(t)
	raw := registrationJWK(t, privateKey)
	parsed, err := ParsePublicJWK(raw)
	if err != nil {
		t.Fatal(err)
	}
	if len(parsed.Uncompressed) != 65 || parsed.Uncompressed[0] != 4 {
		t.Fatalf("uncompressed key length/prefix = %d/%d", len(parsed.Uncompressed), parsed.Uncompressed[0])
	}
	jwk := jose.JSONWebKey{Key: &privateKey.PublicKey}
	want, err := jwk.Thumbprint(crypto.SHA256)
	if err != nil {
		t.Fatal(err)
	}
	if string(parsed.Thumbprint[:]) != string(want) {
		t.Fatal("thumbprint does not match RFC 7638")
	}
}

func TestParsePublicJWKRejectsNonCanonicalOrUnsafeKeys(t *testing.T) {
	privateKey := testPrivateKey(t)
	var valid map[string]any
	if err := json.Unmarshal(registrationJWK(t, privateKey), &valid); err != nil {
		t.Fatal(err)
	}
	clone := func() map[string]any {
		copy := make(map[string]any, len(valid))
		for key, value := range valid {
			copy[key] = value
		}
		return copy
	}
	tests := []struct {
		name string
		raw  func() []byte
	}{
		{"algorithm confusion", func() []byte { fields := clone(); fields["alg"] = "ES384"; return mustJSON(t, fields) }},
		{"wrong curve", func() []byte { fields := clone(); fields["crv"] = "P-384"; return mustJSON(t, fields) }},
		{"private field", func() []byte { fields := clone(); fields["d"] = valid["x"]; return mustJSON(t, fields) }},
		{"remote field", func() []byte {
			fields := clone()
			fields["jku"] = "https://example.com/key"
			return mustJSON(t, fields)
		}},
		{"short coordinate", func() []byte {
			fields := clone()
			fields["x"] = base64.RawURLEncoding.EncodeToString(make([]byte, 31))
			return mustJSON(t, fields)
		}},
		{"malformed point", func() []byte {
			fields := clone()
			zero := base64.RawURLEncoding.EncodeToString(make([]byte, 32))
			fields["x"], fields["y"] = zero, zero
			return mustJSON(t, fields)
		}},
		{"duplicate field", func() []byte {
			x := valid["x"].(string)
			return []byte(`{"kty":"EC","crv":"P-256","x":"` + x + `","x":"` + x + `","y":"` + valid["y"].(string) + `","alg":"ES256"}`)
		}},
		{"padded coordinate", func() []byte { fields := clone(); fields["x"] = valid["x"].(string) + "="; return mustJSON(t, fields) }},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if _, err := ParsePublicJWK(test.raw()); !errors.Is(err, ErrInvalidPublicJWK) {
				t.Fatalf("error = %v, want ErrInvalidPublicJWK", err)
			}
		})
	}
}

func registrationJWK(t *testing.T, privateKey *ecdsa.PrivateKey) []byte {
	t.Helper()
	raw, err := json.Marshal(jose.JSONWebKey{Key: &privateKey.PublicKey, Algorithm: string(jose.ES256)})
	if err != nil {
		t.Fatal(err)
	}
	return raw
}

func testPrivateKey(t *testing.T) *ecdsa.PrivateKey {
	t.Helper()
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	return key
}

func mustJSON(t *testing.T, value any) []byte {
	t.Helper()
	raw, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	return raw
}

func replaceProtectedHeader(t *testing.T, proof string, header string) string {
	t.Helper()
	parts := strings.Split(proof, ".")
	if len(parts) != 3 {
		t.Fatalf("proof has %d parts", len(parts))
	}
	parts[0] = base64.RawURLEncoding.EncodeToString([]byte(header))
	return strings.Join(parts, ".")
}
