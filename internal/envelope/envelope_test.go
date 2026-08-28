package envelope

import (
	"bytes"
	"context"
	"encoding/base64"
	"errors"
	"io"
	"maps"
	"slices"
	"testing"

	"github.com/aws/aws-sdk-go-v2/service/kms"
	"github.com/aws/aws-sdk-go-v2/service/kms/types"
)

const (
	messageKeyARN  = "arn:aws:kms:us-east-1:123456789012:key/11111111-1111-1111-1111-111111111111"
	evidenceKeyARN = "arn:aws:kms:us-east-1:123456789012:key/22222222-2222-2222-2222-222222222222"
)

func TestRoundTripsUseExactBindingsAndFreshNonces(t *testing.T) {
	letterID := testID(1)
	replyID := testID(2)
	reportID := testID(3)
	plaintext := []byte("a synthetic letter")

	tests := []struct {
		name    string
		keyARN  string
		context map[string]string
		encrypt func(*Cipher, context.Context, []byte) (Envelope, error)
		decrypt func(*Cipher, context.Context, Envelope) ([]byte, error)
	}{
		{
			name:   "original",
			keyARN: messageKeyARN,
			context: map[string]string{
				"service": "orifude", "schema": "1", "key_purpose": "message",
				"letter_id": letterID, "part": "original",
			},
			encrypt: func(c *Cipher, ctx context.Context, plaintext []byte) (Envelope, error) {
				return c.EncryptOriginal(ctx, letterID, plaintext)
			},
			decrypt: func(c *Cipher, ctx context.Context, envelope Envelope) ([]byte, error) {
				return c.DecryptOriginal(ctx, letterID, envelope)
			},
		},
		{
			name:   "reply",
			keyARN: messageKeyARN,
			context: map[string]string{
				"service": "orifude", "schema": "1", "key_purpose": "message",
				"letter_id": letterID, "part": "reply", "reply_id": replyID,
			},
			encrypt: func(c *Cipher, ctx context.Context, plaintext []byte) (Envelope, error) {
				return c.EncryptReply(ctx, letterID, replyID, plaintext)
			},
			decrypt: func(c *Cipher, ctx context.Context, envelope Envelope) ([]byte, error) {
				return c.DecryptReply(ctx, letterID, replyID, envelope)
			},
		},
		{
			name:   "evidence original",
			keyARN: evidenceKeyARN,
			context: map[string]string{
				"service": "orifude", "schema": "1", "key_purpose": "evidence",
				"report_id": reportID, "letter_id": letterID, "target": "original",
				"purpose": "reported-content-review",
			},
			encrypt: func(c *Cipher, ctx context.Context, plaintext []byte) (Envelope, error) {
				return c.EncryptEvidence(ctx, reportID, letterID, TargetOriginal, plaintext)
			},
			decrypt: func(c *Cipher, ctx context.Context, envelope Envelope) ([]byte, error) {
				return c.DecryptEvidence(ctx, reportID, letterID, TargetOriginal, envelope)
			},
		},
		{
			name:   "evidence reply",
			keyARN: evidenceKeyARN,
			context: map[string]string{
				"service": "orifude", "schema": "1", "key_purpose": "evidence",
				"report_id": reportID, "letter_id": letterID, "target": "reply",
				"purpose": "reported-content-review",
			},
			encrypt: func(c *Cipher, ctx context.Context, plaintext []byte) (Envelope, error) {
				return c.EncryptEvidence(ctx, reportID, letterID, TargetReply, plaintext)
			},
			decrypt: func(c *Cipher, ctx context.Context, envelope Envelope) ([]byte, error) {
				return c.DecryptEvidence(ctx, reportID, letterID, TargetReply, envelope)
			},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			client := newSyntheticKMS()
			c := newTestCipher(t, client, bytes.NewReader(sequence(2*NonceBytes)))

			first, err := test.encrypt(c, t.Context(), plaintext)
			if err != nil {
				t.Fatalf("encrypt: %v", err)
			}
			second, err := test.encrypt(c, t.Context(), plaintext)
			if err != nil {
				t.Fatalf("second encrypt: %v", err)
			}
			if first.KMSKeyARN != test.keyARN || first.Version != EncryptionVersion {
				t.Fatalf("envelope metadata = (%q, %d)", first.KMSKeyARN, first.Version)
			}
			if len(first.Nonce) != NonceBytes || slices.Equal(first.Nonce, second.Nonce) {
				t.Fatalf("nonces were not distinct %d-byte values", NonceBytes)
			}
			if len(first.Ciphertext) != len(plaintext)+16 {
				t.Fatalf("ciphertext length = %d, want %d", len(first.Ciphertext), len(plaintext)+16)
			}
			if len(client.generates) != 2 || client.generates[0].keySpec != types.DataKeySpecAes256 ||
				client.generates[0].keyARN != test.keyARN || !maps.Equal(client.generates[0].encryptionContext, test.context) ||
				!maps.Equal(client.generates[0].encryptionContext, client.generates[1].encryptionContext) {
				t.Fatalf("GenerateDataKey request did not use the exact deterministic binding")
			}

			got, err := test.decrypt(c, t.Context(), first)
			if err != nil {
				t.Fatalf("decrypt: %v", err)
			}
			if !bytes.Equal(got, plaintext) {
				t.Fatalf("plaintext = %q, want %q", got, plaintext)
			}
			if len(client.decrypts) != 1 || client.decrypts[0].keyARN != test.keyARN ||
				!maps.Equal(client.decrypts[0].encryptionContext, test.context) {
				t.Fatalf("Decrypt request did not use the exact deterministic binding")
			}
		})
	}
}

