package httpapi

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"mime"
	"net"
	"net/http"
	"net/netip"
	"net/url"
	"reflect"
	"strconv"
	"strings"
	"sync/atomic"
	"time"
	"unicode/utf8"

	"github.com/go-chi/chi/v5"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/postoffice"
)

const (
	letterRequestBytes = 16 << 10
	smallRequestBytes  = 4 << 10
	tinyRequestBytes   = 1 << 10
)

type Ready interface {
	Ready(context.Context) error
}

type Config struct {
	Logger           *slog.Logger
	ModerationOrigin string
	TrustedProxies   []netip.Prefix
	DatabaseTimeout  time.Duration
	OperationTimeout time.Duration
}

type contextKey uint8

const (
	requestIDKey contextKey = iota
	principalKey
	moderatorKey
	requestLogKey
	forwardedSchemeKey
	forwardedHostKey
)

type server struct {
	service          *postoffice.Service
	ready            Ready
	access           *AccessVerifier
	logger           *slog.Logger
	moderationOrigin string
	trustedProxies   []netip.Prefix
	databaseTimeout  time.Duration
	operationTimeout time.Duration
}

func New(service *postoffice.Service, ready Ready, access *AccessVerifier, config Config) (http.Handler, error) {
	if service == nil || ready == nil || access == nil {
		return nil, errors.New("httpapi: missing dependency")
	}
	origin, err := url.Parse(config.ModerationOrigin)
	if err != nil || (origin.Scheme != "https" && origin.Scheme != "http") || origin.Host == "" || origin.User != nil ||
		origin.Path != "" || origin.RawQuery != "" || origin.Fragment != "" || origin.Scheme == "http" && !loopbackHost(origin.Hostname()) {
		return nil, errors.New("httpapi: invalid moderation origin")
	}
	if config.Logger == nil {
		config.Logger = slog.Default()
	}
	if config.DatabaseTimeout <= 0 {
		config.DatabaseTimeout = 5 * time.Second
	}
	if config.OperationTimeout <= 0 {
		config.OperationTimeout = 15 * time.Second
	}
	s := &server{
		service: service, ready: ready, access: access, logger: config.Logger,
		moderationOrigin: origin.Scheme + "://" + origin.Host, trustedProxies: config.TrustedProxies,
		databaseTimeout: config.DatabaseTimeout, operationTimeout: config.OperationTimeout,
	}

	router := chi.NewRouter()
	router.Use(s.requestID, s.trustedProxy, s.recover, s.accessLog, securityHeaders, rejectDuplicateCredentials)
	router.NotFound(func(w http.ResponseWriter, _ *http.Request) {
		writeError(w, api.ErrorCodeNotFound, "The requested resource was not found.", http.StatusNotFound)
	})
	router.MethodNotAllowed(func(w http.ResponseWriter, _ *http.Request) {
		writeError(w, api.ErrorCodeInvalidRequest, "The request method is not allowed.", http.StatusMethodNotAllowed)
	})

	router.With(s.timeout(s.databaseTimeout)).Get("/healthz", s.health)
	router.With(s.timeout(s.databaseTimeout)).Get("/readyz", s.readiness)
	router.Route("/v1", func(r chi.Router) {
		r.With(s.timeout(s.databaseTimeout), bodyLimit(smallRequestBytes)).Post("/auth/challenges", s.createChallenge)
		r.With(s.timeout(s.databaseTimeout), bodyLimit(smallRequestBytes)).Post("/identities", s.createIdentity)
		r.With(s.timeout(s.databaseTimeout), bodyLimit(tinyRequestBytes)).Post("/sessions", s.createSession)
		r.With(s.timeout(s.databaseTimeout), bodyLimit(tinyRequestBytes)).Post("/identities/revoke", s.revokeIdentity)

		r.With(s.timeout(s.databaseTimeout), s.participant).Get("/me", s.me)
		r.With(s.timeout(s.databaseTimeout), s.participant).Delete("/me", s.deleteIdentity)
		r.With(s.timeout(s.operationTimeout), bodyLimit(letterRequestBytes), s.participant).Post("/letters", s.createLetter)
		r.With(s.timeout(s.databaseTimeout), bodyLimit(tinyRequestBytes), s.participant).Post("/letters/claim", s.claimLetter)
		r.With(s.timeout(s.operationTimeout), s.participant).Get("/letters/{id}", s.getLetter)
		r.With(s.timeout(s.operationTimeout), bodyLimit(tinyRequestBytes), s.participant).Post("/letters/{id}/open", s.openLetter)
		r.With(s.timeout(s.operationTimeout), bodyLimit(letterRequestBytes), s.participant).Post("/letters/{id}/reply", s.replyLetter)
		r.With(s.timeout(s.databaseTimeout), bodyLimit(tinyRequestBytes), s.participant).Post("/letters/{id}/withdraw", s.withdrawLetter)
		r.With(s.timeout(s.operationTimeout), bodyLimit(letterRequestBytes), s.participant).Post("/letters/{id}/report", s.reportLetter)
		r.With(s.timeout(s.databaseTimeout), bodyLimit(tinyRequestBytes), s.participant).Post("/letters/{id}/block", s.blockLetter)
		r.With(s.timeout(s.databaseTimeout), s.participant).Get("/keepsakes", s.keepsakes)
		r.With(s.timeout(s.databaseTimeout), s.participant).Delete("/keepsakes/{id}", s.deleteKeepsake)
	})
	router.Route("/moderation/v1", func(r chi.Router) {
		r.With(s.timeout(s.operationTimeout), bodyLimit(tinyRequestBytes), s.moderation).Post("/reports/next/claim", s.claimNextReport)
		r.With(s.timeout(s.operationTimeout), bodyLimit(tinyRequestBytes), s.moderation).Post("/reports/{id}/review", s.reviewReport)
		r.With(s.timeout(s.databaseTimeout), bodyLimit(tinyRequestBytes), s.moderation).Post("/reports/{id}/close", s.closeReport)
	})
	return router, nil
}

