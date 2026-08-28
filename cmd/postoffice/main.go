package main

import (
	"context"
	"crypto/rand"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"net/netip"
	"net/url"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"

	awsconfig "github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/service/kms"
	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/database"
	"github.com/nuggocto/orifude/internal/envelope"
	"github.com/nuggocto/orifude/internal/httpapi"
	"github.com/nuggocto/orifude/internal/postoffice"
)

const (
	databaseMaxConns = 10
	cleanupBatchSize = 1000
	startupTimeout   = 30 * time.Second
	shutdownTimeout  = 15 * time.Second
)

type config struct {
	databaseURL      string
	listenAddr       string
	publicOrigin     string
	moderationOrigin string
	awsRegion        string
	messageKeyARN    string
	evidenceKeyARN   string
	accessIssuer     string
	accessAudience   string
	latestTUIVersion string
	trustedProxies   []netip.Prefix
	logLevel         slog.Level
}

func main() {
	logger := slog.New(slog.NewJSONHandler(os.Stderr, nil))
	slog.SetDefault(logger)
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	if err := run(ctx, os.Args[1:]); err != nil {
		logger.Error("post office stopped", "error", err)
		os.Exit(1)
	}
}

func run(ctx context.Context, args []string) error {
	if len(args) > 1 || len(args) == 1 && args[0] != "cleanup" {
		return errors.New("usage: postoffice [cleanup]")
	}
	settings, err := loadConfig()
	if err != nil {
		return err
	}
	logger := slog.New(slog.NewJSONHandler(os.Stderr, &slog.HandlerOptions{Level: settings.logLevel}))
	slog.SetDefault(logger)

	verifier, err := auth.NewVerifier(settings.publicOrigin)
	if err != nil {
		return errors.New("invalid PUBLIC_ORIGIN")
	}
	access, err := httpapi.NewAccessVerifier(settings.accessIssuer, settings.accessAudience)
	if err != nil {
		return errors.New("invalid Cloudflare Access configuration")
	}

	startupCtx, cancelStartup := context.WithTimeout(ctx, startupTimeout)
	defer cancelStartup()
	aws, err := awsconfig.LoadDefaultConfig(startupCtx, awsconfig.WithRegion(settings.awsRegion))
	if err != nil {
		return fmt.Errorf("load AWS configuration: %w", err)
	}
	cipher, err := envelope.New(kms.NewFromConfig(aws), rand.Reader, settings.messageKeyARN, settings.evidenceKeyARN)
	if err != nil {
		return errors.New("invalid KMS key configuration")
	}
	db, err := database.Open(startupCtx, settings.databaseURL, databaseMaxConns)
	if err != nil {
		return errors.New("database startup check failed")
	}
	defer db.Close()

	serviceConfig := postoffice.DefaultConfig()
	serviceConfig.LatestTUIVersion = settings.latestTUIVersion
	service, err := postoffice.New(db, verifier, cipher, serviceConfig)
	if err != nil {
		return fmt.Errorf("create post office service: %w", err)
	}
	if len(args) == 1 {
		result, err := service.Cleanup(ctx, cleanupBatchSize)
		if err != nil {
			return fmt.Errorf("cleanup: %w", err)
		}
		logger.Info("post office cleanup complete",
			"challenges", result.Challenges, "sessions", result.Sessions, "replays", result.Replays,
			"claims", result.Claims, "waiting_letters", result.WaitingLetters, "withdrawn", result.Withdrawn,
			"identities", result.Identities, "evidence", result.Evidence, "reports", result.Reports,
			"audits", result.Audits, "rate_events", result.RateEvents)
		return nil
	}
	if err := cipher.CheckMessageKey(startupCtx); err != nil {
		return errors.New("message KMS startup canary failed")
	}
	if err := cipher.CheckEvidenceKey(startupCtx); err != nil {
		return errors.New("evidence KMS startup canary failed")
	}
	handler, err := httpapi.New(service, db, access, httpapi.Config{
		Logger: logger, ModerationOrigin: settings.moderationOrigin, TrustedProxies: settings.trustedProxies,
	})
	if err != nil {
		return fmt.Errorf("create HTTP handler: %w", err)
	}

	listener, err := net.Listen("tcp", settings.listenAddr)
	if err != nil {
		return fmt.Errorf("listen: %w", err)
	}
	logger.Info("post office listening", "listen_addr", settings.listenAddr)
	return serve(ctx, listener, handler, logger)
}

