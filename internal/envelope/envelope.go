// Package envelope encrypts message and report evidence with KMS data keys.
package envelope

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/subtle"
	"encoding/base64"
	"errors"
	"io"
	"strings"

	"github.com/aws/aws-sdk-go-v2/aws/arn"
	"github.com/aws/aws-sdk-go-v2/service/kms"
	"github.com/aws/aws-sdk-go-v2/service/kms/types"
)

const (
	EncryptionVersion          int16 = 1
	IDLength                         = 22
	NonceBytes                       = 12
	DataKeyBytes                     = 32
	MaxWrappedKeyBytes               = 6 * 1024
	MaxMessagePlaintextBytes         = 12 * 1024
	MaxMessageCiphertextBytes        = MaxMessagePlaintextBytes + 16
	MaxEvidencePlaintextBytes        = 12 * 1024
	MaxEvidenceCiphertextBytes       = MaxEvidencePlaintextBytes + 16
)

var (
	errConfiguration  = errors.New("envelope: invalid configuration")
	errMetadata       = errors.New("envelope: invalid metadata")
	errPlaintext      = errors.New("envelope: invalid plaintext size")
	errRecord         = errors.New("envelope: invalid encrypted record")
	errKMS            = errors.New("envelope: KMS operation failed")
	errKMSResponse    = errors.New("envelope: invalid KMS response")
	errRandom         = errors.New("envelope: nonce generation failed")
	errAuthentication = errors.New("envelope: authentication failed")
)

// KMS is the exact subset of the AWS KMS client used by Cipher.
type KMS interface {
	GenerateDataKey(context.Context, *kms.GenerateDataKeyInput, ...func(*kms.Options)) (*kms.GenerateDataKeyOutput, error)
	Decrypt(context.Context, *kms.DecryptInput, ...func(*kms.Options)) (*kms.DecryptOutput, error)
}

var _ KMS = (*kms.Client)(nil)

// Target identifies which message a report contains.
type Target string

const (
	TargetOriginal Target = "original"
	TargetReply    Target = "reply"
)

// Envelope is the complete encrypted value safe for persistence.
type Envelope struct {
	Ciphertext []byte
	Nonce      []byte
	WrappedKey []byte
	KMSKeyARN  string
	Version    int16
}

// Cipher performs envelope encryption with exact configured KMS key ARNs.
type Cipher struct {
	kms            KMS
	random         io.Reader
	messageKeyARN  string
	evidenceKeyARN string
}

// New constructs a Cipher. messageKeyARN and evidenceKeyARN must be distinct
// KMS key ARNs; random should be crypto/rand.Reader in production.
func New(client KMS, random io.Reader, messageKeyARN, evidenceKeyARN string) (*Cipher, error) {
	if client == nil || random == nil || !validKeyARN(messageKeyARN) || !validKeyARN(evidenceKeyARN) || messageKeyARN == evidenceKeyARN {
		return nil, errConfiguration
	}
	return &Cipher{
		kms:            client,
		random:         random,
		messageKeyARN:  messageKeyARN,
		evidenceKeyARN: evidenceKeyARN,
	}, nil
}

// EncryptOriginal encrypts an original participant message.
func (c *Cipher) EncryptOriginal(ctx context.Context, letterID string, plaintext []byte) (Envelope, error) {
	context, aad, err := originalBinding(letterID)
	if err != nil {
		return Envelope{}, err
	}
	return c.encrypt(ctx, c.messageKeyARN, context, aad, plaintext, MaxMessagePlaintextBytes)
}

// DecryptOriginal decrypts an original participant message.
func (c *Cipher) DecryptOriginal(ctx context.Context, letterID string, envelope Envelope) ([]byte, error) {
	context, aad, err := originalBinding(letterID)
	if err != nil {
		return nil, err
	}
	return c.decrypt(ctx, c.messageKeyARN, context, aad, envelope, MaxMessageCiphertextBytes)
}

// EncryptReply encrypts a participant reply.
func (c *Cipher) EncryptReply(ctx context.Context, letterID, replyID string, plaintext []byte) (Envelope, error) {
	context, aad, err := replyBinding(letterID, replyID)
	if err != nil {
		return Envelope{}, err
	}
	return c.encrypt(ctx, c.messageKeyARN, context, aad, plaintext, MaxMessagePlaintextBytes)
}