func TestAdditionalDataEncodings(t *testing.T) {
	letterID := testID(1)
	replyID := testID(2)
	reportID := testID(3)
	tests := []struct {
		name    string
		binding func() (map[string]string, []byte, error)
		want    string
	}{
		{
			name: "original",
			binding: func() (map[string]string, []byte, error) {
				return originalBinding(letterID)
			},
			want: "orifude:v1:letter:" + letterID + ":original",
		},
		{
			name: "reply",
			binding: func() (map[string]string, []byte, error) {
				return replyBinding(letterID, replyID)
			},
			want: "orifude:v1:letter:" + letterID + ":reply:" + replyID,
		},
		{
			name: "evidence",
			binding: func() (map[string]string, []byte, error) {
				return evidenceBinding(reportID, letterID, TargetReply)
			},
			want: "orifude:v1:evidence:" + reportID + ":" + letterID + ":reply:reported-content-review",
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			_, aad, err := test.binding()
			if err != nil {
				t.Fatal(err)
			}
			if string(aad) != test.want {
				t.Fatalf("AAD = %q, want %q", aad, test.want)
			}
		})
	}
}

func TestDecryptFailsClosedOnTampering(t *testing.T) {
	letterID := testID(1)
	replyID := testID(2)
	client := newSyntheticKMS()
	c := newTestCipher(t, client, bytes.NewReader(sequence(NonceBytes)))
	encrypted, err := c.EncryptReply(t.Context(), letterID, replyID, []byte("reply"))
	if err != nil {
		t.Fatal(err)
	}

	tests := []struct {
		name   string
		mutate func(*Envelope)
		want   error
	}{
		{name: "nonce", mutate: func(e *Envelope) { e.Nonce[0] ^= 1 }, want: errAuthentication},
		{name: "ciphertext", mutate: func(e *Envelope) { e.Ciphertext[0] ^= 1 }, want: errAuthentication},
		{name: "tag", mutate: func(e *Envelope) { e.Ciphertext[len(e.Ciphertext)-1] ^= 1 }, want: errAuthentication},
		{name: "wrapped key", mutate: func(e *Envelope) { e.WrappedKey[0] ^= 1 }, want: errKMS},
		{name: "version", mutate: func(e *Envelope) { e.Version++ }, want: errRecord},
		{name: "ARN", mutate: func(e *Envelope) { e.KMSKeyARN = evidenceKeyARN }, want: errRecord},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			altered := cloneEnvelope(encrypted)
			test.mutate(&altered)
			_, err := c.DecryptReply(t.Context(), letterID, replyID, altered)
			if !errors.Is(err, test.want) {
				t.Fatalf("error = %v, want %v", err, test.want)
			}
		})
	}

	if _, err := c.DecryptReply(t.Context(), letterID, testID(4), encrypted); !errors.Is(err, errKMS) {
		t.Fatalf("altered KMS context error = %v, want %v", err, errKMS)
	}

	client.ignoreContext = true
	if _, err := c.DecryptReply(t.Context(), letterID, testID(4), encrypted); !errors.Is(err, errAuthentication) {
		t.Fatalf("altered AAD error = %v, want %v", err, errAuthentication)
	}
}

