package httpapi

import (
	"context"
	"crypto/rsa"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/netip"
	"net/url"
	"strings"
	"sync"
	"time"

	"github.com/go-jose/go-jose/v4"
	"github.com/go-jose/go-jose/v4/jwt"
)

const (
	maxAccessTokenBytes  = 16 << 10
	maxCertsBodyBytes    = 64 << 10
	accessKeyTTL         = time.Hour
	accessRefreshBackoff = time.Minute
)

var ErrAccessDenied = errors.New("httpapi: Cloudflare Access denied")

type AccessVerifier struct {
	issuer   string
	audience string
	certsURL string
	client   *http.Client
	now      func() time.Time

	mu           sync.Mutex
	keys         map[string]*rsa.PublicKey
	expires      time.Time
	refreshDone  chan struct{}
	refreshAfter time.Time
}

func NewAccessVerifier(issuer, audience string) (*AccessVerifier, error) {
	parsed, err := url.Parse(issuer)
	if err != nil || (parsed.Scheme != "https" && parsed.Scheme != "http") || parsed.Host == "" || parsed.User != nil ||
		(parsed.Path != "" && parsed.Path != "/") || parsed.RawQuery != "" || parsed.Fragment != "" || audience == "" {
		return nil, errors.New("httpapi: invalid Cloudflare Access configuration")
	}
	if parsed.Scheme == "http" && parsed.Hostname() != "localhost" {
		ip, err := netip.ParseAddr(parsed.Hostname())
		if err != nil || !ip.IsLoopback() {
			return nil, errors.New("httpapi: Cloudflare Access issuer must use HTTPS")
		}
	}
	origin := parsed.Scheme + "://" + parsed.Host
	return &AccessVerifier{
		issuer:   issuer,
		audience: audience,
		certsURL: origin + "/cdn-cgi/access/certs",
		client: &http.Client{
			Timeout: 5 * time.Second,
			CheckRedirect: func(*http.Request, []*http.Request) error {
				return http.ErrUseLastResponse
			},
		},
		now: time.Now,
	}, nil
}

func (v *AccessVerifier) Verify(ctx context.Context, raw string) (string, error) {
	if len(raw) == 0 || len(raw) > maxAccessTokenBytes {
		return "", ErrAccessDenied
	}
	parts := strings.Split(raw, ".")
	if len(parts) != 3 {
		return "", ErrAccessDenied
	}
	for _, part := range parts[:2] {
		data, err := base64.RawURLEncoding.DecodeString(part)
		if err != nil || validateUniqueJSON(data) != nil {
			return "", ErrAccessDenied
		}
	}
	token, err := jwt.ParseSigned(raw, []jose.SignatureAlgorithm{jose.RS256})
	if err != nil || len(token.Headers) != 1 || token.Headers[0].Algorithm != string(jose.RS256) || token.Headers[0].KeyID == "" {
		return "", ErrAccessDenied
	}
	key, err := v.key(ctx, token.Headers[0].KeyID)
	if err != nil {
		return "", ErrAccessDenied
	}
	var claims struct {
		jwt.Claims
		Type string `json:"type"`
	}
	if err := token.Claims(key, &claims); err != nil {
		return "", ErrAccessDenied
	}
	now := v.now().UTC()
	if claims.Type != "app" || claims.Subject == "" || claims.IssuedAt == nil || claims.Expiry == nil ||
		claims.IssuedAt.Time().After(now) || !claims.Expiry.Time().After(now) || claims.Expiry.Time().Before(claims.IssuedAt.Time()) ||
		claims.NotBefore != nil && claims.NotBefore.Time().After(now) ||
		claims.Validate(jwt.Expected{Issuer: v.issuer, AnyAudience: jwt.Audience{v.audience}, Time: now}) != nil {
		return "", ErrAccessDenied
	}
	return claims.Subject, nil
}

func (v *AccessVerifier) key(ctx context.Context, kid string) (*rsa.PublicKey, error) {
	for {
		v.mu.Lock()
		now := v.now()
		if now.Before(v.expires) {
			if key := v.keys[kid]; key != nil {
				v.mu.Unlock()
				return key, nil
			}
		}
		if now.Before(v.refreshAfter) {
			v.mu.Unlock()
			return nil, ErrAccessDenied
		}
		if done := v.refreshDone; done != nil {
			v.mu.Unlock()
			select {
			case <-ctx.Done():
				return nil, ctx.Err()
			case <-done:
				continue
			}
		}
		done := make(chan struct{})
		v.refreshDone = done
		v.mu.Unlock()

		keys, err := v.fetchKeys(ctx)
		completedAt := v.now()
		v.mu.Lock()
		if err == nil {
			v.keys = keys
			v.expires = completedAt.Add(accessKeyTTL)
		}
		key := keys[kid]
		if err != nil || key == nil {
			v.refreshAfter = completedAt.Add(accessRefreshBackoff)
		} else {
			v.refreshAfter = time.Time{}
		}
		v.refreshDone = nil
		close(done)
		v.mu.Unlock()

		if err != nil {
			return nil, err
		}
		if key == nil {
			return nil, ErrAccessDenied
		}
		return key, nil
	}
}

func (v *AccessVerifier) fetchKeys(ctx context.Context) (map[string]*rsa.PublicKey, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, v.certsURL, nil)
	if err != nil {
		return nil, err
	}
	response, err := v.client.Do(request)
	if err != nil {
		return nil, err
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("Access certs status %d", response.StatusCode)
	}
	data, err := io.ReadAll(io.LimitReader(response.Body, maxCertsBodyBytes+1))
	if err != nil || len(data) > maxCertsBodyBytes {
		return nil, errors.New("httpapi: invalid Cloudflare Access certs response")
	}
	var set jose.JSONWebKeySet
	if err := validateUniqueJSON(data); err != nil {
		return nil, errors.New("httpapi: invalid Cloudflare Access certs response")
	}
	if err := json.Unmarshal(data, &set); err != nil {
		return nil, errors.New("httpapi: invalid Cloudflare Access certs response")
	}
	keys := make(map[string]*rsa.PublicKey, len(set.Keys))
	for i := range set.Keys {
		jwk := &set.Keys[i]
		key, ok := jwk.Key.(*rsa.PublicKey)
		if !ok || jwk.KeyID == "" || !jwk.Valid() || (jwk.Algorithm != "" && jwk.Algorithm != string(jose.RS256)) ||
			(jwk.Use != "" && jwk.Use != "sig") || keys[jwk.KeyID] != nil {
			return nil, errors.New("httpapi: invalid Cloudflare Access certs response")
		}
		keys[jwk.KeyID] = key
	}
	if len(keys) == 0 {
		return nil, errors.New("httpapi: empty Cloudflare Access certs response")
	}
	return keys, nil
}
