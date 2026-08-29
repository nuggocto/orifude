package auth

import (
	"crypto/sha256"
	"encoding/json"
	"errors"
	"io"
	"net/url"
	"strings"
	"time"

	"github.com/go-jose/go-jose/v4"
)

// Prover signs challenge and resource DPoP proofs for one device key.
type Prover struct {
	device *DeviceKey
	origin string
	random io.Reader
	now    func() time.Time
}

// NewProver binds a device key to one exact API origin.
func NewProver(device *DeviceKey, publicOrigin string, random io.Reader, now func() time.Time) (*Prover, error) {
	origin, err := url.Parse(publicOrigin)
	if err != nil || origin.Scheme == "" || origin.Host == "" || origin.User != nil || origin.Path != "" ||
		origin.RawQuery != "" || origin.Fragment != "" || origin.Scheme != "https" && (origin.Scheme != "http" || !loopbackHost(origin.Hostname())) {
		return nil, ErrInvalidOrigin
	}
	if device == nil || device.private == nil {
		return nil, ErrInvalidDeviceKey
	}
	if random == nil {
		return nil, ErrRandomSource
	}
	if now == nil {
		now = time.Now
	}
	return &Prover{device: device, origin: origin.Scheme + "://" + origin.Host, random: random, now: now}, nil
}

// ChallengeProof signs a proof bound to an issued challenge nonce.
func (p *Prover) ChallengeProof(method, escapedPath, nonce string) (string, error) {
	if !validOpaqueSecret(nonce) {
		return "", ErrInvalidProof
	}
	return p.sign(method, escapedPath, "nonce", nonce)
}

// ResourceProof signs a proof bound to an opaque access token.
func (p *Prover) ResourceProof(method, escapedPath, accessToken string) (string, error) {
	if !validOpaqueSecret(accessToken) {
		return "", ErrInvalidProof
	}
	hash := sha256.Sum256([]byte(accessToken))
	return p.sign(method, escapedPath, "ath", EncodeHash(hash))
}

func (p *Prover) sign(method, escapedPath, bindingName, binding string) (string, error) {
	if p == nil || p.device == nil || p.device.private == nil || method == "" || method != strings.ToUpper(method) || !validEscapedPath(escapedPath) {
		return "", ErrInvalidProof
	}
	jti, err := randomBase64URL(p.random, publicIDBytes)
	if err != nil {
		return "", err
	}
	claims := map[string]any{
		"htm":       method,
		"htu":       p.origin + escapedPath,
		"iat":       p.now().Unix(),
		"jti":       jti,
		bindingName: binding,
	}
	payload, err := json.Marshal(claims)
	if err != nil {
		return "", errors.Join(ErrInvalidProof, err)
	}
	options := new(jose.SignerOptions).WithType("dpop+jwt")
	options.EmbedJWK = true
	signer, err := jose.NewSigner(jose.SigningKey{Algorithm: jose.ES256, Key: p.device.private}, options)
	if err != nil {
		return "", errors.Join(ErrInvalidProof, err)
	}
	signed, err := signer.Sign(payload)
	if err != nil {
		return "", errors.Join(ErrInvalidProof, err)
	}
	proof, err := signed.CompactSerialize()
	if err != nil || len(proof) > MaxProofBytes {
		return "", ErrInvalidProof
	}
	return proof, nil
}