// DecryptReply decrypts a participant reply.
func (c *Cipher) DecryptReply(ctx context.Context, letterID, replyID string, envelope Envelope) ([]byte, error) {
	context, aad, err := replyBinding(letterID, replyID)
	if err != nil {
		return nil, err
	}
	return c.decrypt(ctx, c.messageKeyARN, context, aad, envelope, MaxMessageCiphertextBytes)
}

// EncryptEvidence encrypts reported content with the evidence key.
func (c *Cipher) EncryptEvidence(ctx context.Context, reportID, letterID string, target Target, plaintext []byte) (Envelope, error) {
	context, aad, err := evidenceBinding(reportID, letterID, target)
	if err != nil {
		return Envelope{}, err
	}
	return c.encrypt(ctx, c.evidenceKeyARN, context, aad, plaintext, MaxEvidencePlaintextBytes)
}

// DecryptEvidence decrypts reported content when the supplied KMS caller has
// evidence-decrypt permission.
func (c *Cipher) DecryptEvidence(ctx context.Context, reportID, letterID string, target Target, envelope Envelope) ([]byte, error) {
	context, aad, err := evidenceBinding(reportID, letterID, target)
	if err != nil {
		return nil, err
	}
	return c.decrypt(ctx, c.evidenceKeyARN, context, aad, envelope, MaxEvidenceCiphertextBytes)
}

// CheckMessageKey verifies message GenerateDataKey and Decrypt permissions.
func (c *Cipher) CheckMessageKey(ctx context.Context) error {
	context := messageCanaryContext()
	generated, err := c.generateDataKey(ctx, c.messageKeyARN, context)
	if err != nil {
		return err
	}
	defer clear(generated.Plaintext)
	defer clear(generated.CiphertextBlob)

	decrypted, err := c.decryptDataKey(ctx, c.messageKeyARN, context, generated.CiphertextBlob)
	if err != nil {
		return err
	}
	defer clear(decrypted)
	if subtle.ConstantTimeCompare(generated.Plaintext, decrypted) != 1 {
		return errKMSResponse
	}
	return nil
}

// CheckEvidenceKey verifies evidence GenerateDataKey permission without
// requiring evidence-decrypt permission.
func (c *Cipher) CheckEvidenceKey(ctx context.Context) error {
	generated, err := c.generateDataKey(ctx, c.evidenceKeyARN, evidenceCanaryContext())
	if err != nil {
		return err
	}
	clear(generated.Plaintext)
	clear(generated.CiphertextBlob)
	return nil
}

func (c *Cipher) encrypt(ctx context.Context, keyARN string, encryptionContext map[string]string, aad, plaintext []byte, maxPlaintext int) (Envelope, error) {
	if len(plaintext) == 0 || len(plaintext) > maxPlaintext {
		return Envelope{}, errPlaintext
	}
	generated, err := c.generateDataKey(ctx, keyARN, encryptionContext)
	if err != nil {
		return Envelope{}, err
	}
	defer clear(generated.Plaintext)

	block, err := aes.NewCipher(generated.Plaintext)
	if err != nil {
		return Envelope{}, errKMSResponse
	}
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return Envelope{}, errKMSResponse
	}
	nonce := make([]byte, NonceBytes)
	if _, err := io.ReadFull(c.random, nonce); err != nil {
		return Envelope{}, errRandom
	}
	return Envelope{
		Ciphertext: gcm.Seal(nil, nonce, plaintext, aad),
		Nonce:      nonce,
		WrappedKey: generated.CiphertextBlob,
		KMSKeyARN:  keyARN,
		Version:    EncryptionVersion,
	}, nil
}

func (c *Cipher) decrypt(ctx context.Context, keyARN string, encryptionContext map[string]string, aad []byte, envelope Envelope, maxCiphertext int) ([]byte, error) {
	if envelope.Version != EncryptionVersion || envelope.KMSKeyARN != keyARN || len(envelope.Nonce) != NonceBytes ||
		len(envelope.WrappedKey) == 0 || len(envelope.WrappedKey) > MaxWrappedKeyBytes ||
		len(envelope.Ciphertext) <= aes.BlockSize || len(envelope.Ciphertext) > maxCiphertext {
		return nil, errRecord
	}
	dataKey, err := c.decryptDataKey(ctx, keyARN, encryptionContext, envelope.WrappedKey)
	if err != nil {
		return nil, err
	}
	defer clear(dataKey)

	block, err := aes.NewCipher(dataKey)
	if err != nil {
		return nil, errKMSResponse
	}
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, errKMSResponse
	}
	plaintext, err := gcm.Open(nil, envelope.Nonce, envelope.Ciphertext, aad)
	if err != nil {
		return nil, errAuthentication
	}
	return plaintext, nil
}