func (s *server) createChallenge(w http.ResponseWriter, r *http.Request) {
	var request api.CreateChallengeRequest
	if !decode(w, r, &request) {
		return
	}
	response, err := s.service.CreateChallenge(r.Context(), request)
	respond(w, response, err, http.StatusCreated)
}

func (s *server) createIdentity(w http.ResponseWriter, r *http.Request) {
	proof, ok := oneHeader(r, "DPoP")
	if !ok {
		writeError(w, api.ErrorCodeInvalidProof, "A valid device proof is required.", http.StatusUnauthorized)
		return
	}
	var request api.CreateIdentityRequest
	if !decode(w, r, &request) {
		return
	}
	response, err := s.service.Register(r.Context(), request, proof)
	respond(w, response, err, http.StatusCreated)
}

func (s *server) createSession(w http.ResponseWriter, r *http.Request) {
	proof, ok := oneHeader(r, "DPoP")
	if !ok {
		writeError(w, api.ErrorCodeInvalidProof, "A valid device proof is required.", http.StatusUnauthorized)
		return
	}
	var request api.CreateSessionRequest
	if !decode(w, r, &request) {
		return
	}
	response, err := s.service.CreateSession(r.Context(), request, proof)
	respond(w, response, err, http.StatusCreated)
}

func (s *server) revokeIdentity(w http.ResponseWriter, r *http.Request) {
	var request api.RevokeIdentityRequest
	if !decode(w, r, &request) {
		return
	}
	if err := s.service.RevokeIdentity(r.Context(), request.RevocationCredential); err != nil {
		respondError(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func (s *server) me(w http.ResponseWriter, r *http.Request) {
	response, err := s.service.Me(r.Context(), principal(r))
	respond(w, response, err, http.StatusOK)
}

func (s *server) deleteIdentity(w http.ResponseWriter, r *http.Request) {
	if err := s.service.DeleteIdentity(r.Context(), principal(r)); err != nil {
		respondError(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func (s *server) createLetter(w http.ResponseWriter, r *http.Request) {
	var request api.CreateLetterRequest
	if !decode(w, r, &request) {
		return
	}
	response, err := s.service.SendLetter(r.Context(), principal(r), request)
	respond(w, response, err, http.StatusCreated)
}

func (s *server) claimLetter(w http.ResponseWriter, r *http.Request) {
	if !decode(w, r, &api.ClaimLetterRequest{}) {
		return
	}
	response, err := s.service.ClaimLetter(r.Context(), principal(r))
	respond(w, response, err, http.StatusOK)
}

func (s *server) getLetter(w http.ResponseWriter, r *http.Request) {
	response, err := s.service.GetLetter(r.Context(), principal(r), chi.URLParam(r, "id"))
	respond(w, response, err, http.StatusOK)
}

func (s *server) openLetter(w http.ResponseWriter, r *http.Request) {
	if !decode(w, r, &api.OpenLetterRequest{}) {
		return
	}
	response, err := s.service.OpenLetter(r.Context(), principal(r), chi.URLParam(r, "id"))
	respond(w, response, err, http.StatusOK)
}

func (s *server) replyLetter(w http.ResponseWriter, r *http.Request) {
	var request api.ReplyToLetterRequest
	if !decode(w, r, &request) {
		return
	}
	response, err := s.service.ReplyToLetter(r.Context(), principal(r), chi.URLParam(r, "id"), request)
	respond(w, response, err, http.StatusCreated)
}

func (s *server) withdrawLetter(w http.ResponseWriter, r *http.Request) {
	if !decode(w, r, &api.WithdrawLetterRequest{}) {
		return
	}
	response, err := s.service.WithdrawLetter(r.Context(), principal(r), chi.URLParam(r, "id"))
	respond(w, response, err, http.StatusOK)
}

func (s *server) reportLetter(w http.ResponseWriter, r *http.Request) {
	var request api.ReportLetterRequest
	if !decode(w, r, &request) {
		return
	}
	response, err := s.service.ReportLetter(r.Context(), principal(r), chi.URLParam(r, "id"), request)
	respond(w, response, err, http.StatusCreated)
}

func (s *server) blockLetter(w http.ResponseWriter, r *http.Request) {
	if !decode(w, r, &api.BlockLetterRequest{}) {
		return
	}
	response, err := s.service.BlockLetter(r.Context(), principal(r), chi.URLParam(r, "id"))
	respond(w, response, err, http.StatusOK)
}

func (s *server) keepsakes(w http.ResponseWriter, r *http.Request) {
	query := r.URL.Query()
	for name, values := range query {
		if (name != "cursor" && name != "limit") || len(values) != 1 {
			writeError(w, api.ErrorCodeInvalidRequest, "The request is invalid.", http.StatusBadRequest)
			return
		}
	}
	request := api.ListKeepsakesRequest{Cursor: query.Get("cursor")}
	if value := query.Get("limit"); value != "" {
		limit, err := strconv.Atoi(value)
		if err != nil {
			writeError(w, api.ErrorCodeInvalidRequest, "The request is invalid.", http.StatusBadRequest)
			return
		}
		request.Limit = limit
	}
	response, err := s.service.ListKeepsakes(r.Context(), principal(r), request)
	respond(w, response, err, http.StatusOK)
}

func (s *server) deleteKeepsake(w http.ResponseWriter, r *http.Request) {
	if err := s.service.DeleteKeepsake(r.Context(), principal(r), chi.URLParam(r, "id")); err != nil {
		respondError(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func (s *server) claimNextReport(w http.ResponseWriter, r *http.Request) {
	var request api.ClaimNextReportRequest
	if !decode(w, r, &request) {
		return
	}
	response, err := s.service.ClaimNextReport(r.Context(), moderator(r), request)
	respond(w, response, err, http.StatusOK)
}

func (s *server) reviewReport(w http.ResponseWriter, r *http.Request) {
	var request api.ReviewReportRequest
	if !decode(w, r, &request) {
		return
	}
	response, err := s.service.ReviewReport(r.Context(), moderator(r), chi.URLParam(r, "id"), request)
	respond(w, response, err, http.StatusOK)
}

func (s *server) closeReport(w http.ResponseWriter, r *http.Request) {
	var request api.CloseReportRequest
	if !decode(w, r, &request) {
		return
	}
	response, err := s.service.CloseReport(r.Context(), moderator(r), chi.URLParam(r, "id"), request)
	respond(w, response, err, http.StatusOK)
}

func (s *server) health(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, api.HealthResponse{Status: "ok"}, http.StatusOK)
}

func (s *server) readiness(w http.ResponseWriter, r *http.Request) {
	if err := s.ready.Ready(r.Context()); err != nil {
		writeError(w, api.ErrorCodeServiceUnavailable, "The post office is temporarily unavailable.", http.StatusServiceUnavailable)
		return
	}
	writeJSON(w, api.ReadinessResponse{Status: "ready"}, http.StatusOK)
}

func decode(w http.ResponseWriter, r *http.Request, target any) bool {
	mediaType, parameters, err := mime.ParseMediaType(r.Header.Get("Content-Type"))
	charset, hasCharset := parameters["charset"]
	if err != nil || mediaType != "application/json" || len(parameters) != 0 && (len(parameters) != 1 || !hasCharset || !strings.EqualFold(charset, "utf-8")) {
		writeError(w, api.ErrorCodeInvalidRequest, "Content-Type must be application/json.", http.StatusUnsupportedMediaType)
		return false
	}
	data, err := io.ReadAll(r.Body)
	if err != nil {
		var tooLarge *http.MaxBytesError
		if errors.As(err, &tooLarge) {
			writeError(w, api.ErrorCodeInvalidRequest, "The request body is too large.", http.StatusRequestEntityTooLarge)
		} else {
			writeError(w, api.ErrorCodeInvalidRequest, "The request body is invalid.", http.StatusBadRequest)
		}
		return false
	}
	if err := validateUniqueJSON(data); err != nil {
		writeError(w, api.ErrorCodeInvalidRequest, "The request body is invalid.", http.StatusBadRequest)
		return false
	}
	if err := validateExactJSONFields(data, reflect.TypeOf(target)); err != nil {
		writeError(w, api.ErrorCodeInvalidRequest, "The request body is invalid.", http.StatusBadRequest)
		return false
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(target); err != nil {
		writeError(w, api.ErrorCodeInvalidRequest, "The request body is invalid.", http.StatusBadRequest)
		return false
	}
	if err := decoder.Decode(&struct{}{}); err != io.EOF {
		writeError(w, api.ErrorCodeInvalidRequest, "The request body is invalid.", http.StatusBadRequest)
		return false
	}
	return true
}

func validateExactJSONFields(data []byte, target reflect.Type) error {
	for target.Kind() == reflect.Pointer {
		target = target.Elem()
	}
	if target.Kind() != reflect.Struct {
		return nil
	}
	var object map[string]json.RawMessage
	if err := json.Unmarshal(data, &object); err != nil {
		return err
	}
	if object == nil {
		return errors.New("expected JSON object")
	}
	fields := make(map[string]reflect.Type, target.NumField())
	for i := range target.NumField() {
		field := target.Field(i)
		if !field.IsExported() {
			continue
		}
		name := strings.Split(field.Tag.Get("json"), ",")[0]
		if name == "-" {
			continue
		}
		if name == "" {
			name = field.Name
		}
		fields[name] = field.Type
	}
	for name, raw := range object {
		field, ok := fields[name]
		if !ok {
			return errors.New("unknown JSON field")
		}
		if err := validateExactJSONFields(raw, field); err != nil {
			return err
		}
	}
	return nil
}

func validateUniqueJSON(data []byte) error {
	if !utf8.Valid(data) {
		return errors.New("invalid UTF-8")
	}
	if err := validateJSONSurrogates(data); err != nil {
		return err
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	if err := uniqueJSON(decoder); err != nil {
		return err
	}
	if _, err := decoder.Token(); err != io.EOF {
		if err != nil {
			return err
		}
		return errors.New("trailing JSON value")
	}
	return nil
}

func validateJSONSurrogates(data []byte) error {
	for index := 0; index < len(data); index++ {
		if data[index] != '"' {
			continue
		}
		for index++; index < len(data) && data[index] != '"'; index++ {
			if data[index] != '\\' {
				continue
			}
			index++
			if index >= len(data) || data[index] != 'u' {
				continue
			}
			value, ok := jsonHexQuad(data, index+1)
			if !ok {
				continue
			}
			index += 4
			switch {
			case value >= 0xd800 && value <= 0xdbff:
				if index+6 >= len(data) || data[index+1] != '\\' || data[index+2] != 'u' {
					return errors.New("unpaired high surrogate")
				}
				low, ok := jsonHexQuad(data, index+3)
				if !ok || low < 0xdc00 || low > 0xdfff {
					return errors.New("unpaired high surrogate")
				}
				index += 6
			case value >= 0xdc00 && value <= 0xdfff:
				return errors.New("unpaired low surrogate")
			}
		}
	}
	return nil
}

func jsonHexQuad(data []byte, start int) (uint16, bool) {
	if start+4 > len(data) {
		return 0, false
	}
	var value uint16
	for _, digit := range data[start : start+4] {
		value <<= 4
		switch {
		case digit >= '0' && digit <= '9':
			value |= uint16(digit - '0')
		case digit >= 'a' && digit <= 'f':
			value |= uint16(digit-'a') + 10
		case digit >= 'A' && digit <= 'F':
			value |= uint16(digit-'A') + 10
		default:
			return 0, false
		}
	}
	return value, true
}

func uniqueJSON(decoder *json.Decoder) error {
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	delimiter, ok := token.(json.Delim)
	if !ok {
		return nil
	}
	switch delimiter {
	case '{':
		fields := make(map[string]struct{})
		for decoder.More() {
			name, err := decoder.Token()
			if err != nil {
				return err
			}
			field, ok := name.(string)
			if !ok {
				return errors.New("invalid object field")
			}
			if _, duplicate := fields[field]; duplicate {
				return errors.New("duplicate object field")
			}
			fields[field] = struct{}{}
			if err := uniqueJSON(decoder); err != nil {
				return err
			}
		}
	case '[':
		for decoder.More() {
			if err := uniqueJSON(decoder); err != nil {
				return err
			}
		}
	default:
		return errors.New("invalid JSON delimiter")
	}
	_, err = decoder.Token()
	return err
}

func respond[T any](w http.ResponseWriter, response T, err error, status int) {
	if err != nil {
		respondError(w, err)
		return
	}
	writeJSON(w, response, status)
}

func respondError(w http.ResponseWriter, err error) {
	switch {
	case errors.Is(err, auth.ErrProofClockSkew):
		writeError(w, api.ErrorCodeClockSkew, "The device clock is outside the allowed window.", http.StatusUnauthorized)
	case errors.Is(err, auth.ErrInvalidProof):
		writeError(w, api.ErrorCodeInvalidProof, "The device proof is invalid.", http.StatusUnauthorized)
	case errors.Is(err, postoffice.ErrAuthentication):
		writeError(w, api.ErrorCodeAuthenticationFailed, "Authentication failed.", http.StatusUnauthorized)
	case errors.Is(err, postoffice.ErrSessionExpired):
		writeError(w, api.ErrorCodeSessionExpired, "The access session has expired.", http.StatusUnauthorized)
	case errors.Is(err, postoffice.ErrReplay):
		writeError(w, api.ErrorCodeDPoPReplay, "This device proof has already been used.", http.StatusUnauthorized)
	case errors.Is(err, postoffice.ErrInvalid):
		writeError(w, api.ErrorCodeInvalidRequest, "The request is invalid.", http.StatusBadRequest)
	case errors.Is(err, postoffice.ErrAliasInvalid):
		writeError(w, api.ErrorCodeAliasInvalid, "The alias is invalid.", http.StatusBadRequest)
	case errors.Is(err, postoffice.ErrInviteInvalid):
		writeError(w, api.ErrorCodeInviteInvalid, "The invite is invalid.", http.StatusUnauthorized)
	case errors.Is(err, postoffice.ErrIdentityConflict):
		writeError(w, api.ErrorCodeIdentityConflict, "The identity conflicts with existing state.", http.StatusConflict)
	case errors.Is(err, postoffice.ErrNotFound), errors.Is(err, postoffice.ErrNoLetters), errors.Is(err, postoffice.ErrNoReports):
		writeError(w, api.ErrorCodeNotFound, "The requested resource was not found.", http.StatusNotFound)
	case errors.Is(err, postoffice.ErrRateLimited):
		writeError(w, api.ErrorCodeRateLimited, "The request limit has been reached.", http.StatusTooManyRequests)
	case errors.Is(err, postoffice.ErrAlreadyReplied):
		writeError(w, api.ErrorCodeLetterAlreadyReplied, "This letter already has a reply.", http.StatusConflict)
	case errors.Is(err, postoffice.ErrClaimExpired):
		writeError(w, api.ErrorCodeClaimExpired, "This claim has expired.", http.StatusConflict)
	case errors.Is(err, postoffice.ErrReportExists):
		writeError(w, api.ErrorCodeReportAlreadyExists, "This letter has already been reported.", http.StatusConflict)
	case errors.Is(err, postoffice.ErrEvidenceExpired):
		writeError(w, api.ErrorCodeEvidenceExpired, "The report evidence has expired.", http.StatusGone)
	case errors.Is(err, postoffice.ErrReportNotReviewed):
		writeError(w, api.ErrorCodeReportNotReviewed, "The report has not been reviewed.", http.StatusConflict)
	case errors.Is(err, postoffice.ErrReportClosed):
		writeError(w, api.ErrorCodeReportAlreadyClosed, "The report is already closed.", http.StatusConflict)
	case errors.Is(err, postoffice.ErrConflict):
		writeError(w, api.ErrorCodeConflict, "The request conflicts with existing state.", http.StatusConflict)
	default:
		writeError(w, api.ErrorCodeServiceUnavailable, "The post office is temporarily unavailable.", http.StatusServiceUnavailable)
	}
}

func writeJSON(w http.ResponseWriter, value any, status int) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
}

func writeError(w http.ResponseWriter, code api.ErrorCode, message string, status int) {
	writeJSON(w, api.ErrorResponse{Error: api.APIError{Code: code, Message: message}}, status)
}

func (s *server) participant(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		authorization, authOK := oneHeader(r, "Authorization")
		proof, proofOK := oneHeader(r, "DPoP")
		if !authOK || !proofOK || !strings.HasPrefix(authorization, "DPoP ") || strings.ContainsAny(strings.TrimPrefix(authorization, "DPoP "), " \t,") {
			writeError(w, api.ErrorCodeAuthenticationFailed, "Authentication failed.", http.StatusUnauthorized)
			return
		}
		principal, err := s.service.Authenticate(r.Context(), strings.TrimPrefix(authorization, "DPoP "), proof, r.Method, r.URL.EscapedPath())
		if err != nil {
			respondError(w, err)
			return
		}
		if state, ok := r.Context().Value(requestLogKey).(*requestLog); ok {
			state.identityID.Store(principal.IdentityID)
		}
		next.ServeHTTP(w, r.WithContext(context.WithValue(r.Context(), principalKey, principal)))
	})
}

func (s *server) moderation(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodOptions {
			writeError(w, api.ErrorCodeInvalidRequest, "The request method is not allowed.", http.StatusMethodNotAllowed)
			return
		}
		moderationPurpose, purposeOK := oneHeader(r, "X-Orifude-Moderation")
		if requestOrigin(r) != s.moderationOrigin || !purposeOK || moderationPurpose != string(api.ModerationPurposeReportedContentReview) {
			writeError(w, api.ErrorCodeAuthenticationFailed, "Authentication failed.", http.StatusForbidden)
			return
		}
		assertion, ok := oneHeader(r, "Cf-Access-Jwt-Assertion")
		if !ok {
			writeError(w, api.ErrorCodeAuthenticationFailed, "Authentication failed.", http.StatusUnauthorized)
			return
		}
		subject, err := s.access.Verify(r.Context(), assertion)
		if err != nil {
			writeError(w, api.ErrorCodeAuthenticationFailed, "Authentication failed.", http.StatusUnauthorized)
			return
		}
		next.ServeHTTP(w, r.WithContext(context.WithValue(r.Context(), moderatorKey, subject)))
	})
}

func oneHeader(r *http.Request, name string) (string, bool) {
	values := r.Header.Values(name)
	returnValue := ""
	if len(values) == 1 {
		returnValue = values[0]
	}
	return returnValue, len(values) == 1 && returnValue != "" && !strings.Contains(returnValue, ",")
}

func rejectDuplicateCredentials(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		for _, name := range []string{"Authorization", "DPoP", "Cf-Access-Jwt-Assertion"} {
			values := r.Header.Values(name)
			if len(values) > 1 || len(values) == 1 && strings.Contains(values[0], ",") {
				writeError(w, api.ErrorCodeInvalidRequest, "Duplicate credential headers are not allowed.", http.StatusBadRequest)
				return
			}
		}
		next.ServeHTTP(w, r)
	})
}

func bodyLimit(limit int64) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			r.Body = http.MaxBytesReader(w, r.Body, limit)
			next.ServeHTTP(w, r)
		})
	}
}

