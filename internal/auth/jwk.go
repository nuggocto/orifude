package auth

import (
	"bytes"
	"crypto"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/big"

	"github.com/go-jose/go-jose/v4"
)

var ErrInvalidPublicJWK = errors.New("auth: invalid public JWK")

type PublicKey struct {
	Key          *ecdsa.PublicKey
	Uncompressed []byte
	Thumbprint   [sha256.Size]byte
}

func ParsePublicJWK(data []byte) (PublicKey, error) {
	fields, err := strictObject(data, "kty", "crv", "x", "y", "alg")
	if err != nil {
		return PublicKey{}, errors.Join(ErrInvalidPublicJWK, err)
	}
	alg, err := stringField(fields, "alg")
	if err != nil || alg != string(jose.ES256) {
		return PublicKey{}, ErrInvalidPublicJWK
	}
	return parseP256Fields(fields)
}

func parseDPoPPublicJWK(data []byte) (PublicKey, error) {
	fields, err := strictObject(data, "kty", "crv", "x", "y")
	if err != nil {
		return PublicKey{}, errors.Join(ErrInvalidPublicJWK, err)
	}
	return parseP256Fields(fields)
}

func parseP256Fields(fields map[string]json.RawMessage) (PublicKey, error) {
	kty, err := stringField(fields, "kty")
	if err != nil || kty != "EC" {
		return PublicKey{}, ErrInvalidPublicJWK
	}
	curve, err := stringField(fields, "crv")
	if err != nil || curve != "P-256" {
		return PublicKey{}, ErrInvalidPublicJWK
	}
	x, err := coordinate(fields, "x")
	if err != nil {
		return PublicKey{}, ErrInvalidPublicJWK
	}
	y, err := coordinate(fields, "y")
	if err != nil {
		return PublicKey{}, ErrInvalidPublicJWK
	}

	key := &ecdsa.PublicKey{Curve: elliptic.P256(), X: new(big.Int).SetBytes(x), Y: new(big.Int).SetBytes(y)}
	if !key.Curve.IsOnCurve(key.X, key.Y) {
		return PublicKey{}, ErrInvalidPublicJWK
	}
	jwk := jose.JSONWebKey{Key: key}
	thumbprint, err := jwk.Thumbprint(crypto.SHA256)
	if err != nil || len(thumbprint) != sha256.Size {
		return PublicKey{}, ErrInvalidPublicJWK
	}
	var result PublicKey
	result.Key = key
	result.Uncompressed = elliptic.Marshal(elliptic.P256(), key.X, key.Y)
	copy(result.Thumbprint[:], thumbprint)
	return result, nil
}

func coordinate(fields map[string]json.RawMessage, name string) ([]byte, error) {
	encoded, err := stringField(fields, name)
	if err != nil {
		return nil, err
	}
	decoded, err := base64.RawURLEncoding.DecodeString(encoded)
	if err != nil || len(decoded) != 32 || base64.RawURLEncoding.EncodeToString(decoded) != encoded {
		return nil, ErrInvalidPublicJWK
	}
	return decoded, nil
}

func stringField(fields map[string]json.RawMessage, name string) (string, error) {
	var value string
	if err := json.Unmarshal(fields[name], &value); err != nil {
		return "", err
	}
	return value, nil
}

func strictObject(data []byte, names ...string) (map[string]json.RawMessage, error) {
	allowed := make(map[string]struct{}, len(names))
	for _, name := range names {
		allowed[name] = struct{}{}
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	token, err := decoder.Token()
	if err != nil || token != json.Delim('{') {
		return nil, errors.New("expected object")
	}
	fields := make(map[string]json.RawMessage, len(names))
	for decoder.More() {
		token, err = decoder.Token()
		if err != nil {
			return nil, err
		}
		name, ok := token.(string)
		if !ok {
			return nil, errors.New("expected field name")
		}
		if _, ok := allowed[name]; !ok {
			return nil, fmt.Errorf("field %q is not allowed", name)
		}
		if _, duplicate := fields[name]; duplicate {
			return nil, fmt.Errorf("field %q is duplicated", name)
		}
		var value json.RawMessage
		if err := decoder.Decode(&value); err != nil {
			return nil, err
		}
		fields[name] = value
	}
	if _, err := decoder.Token(); err != nil {
		return nil, err
	}
	if token, err := decoder.Token(); err != io.EOF {
		if err != nil {
			return nil, err
		}
		return nil, fmt.Errorf("unexpected trailing token %v", token)
	}
	if len(fields) != len(names) {
		return nil, errors.New("missing required field")
	}
	return fields, nil
}