func TestRejectsInvalidKMSResponsesAndFailures(t *testing.T) {
	letterID := testID(1)

	tests := []struct {
		name      string
		configure func(*syntheticKMS)
		decrypt   bool
		want      error
	}{
		{name: "generate failure", configure: func(k *syntheticKMS) { k.generateErr = errors.New("secret provider detail") }, want: errKMS},
		{name: "generate wrong ARN", configure: func(k *syntheticKMS) { k.generateARN = evidenceKeyARN }, want: errKMSResponse},
		{name: "generate short key", configure: func(k *syntheticKMS) { k.generateKeyBytes = DataKeyBytes - 1 }, want: errKMSResponse},
		{name: "generate empty wrapped key", configure: func(k *syntheticKMS) { k.emptyWrappedKey = true }, want: errKMSResponse},
		{name: "generate oversized wrapped key", configure: func(k *syntheticKMS) { k.oversizedWrappedKey = true }, want: errKMSResponse},
		{name: "decrypt failure", configure: func(k *syntheticKMS) { k.decryptErr = errors.New("secret provider detail") }, decrypt: true, want: errKMS},
		{name: "decrypt wrong ARN", configure: func(k *syntheticKMS) { k.decryptARN = evidenceKeyARN }, decrypt: true, want: errKMSResponse},
		{name: "decrypt short key", configure: func(k *syntheticKMS) { k.decryptKeyBytes = DataKeyBytes - 1 }, decrypt: true, want: errKMSResponse},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			client := newSyntheticKMS()
			c := newTestCipher(t, client, bytes.NewReader(sequence(NonceBytes)))
			if !test.decrypt {
				test.configure(client)
				_, err := c.EncryptOriginal(t.Context(), letterID, []byte("letter"))
				if !errors.Is(err, test.want) {
					t.Fatalf("error = %v, want %v", err, test.want)
				}
				return
			}

			encrypted, err := c.EncryptOriginal(t.Context(), letterID, []byte("letter"))
			if err != nil {
				t.Fatal(err)
			}
			test.configure(client)
			_, err = c.DecryptOriginal(t.Context(), letterID, encrypted)
			if !errors.Is(err, test.want) {
				t.Fatalf("error = %v, want %v", err, test.want)
			}
		})
	}
}