func (s *server) timeout(duration time.Duration) func(http.Handler) http.Handler {
	const message = `{"error":{"code":"service_unavailable","message":"The post office is temporarily unavailable."}}` + "\n"
	return func(next http.Handler) http.Handler {
		handler := http.TimeoutHandler(next, duration, message)
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "application/json")
			handler.ServeHTTP(w, r)
		})
	}
}

func securityHeaders(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("X-Frame-Options", "DENY")
		w.Header().Set("Referrer-Policy", "no-referrer")
		w.Header().Set("Content-Security-Policy", "default-src 'none'; frame-ancestors 'none'")
		if strings.HasPrefix(r.URL.Path, "/v1/") || strings.HasPrefix(r.URL.Path, "/moderation/") {
			w.Header().Set("Cache-Control", "no-store")
		}
		next.ServeHTTP(w, r)
	})
}

func (s *server) requestID(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		id := rand.Text()
		w.Header().Set("X-Request-ID", id)
		next.ServeHTTP(w, r.WithContext(context.WithValue(r.Context(), requestIDKey, id)))
	})
}

func (s *server) trustedProxy(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		peer := remoteIP(r.RemoteAddr)
		if !contains(s.trustedProxies, peer) {
			next.ServeHTTP(w, r)
			return
		}
		ctx := r.Context()
		if values := r.Header.Values("X-Forwarded-Proto"); len(values) == 1 && (values[0] == "https" || values[0] == "http") {
			ctx = context.WithValue(ctx, forwardedSchemeKey, values[0])
		}
		if values := r.Header.Values("X-Forwarded-Host"); len(values) == 1 && values[0] != "" && !strings.ContainsAny(values[0], ",/ ") {
			ctx = context.WithValue(ctx, forwardedHostKey, values[0])
		}
		forwarded := r.Header.Values("X-Forwarded-For")
		if len(forwarded) == 1 {
			current := peer
			parts := strings.Split(forwarded[0], ",")
			for i := len(parts) - 1; i >= 0 && contains(s.trustedProxies, current); i-- {
				candidate, err := netip.ParseAddr(strings.TrimSpace(parts[i]))
				if err != nil {
					break
				}
				current = candidate.Unmap()
			}
			if current.IsValid() {
				r.RemoteAddr = net.JoinHostPort(current.String(), "0")
			}
		}
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

func requestOrigin(r *http.Request) string {
	scheme := "http"
	if r.TLS != nil {
		scheme = "https"
	}
	if forwarded, ok := r.Context().Value(forwardedSchemeKey).(string); ok {
		scheme = forwarded
	}
	host := r.Host
	if forwarded, ok := r.Context().Value(forwardedHostKey).(string); ok {
		host = forwarded
	}
	return scheme + "://" + host
}

func remoteIP(value string) netip.Addr {
	host, _, err := net.SplitHostPort(value)
	if err != nil {
		host = value
	}
	ip, _ := netip.ParseAddr(host)
	return ip.Unmap()
}

func contains(prefixes []netip.Prefix, ip netip.Addr) bool {
	if !ip.IsValid() {
		return false
	}
	for _, prefix := range prefixes {
		if prefix.Contains(ip) {
			return true
		}
	}
	return false
}

func loopbackHost(host string) bool {
	if strings.EqualFold(host, "localhost") {
		return true
	}
	ip, err := netip.ParseAddr(host)
	return err == nil && ip.IsLoopback()
}

type responseStatus struct {
	http.ResponseWriter
	status int
}

type requestLog struct {
	identityID atomic.Int64
}

func (w *responseStatus) WriteHeader(status int) {
	if w.status != 0 {
		return
	}
	w.status = status
	w.ResponseWriter.WriteHeader(status)
}

func (w *responseStatus) Write(data []byte) (int, error) {
	if w.status == 0 {
		w.status = http.StatusOK
	}
	return w.ResponseWriter.Write(data)
}

func (s *server) accessLog(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		started := time.Now()
		status := &responseStatus{ResponseWriter: w}
		state := &requestLog{}
		completed := false
		defer func() {
			if !completed || status.status == 0 {
				return
			}
			attributes := []any{"request_id", requestID(r), "route", routePattern(r), "status", status.status, "duration_ms", time.Since(started).Milliseconds()}
			if identityID := state.identityID.Load(); identityID != 0 {
				attributes = append(attributes, "identity_id", identityID)
			}
			s.logger.InfoContext(r.Context(), "http request", attributes...)
		}()
		next.ServeHTTP(status, r.WithContext(context.WithValue(r.Context(), requestLogKey, state)))
		completed = true
	})
}

func (s *server) recover(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		started := time.Now()
		status := &responseStatus{ResponseWriter: w}
		defer func() {
			if recover() == nil {
				return
			}
			if status.status == 0 {
				writeError(status, api.ErrorCodeServiceUnavailable, "The post office is temporarily unavailable.", http.StatusInternalServerError)
			}
			s.logger.ErrorContext(r.Context(), "http request", "request_id", requestID(r), "route", routePattern(r), "status", status.status,
				"duration_ms", time.Since(started).Milliseconds(), "panic_recovered", true)
		}()
		next.ServeHTTP(status, r)
	})
}

func routePattern(r *http.Request) string {
	if context := chi.RouteContext(r.Context()); context != nil {
		if pattern := context.RoutePattern(); pattern != "" {
			return pattern
		}
	}
	return "unmatched"
}

func requestID(r *http.Request) string {
	value, _ := r.Context().Value(requestIDKey).(string)
	return value
}

func principal(r *http.Request) postoffice.Principal {
	value, _ := r.Context().Value(principalKey).(postoffice.Principal)
	return value
}

func moderator(r *http.Request) string {
	value, _ := r.Context().Value(moderatorKey).(string)
	return value
}