func serve(ctx context.Context, listener net.Listener, handler http.Handler, logger *slog.Logger) error {
	server := &http.Server{
		Addr:              listener.Addr().String(),
		Handler:           handler,
		ReadHeaderTimeout: 5 * time.Second,
		ReadTimeout:       20 * time.Second,
		WriteTimeout:      20 * time.Second,
		IdleTimeout:       60 * time.Second,
		MaxHeaderBytes:    32 << 10,
		ErrorLog:          slog.NewLogLogger(logger.Handler(), slog.LevelError),
	}
	serveErr := make(chan error, 1)
	go func() { serveErr <- server.Serve(listener) }()

	select {
	case err := <-serveErr:
		if errors.Is(err, http.ErrServerClosed) {
			return nil
		}
		return fmt.Errorf("serve HTTP: %w", err)
	case <-ctx.Done():
	}

	shutdownCtx, cancelShutdown := context.WithTimeout(context.Background(), shutdownTimeout)
	defer cancelShutdown()
	if err := server.Shutdown(shutdownCtx); err != nil {
		_ = server.Close()
		return fmt.Errorf("shutdown HTTP server: %w", err)
	}
	if err := <-serveErr; err != nil && !errors.Is(err, http.ErrServerClosed) {
		return fmt.Errorf("serve HTTP: %w", err)
	}
	logger.Info("post office stopped")
	return nil
}

func loadConfig() (config, error) {
	values := make(map[string]string)
	for _, name := range []string{
		"DATABASE_URL", "LISTEN_ADDR", "PUBLIC_ORIGIN", "MODERATION_ORIGIN", "AWS_REGION",
		"MESSAGE_KMS_KEY_ARN", "EVIDENCE_KMS_KEY_ARN", "AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY",
		"CF_ACCESS_ISSUER", "CF_ACCESS_AUDIENCE", "LATEST_TUI_VERSION", "LOG_LEVEL",
	} {
		value, ok := os.LookupEnv(name)
		if !ok || value == "" || strings.TrimSpace(value) != value {
			return config{}, fmt.Errorf("%s must be set to a non-empty value", name)
		}
		values[name] = value
	}
	if values["MESSAGE_KMS_KEY_ARN"] == values["EVIDENCE_KMS_KEY_ARN"] {
		return config{}, errors.New("MESSAGE_KMS_KEY_ARN and EVIDENCE_KMS_KEY_ARN must differ")
	}
	if err := validateListenAddr(values["LISTEN_ADDR"]); err != nil {
		return config{}, err
	}
	for _, name := range []string{"PUBLIC_ORIGIN", "MODERATION_ORIGIN", "CF_ACCESS_ISSUER"} {
		if err := validateOrigin(values[name]); err != nil {
			return config{}, fmt.Errorf("%s must be an exact HTTP or HTTPS origin", name)
		}
	}
	var level slog.Level
	if err := level.UnmarshalText([]byte(values["LOG_LEVEL"])); err != nil {
		return config{}, errors.New("LOG_LEVEL must be debug, info, warn, or error")
	}
	proxies, err := trustedProxies(os.Getenv("TRUSTED_PROXY_CIDRS"))
	if err != nil {
		return config{}, err
	}
	return config{
		databaseURL: values["DATABASE_URL"], listenAddr: values["LISTEN_ADDR"],
		publicOrigin: values["PUBLIC_ORIGIN"], moderationOrigin: values["MODERATION_ORIGIN"],
		awsRegion: values["AWS_REGION"], messageKeyARN: values["MESSAGE_KMS_KEY_ARN"], evidenceKeyARN: values["EVIDENCE_KMS_KEY_ARN"],
		accessIssuer: values["CF_ACCESS_ISSUER"], accessAudience: values["CF_ACCESS_AUDIENCE"],
		latestTUIVersion: values["LATEST_TUI_VERSION"], trustedProxies: proxies, logLevel: level,
	}, nil
}

func validateOrigin(value string) error {
	origin, err := url.Parse(value)
	if err != nil || (origin.Scheme != "https" && origin.Scheme != "http") || origin.Host == "" || origin.User != nil ||
		origin.Path != "" || origin.RawQuery != "" || origin.Fragment != "" || origin.Scheme == "http" && !loopbackHost(origin.Hostname()) {
		return errors.New("invalid origin")
	}
	return nil
}

func loopbackHost(host string) bool {
	if strings.EqualFold(host, "localhost") {
		return true
	}
	ip, err := netip.ParseAddr(host)
	return err == nil && ip.IsLoopback()
}

func validateListenAddr(address string) error {
	_, port, err := net.SplitHostPort(address)
	if err != nil {
		return errors.New("LISTEN_ADDR must include a host and port")
	}
	number, err := strconv.Atoi(port)
	if err != nil || number < 1 || number > 65535 {
		return errors.New("LISTEN_ADDR must use a port from 1 to 65535")
	}
	return nil
}

func trustedProxies(value string) ([]netip.Prefix, error) {
	if value == "" {
		return nil, nil
	}
	parts := strings.Split(value, ",")
	prefixes := make([]netip.Prefix, 0, len(parts))
	for _, part := range parts {
		if part == "" || strings.TrimSpace(part) != part {
			return nil, errors.New("TRUSTED_PROXY_CIDRS must be a comma-separated CIDR list without spaces")
		}
		prefix, err := netip.ParsePrefix(part)
		if err != nil || prefix.Bits() == 0 {
			return nil, errors.New("TRUSTED_PROXY_CIDRS contains an invalid or unrestricted CIDR")
		}
		prefixes = append(prefixes, prefix.Masked())
	}
	return prefixes, nil
}
