package api

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"mime"
	"net"
	"net/http"
	"net/netip"
	"net/url"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/jsonsafe"
)

const (
	defaultClientTimeout = 20 * time.Second
	maxResponseBytes     = 256 << 10
	maxRequestBytes      = 16 << 10
	sessionLifetime      = 15 * time.Minute
	sessionRenewalMargin = 15 * time.Second
)

var (
	ErrClientConfig     = errors.New("api: invalid client configuration")
	ErrTransport        = errors.New("api: transport failure")
	ErrProtocol         = errors.New("api: invalid server response")
	ErrResponseTooLarge = errors.New("api: server response is too large")
)

// HTTPError is a non-successful response carrying the stable API error.
type HTTPError struct {
	Status int
	API    APIError
}

func (e *HTTPError) Error() string {
	if e == nil {
		return "api request failed"
	}
	return e.API.Message
}

// Client owns one participant HTTP transport and exact API origin.
type Client struct {
	origin string
	http   *http.Client
}

// NewClient constructs a bounded participant client. A nil HTTP client uses a
// process-long standard transport.
func NewClient(origin string, provided *http.Client) (*Client, error) {
	parsed, err := url.Parse(origin)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" || parsed.User != nil || parsed.Path != "" ||
		parsed.RawQuery != "" || parsed.Fragment != "" || parsed.Scheme != "https" && (parsed.Scheme != "http" || !clientLoopback(parsed.Hostname())) {
		return nil, ErrClientConfig
	}
	var httpClient http.Client
	if provided == nil {
		transport := http.DefaultTransport.(*http.Transport).Clone()
		transport.MaxIdleConns = 16
		transport.MaxIdleConnsPerHost = 8
		transport.MaxConnsPerHost = 8
		transport.IdleConnTimeout = 90 * time.Second
		httpClient.Transport = transport
	} else {
		httpClient = *provided
	}
	if httpClient.Timeout <= 0 {
		httpClient.Timeout = defaultClientTimeout
	}
	httpClient.CheckRedirect = func(_ *http.Request, _ []*http.Request) error {
		return http.ErrUseLastResponse
	}
	return &Client{origin: parsed.Scheme + "://" + parsed.Host, http: &httpClient}, nil
}

func clientLoopback(host string) bool {
	if strings.EqualFold(host, "localhost") {
		return true
	}
	ip, err := netip.ParseAddr(host)
	return err == nil && ip.IsLoopback()
}

// DeviceClient binds one local P-256 key and in-memory session to a Client.
type DeviceClient struct {
	client *Client
	device *auth.DeviceKey
	prover *auth.Prover

	mu        sync.Mutex
	token     string
	expiresAt time.Time
	renewing  chan struct{}
	renewErr  error
}

// ForDevice creates a device-bound client without creating a session.
func (c *Client) ForDevice(device *auth.DeviceKey) (*DeviceClient, error) {
	if c == nil || device == nil {
		return nil, ErrClientConfig
	}
	prover, err := auth.NewProver(device, c.origin, rand.Reader, time.Now)
	if err != nil {
		return nil, err
	}
	return &DeviceClient{client: c, device: device, prover: prover}, nil
}

// PublicJWK returns the registration and session challenge key.
func (d *DeviceClient) PublicJWK() (PublicJWK, error) {
	if d == nil || d.device == nil {
		return PublicJWK{}, ErrClientConfig
	}
	x, y, err := d.device.PublicCoordinates()
	if err != nil {
		return PublicJWK{}, err
	}
	return PublicJWK{KeyType: "EC", Curve: "P-256", X: x, Y: y, Algorithm: "ES256"}, nil
}

// CreateChallenge creates a registration or session challenge for this device.
func (d *DeviceClient) CreateChallenge(ctx context.Context, purpose ChallengePurpose) (CreateChallengeResponse, error) {
	jwk, err := d.PublicJWK()
	if err != nil {
		return CreateChallengeResponse{}, err
	}
	return doJSON[CreateChallengeResponse](ctx, d.client, http.MethodPost, "/v1/auth/challenges", http.StatusCreated, CreateChallengeRequest{Purpose: purpose, PublicJWK: jwk}, nil)
}

