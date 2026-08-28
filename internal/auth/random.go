package auth

import (
	"crypto/sha256"
	"encoding/base64"
	"errors"
	"io"
)

const (
	publicIDBytes = 16
	secretBytes   = 32
)

var ErrRandomSource = errors.New("auth: random source failed")

type Challenge struct {
	ID        string
	Nonce     string
	NonceHash [sha256.Size]byte
}

func NewChallenge(r io.Reader) (Challenge, error) {
	id, err := randomBase64URL(r, publicIDBytes)
	if err != nil {
		return Challenge{}, err
	}
	nonce, err := randomBase64URL(r, secretBytes)
	if err != nil {
		return Challenge{}, err
	}
	return Challenge{ID: id, Nonce: nonce, NonceHash: HashOpaque(nonce)}, nil
}

func NewAccessToken(r io.Reader) (string, [sha256.Size]byte, error) {
	token, err := randomBase64URL(r, secretBytes)
	if err != nil {
		return "", [sha256.Size]byte{}, err
	}
	return token, HashAccessToken(token), nil
}

func NewPublicID(r io.Reader) (string, error) {
	return randomBase64URL(r, publicIDBytes)
}

func HashOpaque(value string) [sha256.Size]byte {
	return sha256.Sum256([]byte(value))
}

func HashAccessToken(token string) [sha256.Size]byte {
	return HashOpaque(token)
}

func HashRevocationCredential(credential string) [sha256.Size]byte {
	return HashOpaque(credential)
}

func EncodeHash(hash [sha256.Size]byte) string {
	return base64.RawURLEncoding.EncodeToString(hash[:])
}

func randomBase64URL(r io.Reader, size int) (string, error) {
	if r == nil {
		return "", ErrRandomSource
	}
	b := make([]byte, size)
	if _, err := io.ReadFull(r, b); err != nil {
		return "", errors.Join(ErrRandomSource, err)
	}
	return base64.RawURLEncoding.EncodeToString(b), nil
}
