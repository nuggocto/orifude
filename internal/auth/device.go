package auth

import (
	"crypto"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"errors"
	"io"

	"github.com/go-jose/go-jose/v4"
)

var ErrInvalidDeviceKey = errors.New("auth: invalid device key")

// DeviceKey owns one P-256 private key used for identity proofs.
type DeviceKey struct {
	private    *ecdsa.PrivateKey
	thumbprint [sha256.Size]byte
}

// GenerateDeviceKey creates a new P-256 identity key.
func GenerateDeviceKey(random io.Reader) (*DeviceKey, error) {
	if random == nil {
		return nil, ErrRandomSource
	}
	private, err := ecdsa.GenerateKey(elliptic.P256(), random)
	if err != nil {
		return nil, errors.Join(ErrRandomSource, err)
	}
	return newDeviceKey(private)
}

// ParseDeviceKey parses a PKCS#8-encoded P-256 identity key.
func ParseDeviceKey(encoded []byte) (*DeviceKey, error) {
	parsed, err := x509.ParsePKCS8PrivateKey(encoded)
	if err != nil {
		return nil, errors.Join(ErrInvalidDeviceKey, err)
	}
	private, ok := parsed.(*ecdsa.PrivateKey)
	if !ok {
		return nil, ErrInvalidDeviceKey
	}
	return newDeviceKey(private)
}

func newDeviceKey(private *ecdsa.PrivateKey) (*DeviceKey, error) {
	if private == nil || private.Curve != elliptic.P256() || private.X == nil || private.Y == nil || private.D == nil ||
		private.D.Sign() <= 0 || !elliptic.P256().IsOnCurve(private.X, private.Y) {
		return nil, ErrInvalidDeviceKey
	}
	jwk := jose.JSONWebKey{Key: &private.PublicKey}
	thumbprint, err := jwk.Thumbprint(crypto.SHA256)
	if err != nil || len(thumbprint) != sha256.Size {
		return nil, ErrInvalidDeviceKey
	}
	device := &DeviceKey{private: private}
	copy(device.thumbprint[:], thumbprint)
	return device, nil
}

// MarshalPKCS8 returns the private key in the local-storage representation.
func (d *DeviceKey) MarshalPKCS8() ([]byte, error) {
	if d == nil || d.private == nil {
		return nil, ErrInvalidDeviceKey
	}
	encoded, err := x509.MarshalPKCS8PrivateKey(d.private)
	if err != nil {
		return nil, errors.Join(ErrInvalidDeviceKey, err)
	}
	return encoded, nil
}

// PublicCoordinates returns canonical unpadded base64url P-256 coordinates.
func (d *DeviceKey) PublicCoordinates() (string, string, error) {
	if d == nil || d.private == nil {
		return "", "", ErrInvalidDeviceKey
	}
	x := d.private.X.FillBytes(make([]byte, 32))
	y := d.private.Y.FillBytes(make([]byte, 32))
	return base64.RawURLEncoding.EncodeToString(x), base64.RawURLEncoding.EncodeToString(y), nil
}

// Thumbprint returns the RFC 7638 SHA-256 public-key thumbprint.
func (d *DeviceKey) Thumbprint() [sha256.Size]byte {
	if d == nil {
		return [sha256.Size]byte{}
	}
	return d.thumbprint
}

// GenerateRevocationCredential creates the delete-only identity credential.
func GenerateRevocationCredential(random io.Reader) (string, error) {
	return randomBase64URL(random, secretBytes)
}

// GenerateClientID creates a canonical 128-bit public mutation identifier.
func GenerateClientID(random io.Reader) (string, error) {
	return randomBase64URL(random, publicIDBytes)
}