// Register creates or idempotently recovers an identity from one challenge.
func (d *DeviceClient) Register(ctx context.Context, challenge CreateChallengeResponse, request CreateIdentityRequest) (CreateIdentityResponse, error) {
	proof, err := d.prover.ChallengeProof(http.MethodPost, "/v1/identities", challenge.Nonce)
	if err != nil {
		return CreateIdentityResponse{}, err
	}
	response, err := doJSON[CreateIdentityResponse](ctx, d.client, http.MethodPost, "/v1/identities", http.StatusCreated, request, map[string]string{"DPoP": proof})
	if err != nil {
		return CreateIdentityResponse{}, err
	}
	if err := d.acceptSession(response.TokenType, response.AccessToken, response.ExpiresIn); err != nil {
		return CreateIdentityResponse{}, err
	}
	return response, nil
}

// CreateSession creates a fresh challenge-bound access session.
func (d *DeviceClient) CreateSession(ctx context.Context) (CreateSessionResponse, error) {
	challenge, err := d.CreateChallenge(ctx, ChallengePurposeSession)
	if err != nil {
		return CreateSessionResponse{}, err
	}
	proof, err := d.prover.ChallengeProof(http.MethodPost, "/v1/sessions", challenge.Nonce)
	if err != nil {
		return CreateSessionResponse{}, err
	}
	response, err := doJSON[CreateSessionResponse](ctx, d.client, http.MethodPost, "/v1/sessions", http.StatusCreated, CreateSessionRequest{ChallengeID: challenge.ChallengeID}, map[string]string{"DPoP": proof})
	if err != nil {
		return CreateSessionResponse{}, err
	}
	if err := d.acceptSession(response.TokenType, response.AccessToken, response.ExpiresIn); err != nil {
		return CreateSessionResponse{}, err
	}
	return response, nil
}

func (d *DeviceClient) acceptSession(tokenType TokenType, token string, expiresIn int) error {
	if tokenType != TokenTypeDPoP || time.Duration(expiresIn)*time.Second != sessionLifetime {
		return ErrProtocol
	}
	decoded, err := authDecodeSecret(token)
	if err != nil || len(decoded) != 32 {
		return ErrProtocol
	}
	defer clear(decoded)
	d.mu.Lock()
	d.token = token
	d.expiresAt = time.Now().Add(time.Duration(expiresIn) * time.Second)
	d.renewErr = nil
	d.mu.Unlock()
	return nil
}

func authDecodeSecret(value string) ([]byte, error) {
	decoded, err := base64.RawURLEncoding.DecodeString(value)
	if err != nil || base64.RawURLEncoding.EncodeToString(decoded) != value {
		return nil, errors.New("invalid opaque secret")
	}
	return decoded, nil
}

// ClearSession removes the in-memory access token.
func (d *DeviceClient) ClearSession() {
	if d == nil {
		return
	}
	d.mu.Lock()
	d.token = ""
	d.expiresAt = time.Time{}
	d.renewErr = nil
	d.mu.Unlock()
}

func (d *DeviceClient) session(ctx context.Context) (string, error) {
	for {
		d.mu.Lock()
		if d.token != "" && time.Until(d.expiresAt) > sessionRenewalMargin {
			token := d.token
			d.mu.Unlock()
			return token, nil
		}
		if wait := d.renewing; wait != nil {
			d.mu.Unlock()
			select {
			case <-ctx.Done():
				return "", errors.Join(ErrTransport, ctx.Err())
			case <-wait:
			}
			d.mu.Lock()
			err := d.renewErr
			d.mu.Unlock()
			if err != nil {
				return "", err
			}
			continue
		}
		wait := make(chan struct{})
		d.renewing = wait
		d.renewErr = nil
		d.mu.Unlock()

		_, err := d.CreateSession(ctx)
		d.mu.Lock()
		d.renewErr = err
		d.renewing = nil
		close(wait)
		if err == nil {
			token := d.token
			d.mu.Unlock()
			return token, nil
		}
		d.mu.Unlock()
		return "", err
	}
}

func (d *DeviceClient) invalidateToken(token string) {
	d.mu.Lock()
	if d.token == token {
		d.token = ""
		d.expiresAt = time.Time{}
	}
	d.mu.Unlock()
}