func TestValidatesConfigurationMetadataAndBounds(t *testing.T) {
	client := newSyntheticKMS()
	if _, err := New(nil, bytes.NewReader(nil), messageKeyARN, evidenceKeyARN); !errors.Is(err, errConfiguration) {
		t.Fatalf("nil KMS error = %v", err)
	}
	if _, err := New(client, nil, messageKeyARN, evidenceKeyARN); !errors.Is(err, errConfiguration) {
		t.Fatalf("nil random error = %v", err)
	}
	if _, err := New(client, bytes.NewReader(nil), "alias/message", evidenceKeyARN); !errors.Is(err, errConfiguration) {
		t.Fatalf("non-ARN key error = %v", err)
	}
	if _, err := New(client, bytes.NewReader(nil), messageKeyARN, messageKeyARN); !errors.Is(err, errConfiguration) {
		t.Fatalf("identical key error = %v", err)
	}

	letterID := testID(1)
	c := newTestCipher(t, client, bytes.NewReader(sequence(3*NonceBytes)))
	if _, err := c.EncryptOriginal(t.Context(), "not-an-id", []byte("letter")); !errors.Is(err, errMetadata) {
		t.Fatalf("invalid ID error = %v", err)
	}
	if _, err := c.EncryptEvidence(t.Context(), testID(2), letterID, Target("other"), []byte("letter")); !errors.Is(err, errMetadata) {
		t.Fatalf("invalid target error = %v", err)
	}
	if _, err := c.EncryptOriginal(t.Context(), letterID, nil); !errors.Is(err, errPlaintext) {
		t.Fatalf("empty plaintext error = %v", err)
	}
	if _, err := c.EncryptOriginal(t.Context(), letterID, make([]byte, MaxMessagePlaintextBytes+1)); !errors.Is(err, errPlaintext) {
		t.Fatalf("oversized message error = %v", err)
	}
	evidence, err := c.EncryptEvidence(t.Context(), testID(2), letterID, TargetOriginal, make([]byte, MaxEvidencePlaintextBytes))
	if err != nil {
		t.Fatalf("maximum evidence: %v", err)
	}
	if len(evidence.Ciphertext) != MaxEvidenceCiphertextBytes {
		t.Fatalf("evidence ciphertext length = %d, want %d", len(evidence.Ciphertext), MaxEvidenceCiphertextBytes)
	}
	if _, err := c.EncryptEvidence(t.Context(), testID(2), letterID, TargetOriginal, make([]byte, MaxEvidencePlaintextBytes+1)); !errors.Is(err, errPlaintext) {
		t.Fatalf("oversized evidence error = %v", err)
	}

	message, err := c.EncryptOriginal(t.Context(), letterID, []byte("letter"))
	if err != nil {
		t.Fatal(err)
	}
	invalidRecords := []Envelope{
		{Ciphertext: message.Ciphertext, Nonce: message.Nonce[:NonceBytes-1], WrappedKey: message.WrappedKey, KMSKeyARN: messageKeyARN, Version: EncryptionVersion},
		{Ciphertext: make([]byte, MaxMessageCiphertextBytes+1), Nonce: message.Nonce, WrappedKey: message.WrappedKey, KMSKeyARN: messageKeyARN, Version: EncryptionVersion},
		{Ciphertext: message.Ciphertext, Nonce: message.Nonce, WrappedKey: nil, KMSKeyARN: messageKeyARN, Version: EncryptionVersion},
		{Ciphertext: message.Ciphertext, Nonce: message.Nonce, WrappedKey: make([]byte, MaxWrappedKeyBytes+1), KMSKeyARN: messageKeyARN, Version: EncryptionVersion},
	}
	for i, record := range invalidRecords {
		if _, err := c.DecryptOriginal(t.Context(), letterID, record); !errors.Is(err, errRecord) {
			t.Fatalf("invalid record %d error = %v", i, err)
		}
	}

	c = newTestCipher(t, newSyntheticKMS(), io.LimitReader(bytes.NewReader(nil), 0))
	if _, err := c.EncryptOriginal(t.Context(), letterID, []byte("letter")); !errors.Is(err, errRandom) {
		t.Fatalf("random failure error = %v", err)
	}
}

func TestCanariesUseFixedContextsAndPermissions(t *testing.T) {
	client := newSyntheticKMS()
	c := newTestCipher(t, client, bytes.NewReader(nil))
	if err := c.CheckMessageKey(t.Context()); err != nil {
		t.Fatalf("message canary: %v", err)
	}
	if err := c.CheckEvidenceKey(t.Context()); err != nil {
		t.Fatalf("evidence canary: %v", err)
	}
	if len(client.generates) != 2 || len(client.decrypts) != 1 {
		t.Fatalf("canary calls = %d generate, %d decrypt", len(client.generates), len(client.decrypts))
	}
	if !maps.Equal(client.generates[0].encryptionContext, map[string]string{
		"service": "orifude", "schema": "1", "operation": "startup", "key_purpose": "message",
	}) || !maps.Equal(client.decrypts[0].encryptionContext, client.generates[0].encryptionContext) {
		t.Fatal("message canary context mismatch")
	}
	if !maps.Equal(client.generates[1].encryptionContext, map[string]string{
		"service": "orifude", "schema": "1", "operation": "startup", "key_purpose": "evidence",
		"purpose": "reported-content-review",
	}) {
		t.Fatal("evidence canary context mismatch")
	}

	client = newSyntheticKMS()
	client.decryptWrongKey = true
	c = newTestCipher(t, client, bytes.NewReader(nil))
	if err := c.CheckMessageKey(t.Context()); !errors.Is(err, errKMSResponse) {
		t.Fatalf("message canary wrong decrypted key = %v", err)
	}

	client = newSyntheticKMS()
	client.generateErr = errors.New("unavailable")
	c = newTestCipher(t, client, bytes.NewReader(nil))
	if err := c.CheckEvidenceKey(t.Context()); !errors.Is(err, errKMS) {
		t.Fatalf("evidence canary failure = %v", err)
	}
}

