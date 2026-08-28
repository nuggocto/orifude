package auth

import (
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net/netip"
	"net/url"
	"strings"
	"time"

	"github.com/go-jose/go-jose/v4"
)

const (
	MaxProofBytes = 8 << 10
	MaxClockSkew  = 30 * time.Second
	MinJTIBytes   = 16
	MaxJTIBytes   = 128
)

var (
	ErrInvalidOrigin  = errors.New("auth: invalid public origin")
	ErrInvalidProof   = errors.New("auth: invalid DPoP proof")
	ErrProofClockSkew = errors.New("auth: DPoP proof outside clock skew")
)

type Verifier struct {
	origin string
}

type ChallengeProofParams struct {
	Proof         string
	Method        string
	EscapedPath   string
	NonceHash     [sha256.Size]byte
	KeyThumbprint [sha256.Size]byte
	Now           time.Time
}

type ResourceProofParams struct {
	Proof         string
	Method        string
	EscapedPath   string
	AccessToken   string
	KeyThumbprint [sha256.Size]byte
	Now           time.Time
}

type VerifiedProof struct {
	JTI           string
	JTIHash       [sha256.Size]byte
	KeyThumbprint [sha256.Size]byte
	IssuedAt      time.Time
}

func NewVerifier(publicOrigin string) (*Verifier, error) {
	origin, err := url.Parse(publicOrigin)
	if err != nil || origin.Scheme == "" || origin.Host == "" || origin.User != nil || origin.Path != "" || origin.RawQuery != "" || origin.Fragment != "" {
		return nil, ErrInvalidOrigin
	}
	if origin.Scheme != "https" && origin.Scheme != "http" || origin.Scheme == "http" && !loopbackHost(origin.Hostname()) {
		return nil, ErrInvalidOrigin
	}
	return &Verifier{origin: origin.Scheme + "://" + origin.Host}, nil
}

func loopbackHost(host string) bool {
	if strings.EqualFold(host, "localhost") {
		return true
	}
	ip, err := netip.ParseAddr(host)
	return err == nil && ip.IsLoopback()
}

func (v *Verifier) VerifyChallenge(params ChallengeProofParams) (VerifiedProof, error) {
	return v.verify(params.Proof, params.Method, params.EscapedPath, params.Now, params.KeyThumbprint, "nonce", "", params.NonceHash)
}

func (v *Verifier) VerifyResource(params ResourceProofParams) (VerifiedProof, error) {
	if !validOpaqueSecret(params.AccessToken) {
		return VerifiedProof{}, ErrInvalidProof
	}
	return v.verify(params.Proof, params.Method, params.EscapedPath, params.Now, params.KeyThumbprint, "ath", EncodeHash(HashAccessToken(params.AccessToken)), [sha256.Size]byte{})
}

func (v *Verifier) verify(proof, method, escapedPath string, now time.Time, expectedThumbprint [sha256.Size]byte, bindingName, bindingValue string, bindingHash [sha256.Size]byte) (VerifiedProof, error) {
	if len(proof) == 0 || len(proof) > MaxProofBytes || method == "" || method != strings.ToUpper(method) || !validEscapedPath(escapedPath) {
		return VerifiedProof{}, ErrInvalidProof
	}
	parts := strings.Split(proof, ".")
	if len(parts) != 3 {
		return VerifiedProof{}, ErrInvalidProof
	}
	headerJSON, err := base64.RawURLEncoding.DecodeString(parts[0])
	if err != nil {
		return VerifiedProof{}, ErrInvalidProof
	}
	header, err := strictObject(headerJSON, "typ", "alg", "jwk")
	if err != nil {
		return VerifiedProof{}, errors.Join(ErrInvalidProof, err)
	}
	typ, err := stringField(header, "typ")
	if err != nil || typ != "dpop+jwt" {
		return VerifiedProof{}, ErrInvalidProof
	}
	alg, err := stringField(header, "alg")
	if err != nil || alg != string(jose.ES256) {
		return VerifiedProof{}, ErrInvalidProof
	}
	key, err := parseDPoPPublicJWK(header["jwk"])
	if err != nil || subtle.ConstantTimeCompare(key.Thumbprint[:], expectedThumbprint[:]) != 1 {
		return VerifiedProof{}, ErrInvalidProof
	}

	signed, err := jose.ParseSignedCompact(proof, []jose.SignatureAlgorithm{jose.ES256})
	if err != nil || len(signed.Signatures) != 1 {
		return VerifiedProof{}, ErrInvalidProof
	}
	payload, err := signed.Verify(key.Key)
	if err != nil {
		return VerifiedProof{}, ErrInvalidProof
	}
	claims, err := strictObject(payload, "htm", "htu", "iat", "jti", bindingName)
	if err != nil {
		return VerifiedProof{}, errors.Join(ErrInvalidProof, err)
	}
	htm, err := stringField(claims, "htm")
	if err != nil || htm != strings.ToUpper(method) {
		return VerifiedProof{}, ErrInvalidProof
	}
	htu, err := stringField(claims, "htu")
	if err != nil || htu != v.origin+escapedPath {
		return VerifiedProof{}, ErrInvalidProof
	}
	jti, err := stringField(claims, "jti")
	if err != nil || !validJTI(jti) {
		return VerifiedProof{}, ErrInvalidProof
	}
	binding, err := stringField(claims, bindingName)
	if err != nil || !validBinding(bindingName, binding, bindingValue, bindingHash) {
		return VerifiedProof{}, ErrInvalidProof
	}
	issuedAt, err := numericDate(claims["iat"])
	if err != nil {
		return VerifiedProof{}, ErrInvalidProof
	}
	if issuedAt.Unix() < now.Unix()-int64(MaxClockSkew/time.Second) || issuedAt.Unix() > now.Unix()+int64(MaxClockSkew/time.Second) {
		return VerifiedProof{}, errors.Join(ErrInvalidProof, ErrProofClockSkew)
	}
	return VerifiedProof{
		JTI:           jti,
		JTIHash:       HashOpaque(jti),
		KeyThumbprint: key.Thumbprint,
		IssuedAt:      issuedAt,
	}, nil
}

func validBinding(name, value, expected string, expectedHash [sha256.Size]byte) bool {
	if name == "nonce" {
		hash := HashOpaque(value)
		return validOpaqueSecret(value) && subtle.ConstantTimeCompare(hash[:], expectedHash[:]) == 1
	}
	return subtle.ConstantTimeCompare([]byte(value), []byte(expected)) == 1
}

func validOpaqueSecret(value string) bool {
	decoded, err := base64.RawURLEncoding.DecodeString(value)
	return err == nil && len(decoded) == secretBytes && base64.RawURLEncoding.EncodeToString(decoded) == value
}

func validEscapedPath(path string) bool {
	if !strings.HasPrefix(path, "/") || strings.ContainsAny(path, "?#") {
		return false
	}
	_, err := url.PathUnescape(path)
	return err == nil
}

func validJTI(jti string) bool {
	if len(jti) < MinJTIBytes || len(jti) > MaxJTIBytes {
		return false
	}
	for i := range len(jti) {
		if jti[i] < 0x20 || jti[i] > 0x7e {
			return false
		}
	}
	return true
}

func numericDate(raw json.RawMessage) (time.Time, error) {
	var seconds int64
	if err := json.Unmarshal(raw, &seconds); err != nil {
		return time.Time{}, fmt.Errorf("iat: %w", err)
	}
	return time.Unix(seconds, 0), nil
}