// RevokeIdentity deletes an identity using its delete-only credential.
func (c *Client) RevokeIdentity(ctx context.Context, credential string) error {
	_, err := doJSON[struct{}](ctx, c, http.MethodPost, "/v1/identities/revoke", http.StatusNoContent, RevokeIdentityRequest{RevocationCredential: credential}, nil)
	return err
}

func (d *DeviceClient) Me(ctx context.Context) (GetMeResponse, error) {
	return doProtected[GetMeResponse](ctx, d, http.MethodGet, "/v1/me", nil)
}

func (d *DeviceClient) DeleteIdentity(ctx context.Context) error {
	_, err := doProtected[struct{}](ctx, d, http.MethodDelete, "/v1/me", nil)
	if err == nil {
		d.ClearSession()
	}
	return err
}

func (d *DeviceClient) CreateLetter(ctx context.Context, request CreateLetterRequest) (CreateLetterResponse, error) {
	return doProtected[CreateLetterResponse](ctx, d, http.MethodPost, "/v1/letters", request)
}

func (d *DeviceClient) ClaimLetter(ctx context.Context) (ClaimLetterResponse, error) {
	return doProtected[ClaimLetterResponse](ctx, d, http.MethodPost, "/v1/letters/claim", ClaimLetterRequest{})
}

func (d *DeviceClient) Letter(ctx context.Context, letterID string) (GetLetterResponse, error) {
	return doProtected[GetLetterResponse](ctx, d, http.MethodGet, "/v1/letters/"+url.PathEscape(letterID), nil)
}

func (d *DeviceClient) OpenLetter(ctx context.Context, letterID string) (OpenLetterResponse, error) {
	return doProtected[OpenLetterResponse](ctx, d, http.MethodPost, "/v1/letters/"+url.PathEscape(letterID)+"/open", OpenLetterRequest{})
}

func (d *DeviceClient) Reply(ctx context.Context, letterID string, request ReplyToLetterRequest) (ReplyToLetterResponse, error) {
	return doProtected[ReplyToLetterResponse](ctx, d, http.MethodPost, "/v1/letters/"+url.PathEscape(letterID)+"/reply", request)
}

func (d *DeviceClient) Withdraw(ctx context.Context, letterID string) (WithdrawLetterResponse, error) {
	return doProtected[WithdrawLetterResponse](ctx, d, http.MethodPost, "/v1/letters/"+url.PathEscape(letterID)+"/withdraw", WithdrawLetterRequest{})
}

func (d *DeviceClient) Report(ctx context.Context, letterID string, request ReportLetterRequest) (ReportLetterResponse, error) {
	return doProtected[ReportLetterResponse](ctx, d, http.MethodPost, "/v1/letters/"+url.PathEscape(letterID)+"/report", request)
}

func (d *DeviceClient) Block(ctx context.Context, letterID string) (BlockLetterResponse, error) {
	return doProtected[BlockLetterResponse](ctx, d, http.MethodPost, "/v1/letters/"+url.PathEscape(letterID)+"/block", BlockLetterRequest{})
}

func (d *DeviceClient) Keepsakes(ctx context.Context, cursor string, limit int) (ListKeepsakesResponse, error) {
	query := url.Values{}
	if cursor != "" {
		query.Set("cursor", cursor)
	}
	if limit != 0 {
		query.Set("limit", strconv.Itoa(limit))
	}
	path := "/v1/keepsakes"
	if encoded := query.Encode(); encoded != "" {
		path += "?" + encoded
	}
	return doProtected[ListKeepsakesResponse](ctx, d, http.MethodGet, path, nil)
}

func (d *DeviceClient) DeleteKeepsake(ctx context.Context, letterID string) error {
	_, err := doProtected[struct{}](ctx, d, http.MethodDelete, "/v1/keepsakes/"+url.PathEscape(letterID), nil)
	return err
}