type syntheticKMS struct {
	records             map[string]syntheticKey
	generates           []generateRequest
	decrypts            []decryptRequest
	generateErr         error
	decryptErr          error
	generateARN         string
	decryptARN          string
	generateKeyBytes    int
	decryptKeyBytes     int
	decryptWrongKey     bool
	emptyWrappedKey     bool
	oversizedWrappedKey bool
	ignoreContext       bool
	next                byte
}

type syntheticKey struct {
	plaintext         []byte
	keyARN            string
	encryptionContext map[string]string
}

type generateRequest struct {
	keyARN            string
	keySpec           types.DataKeySpec
	encryptionContext map[string]string
}

type decryptRequest struct {
	keyARN            string
	encryptionContext map[string]string
}

func newSyntheticKMS() *syntheticKMS {
	return &syntheticKMS{records: make(map[string]syntheticKey)}
}

func (s *syntheticKMS) GenerateDataKey(_ context.Context, input *kms.GenerateDataKeyInput, _ ...func(*kms.Options)) (*kms.GenerateDataKeyOutput, error) {
	if s.generateErr != nil {
		return nil, s.generateErr
	}
	request := generateRequest{keySpec: input.KeySpec, encryptionContext: maps.Clone(input.EncryptionContext)}
	if input.KeyId != nil {
		request.keyARN = *input.KeyId
	}
	s.generates = append(s.generates, request)

	keyBytes := s.generateKeyBytes
	if keyBytes == 0 {
		keyBytes = DataKeyBytes
	}
	s.next++
	plaintext := bytes.Repeat([]byte{s.next}, keyBytes)
	wrapped := []byte{s.next, 0xa5}
	if s.emptyWrappedKey {
		wrapped = nil
	}
	if s.oversizedWrappedKey {
		wrapped = make([]byte, MaxWrappedKeyBytes+1)
	}
	keyARN := request.keyARN
	if s.generateARN != "" {
		keyARN = s.generateARN
	}
	if len(wrapped) > 0 {
		s.records[string(wrapped)] = syntheticKey{
			plaintext:         slices.Clone(plaintext),
			keyARN:            request.keyARN,
			encryptionContext: maps.Clone(input.EncryptionContext),
		}
	}
	return &kms.GenerateDataKeyOutput{
		Plaintext:      plaintext,
		CiphertextBlob: wrapped,
		KeyId:          &keyARN,
	}, nil
}

func (s *syntheticKMS) Decrypt(_ context.Context, input *kms.DecryptInput, _ ...func(*kms.Options)) (*kms.DecryptOutput, error) {
	if s.decryptErr != nil {
		return nil, s.decryptErr
	}
	request := decryptRequest{encryptionContext: maps.Clone(input.EncryptionContext)}
	if input.KeyId != nil {
		request.keyARN = *input.KeyId
	}
	s.decrypts = append(s.decrypts, request)

	record, ok := s.records[string(input.CiphertextBlob)]
	if !ok || request.keyARN != record.keyARN || (!s.ignoreContext && !maps.Equal(input.EncryptionContext, record.encryptionContext)) {
		return nil, errors.New("invalid synthetic ciphertext")
	}
	plaintext := slices.Clone(record.plaintext)
	if s.decryptKeyBytes != 0 {
		plaintext = make([]byte, s.decryptKeyBytes)
	}
	if s.decryptWrongKey {
		plaintext[0] ^= 1
	}
	keyARN := record.keyARN
	if s.decryptARN != "" {
		keyARN = s.decryptARN
	}
	return &kms.DecryptOutput{Plaintext: plaintext, KeyId: &keyARN}, nil
}

func newTestCipher(t *testing.T, client KMS, random io.Reader) *Cipher {
	t.Helper()
	c, err := New(client, random, messageKeyARN, evidenceKeyARN)
	if err != nil {
		t.Fatal(err)
	}
	return c
}

func testID(value byte) string {
	return base64.RawURLEncoding.EncodeToString(bytes.Repeat([]byte{value}, 16))
}

func sequence(size int) []byte {
	sequence := make([]byte, size)
	for i := range sequence {
		sequence[i] = byte(i)
	}
	return sequence
}

func cloneEnvelope(envelope Envelope) Envelope {
	envelope.Ciphertext = slices.Clone(envelope.Ciphertext)
	envelope.Nonce = slices.Clone(envelope.Nonce)
	envelope.WrappedKey = slices.Clone(envelope.WrappedKey)
	return envelope
}