func (c *Cipher) generateDataKey(ctx context.Context, keyARN string, encryptionContext map[string]string) (*kms.GenerateDataKeyOutput, error) {
	output, err := c.kms.GenerateDataKey(ctx, &kms.GenerateDataKeyInput{
		KeyId:             &keyARN,
		KeySpec:           types.DataKeySpecAes256,
		EncryptionContext: encryptionContext,
	})
	if err != nil {
		return nil, errKMS
	}
	if output == nil || output.KeyId == nil || *output.KeyId != keyARN || len(output.Plaintext) != DataKeyBytes ||
		len(output.CiphertextBlob) == 0 || len(output.CiphertextBlob) > MaxWrappedKeyBytes {
		if output != nil {
			clear(output.Plaintext)
		}
		return nil, errKMSResponse
	}
	return output, nil
}

func (c *Cipher) decryptDataKey(ctx context.Context, keyARN string, encryptionContext map[string]string, wrappedKey []byte) ([]byte, error) {
	output, err := c.kms.Decrypt(ctx, &kms.DecryptInput{
		CiphertextBlob:      wrappedKey,
		EncryptionAlgorithm: types.EncryptionAlgorithmSpecSymmetricDefault,
		EncryptionContext:   encryptionContext,
		KeyId:               &keyARN,
	})
	if err != nil {
		return nil, errKMS
	}
	if output == nil || output.KeyId == nil || *output.KeyId != keyARN || len(output.Plaintext) != DataKeyBytes {
		if output != nil {
			clear(output.Plaintext)
		}
		return nil, errKMSResponse
	}
	return output.Plaintext, nil
}

func originalBinding(letterID string) (map[string]string, []byte, error) {
	if !validID(letterID) {
		return nil, nil, errMetadata
	}
	return map[string]string{
		"service":     "orifude",
		"schema":      "1",
		"key_purpose": "message",
		"letter_id":   letterID,
		"part":        "original",
	}, []byte("orifude:v1:letter:" + letterID + ":original"), nil
}

func replyBinding(letterID, replyID string) (map[string]string, []byte, error) {
	if !validID(letterID) || !validID(replyID) {
		return nil, nil, errMetadata
	}
	return map[string]string{
		"service":     "orifude",
		"schema":      "1",
		"key_purpose": "message",
		"letter_id":   letterID,
		"part":        "reply",
		"reply_id":    replyID,
	}, []byte("orifude:v1:letter:" + letterID + ":reply:" + replyID), nil
}

func evidenceBinding(reportID, letterID string, target Target) (map[string]string, []byte, error) {
	if !validID(reportID) || !validID(letterID) || (target != TargetOriginal && target != TargetReply) {
		return nil, nil, errMetadata
	}
	return map[string]string{
		"service":     "orifude",
		"schema":      "1",
		"key_purpose": "evidence",
		"report_id":   reportID,
		"letter_id":   letterID,
		"target":      string(target),
		"purpose":     "reported-content-review",
	}, []byte("orifude:v1:evidence:" + reportID + ":" + letterID + ":" + string(target) + ":reported-content-review"), nil
}

func messageCanaryContext() map[string]string {
	return map[string]string{
		"service":     "orifude",
		"schema":      "1",
		"operation":   "startup",
		"key_purpose": "message",
	}
}

func evidenceCanaryContext() map[string]string {
	return map[string]string{
		"service":     "orifude",
		"schema":      "1",
		"operation":   "startup",
		"key_purpose": "evidence",
		"purpose":     "reported-content-review",
	}
}

func validID(value string) bool {
	if len(value) != IDLength {
		return false
	}
	decoded, err := base64.RawURLEncoding.Strict().DecodeString(value)
	return err == nil && len(decoded) == 16
}

func validKeyARN(value string) bool {
	parsed, err := arn.Parse(value)
	return err == nil && parsed.Service == "kms" && parsed.Region != "" && parsed.AccountID != "" &&
		strings.HasPrefix(parsed.Resource, "key/") && len(parsed.Resource) > len("key/") && !strings.ContainsAny(value, "*? \t\r\n")
}