func doProtected[T any](ctx context.Context, device *DeviceClient, method, path string, body any) (T, error) {
	var zero T
	if device == nil || device.client == nil || device.prover == nil {
		return zero, ErrClientConfig
	}
	escapedPath := path
	if index := strings.IndexByte(escapedPath, '?'); index >= 0 {
		escapedPath = escapedPath[:index]
	}
	for attempt := 0; attempt < 2; attempt++ {
		token, err := device.session(ctx)
		if err != nil {
			return zero, err
		}
		proof, err := device.prover.ResourceProof(method, escapedPath, token)
		if err != nil {
			return zero, err
		}
		response, err := doJSON[T](ctx, device.client, method, path, participantStatus(method, path), body, map[string]string{
			"Authorization": "DPoP " + token,
			"DPoP":          proof,
		})
		var httpError *HTTPError
		if attempt == 0 && errors.As(err, &httpError) && httpError.API.Code == ErrorCodeSessionExpired {
			device.invalidateToken(token)
			continue
		}
		return response, err
	}
	return zero, ErrProtocol
}

func doJSON[T any](ctx context.Context, client *Client, method, path string, expectedStatus int, body any, headers map[string]string) (T, error) {
	var zero T
	if client == nil || client.http == nil || !strings.HasPrefix(path, "/") {
		return zero, ErrClientConfig
	}
	var data []byte
	var err error
	if body != nil {
		data, err = json.Marshal(body)
		if err != nil {
			return zero, errors.Join(ErrProtocol, err)
		}
		if len(data) > maxRequestBytes {
			return zero, ErrClientConfig
		}
	}
	request, err := http.NewRequestWithContext(ctx, method, client.origin+path, bytes.NewReader(data))
	if err != nil {
		return zero, errors.Join(ErrClientConfig, err)
	}
	if body != nil {
		request.Header.Set("Content-Type", "application/json")
	}
	request.Header.Set("Accept", "application/json")
	for name, value := range headers {
		request.Header.Set(name, value)
	}
	response, err := client.http.Do(request)
	if err != nil {
		if errors.Is(err, context.DeadlineExceeded) || errors.Is(err, context.Canceled) || isNetworkError(err) {
			return zero, errors.Join(ErrTransport, err)
		}
		return zero, errors.Join(ErrTransport, err)
	}
	defer response.Body.Close()
	responseData, err := io.ReadAll(io.LimitReader(response.Body, maxResponseBytes+1))
	if err != nil {
		return zero, errors.Join(ErrTransport, err)
	}
	if len(responseData) > maxResponseBytes {
		return zero, ErrResponseTooLarge
	}
	if response.StatusCode < http.StatusOK || response.StatusCode >= http.StatusMultipleChoices {
		var envelope ErrorResponse
		if err := decodeResponse(response, responseData, &envelope); err != nil {
			return zero, err
		}
		if envelope.Error.Code == "" || envelope.Error.Message == "" {
			return zero, ErrProtocol
		}
		return zero, &HTTPError{Status: response.StatusCode, API: envelope.Error}
	}
	if response.StatusCode != expectedStatus {
		return zero, ErrProtocol
	}
	if expectedStatus == http.StatusNoContent {
		if len(responseData) != 0 {
			return zero, ErrProtocol
		}
		return zero, nil
	}
	if len(responseData) == 0 {
		return zero, ErrProtocol
	}
	var value T
	if err := decodeResponse(response, responseData, &value); err != nil {
		return zero, err
	}
	return value, nil
}

func participantStatus(method, path string) int {
	if method == http.MethodDelete {
		return http.StatusNoContent
	}
	if method == http.MethodPost && (path == "/v1/letters" || strings.HasSuffix(path, "/reply") || strings.HasSuffix(path, "/report")) {
		return http.StatusCreated
	}
	return http.StatusOK
}

func decodeResponse(response *http.Response, data []byte, target any) error {
	mediaType, parameters, err := mime.ParseMediaType(response.Header.Get("Content-Type"))
	charset, hasCharset := parameters["charset"]
	if err != nil || mediaType != "application/json" || len(parameters) != 0 && (len(parameters) != 1 || !hasCharset || !strings.EqualFold(charset, "utf-8")) {
		return ErrProtocol
	}
	if err := jsonsafe.Validate(data); err != nil {
		return ErrProtocol
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(target); err != nil {
		return errors.Join(ErrProtocol, err)
	}
	if err := decoder.Decode(&struct{}{}); err != io.EOF {
		return ErrProtocol
	}
	return nil
}

func isNetworkError(err error) bool {
	var networkError net.Error
	return errors.As(err, &networkError)
}
