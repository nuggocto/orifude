//go:build integration

package httpapi

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/creack/pty"
	"github.com/go-jose/go-jose/v4"
	"github.com/jackc/pgx/v5"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/database"
	"github.com/nuggocto/orifude/internal/envelope"
	"github.com/nuggocto/orifude/internal/identity"
	"github.com/nuggocto/orifude/internal/postoffice"
	"github.com/nuggocto/orifude/internal/tui"
)

func TestOnlineTUITwoIdentityJourneyAndLostIdentityDeletion(t *testing.T) {
	databaseURL := os.Getenv("TEST_DATABASE_URL")
	if databaseURL == "" {
		t.Skip("TEST_DATABASE_URL is set by testdata/postgres/check.sh")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 45*time.Second)
	t.Cleanup(cancel)
	raw, err := pgx.Connect(ctx, databaseURL)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = raw.Close(context.Background()) })
	if _, err := raw.Exec(ctx, `TRUNCATE moderation_audit, reports, blocks, rate_limit_events, dpop_replays,
		access_sessions, auth_challenges, letters, invites, identities, alias_reservations RESTART IDENTITY CASCADE`); err != nil {
		t.Fatal(err)
	}
	db, err := database.Open(ctx, databaseURL, 8)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(db.Close)

	server := newTUIHTTPServer(t, db)
	invite := httpSecret(71)
	inviteHash := auth.HashOpaque(invite)
	if _, err := db.Queries().CreateInvite(ctx, inviteHash[:]); err != nil {
		t.Fatal(err)
	}
	client, err := api.NewClient(server.URL, server.Client())
	if err != nil {
		t.Fatal(err)
	}
	senderConfig := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", senderConfig)
	t.Setenv("DBUS_SESSION_BUS_ADDRESS", "unix:path=/dev/null")
	store, err := identity.NewStore()
	if err != nil {
		t.Fatal(err)
	}
	runtime := &tui.Runtime{Client: client, Store: store, Version: "v0.3.0-test"}

	terminal := startTUI(t, tui.NewOnline(runtime))
	terminal.wait("Create an identity")
	terminal.input("\r")
	terminal.wait("A quiet post office")
	terminal.input("\r")
	terminal.wait("invite code")
	terminal.input(invite)
	terminal.input("\r")
	terminal.input("willow")
	terminal.input("\r")
	terminal.input("y")
	which := terminal.waitAny("owner-only local file", "Save this delete-only credential")
	if which == 0 {
		terminal.input("y")
		terminal.wait("Save this delete-only credential")
	}
	terminal.wait("I saved it")
	terminal.input("\r")
	terminal.wait("I stored it safely")
	terminal.input("y")
	terminal.wait("Welcome to the branch, willow")
	terminal.input("\r")
	terminal.wait("Text mode. Press esc")
	terminal.input("A real post office letter.")
	terminal.input("\x1b")
	terminal.input("\r")
	terminal.wait("Release this folded letter")
	terminal.input("y")
	terminal.wait("letter was released")
	terminal.sendFresh("q")
	terminal.waitDone()

	terminal = startTUI(t, tui.NewOnline(runtime))
	terminal.wait("Welcome to the branch, willow")
	terminal.sendFresh("q")
	terminal.waitDone()

	recipientInvite := httpSecret(74)
	recipientInviteHash := auth.HashOpaque(recipientInvite)
	if _, err := db.Queries().CreateInvite(ctx, recipientInviteHash[:]); err != nil {
		t.Fatal(err)
	}
	recipientConfig := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", recipientConfig)
	recipientStore, err := identity.NewStore()
	if err != nil {
		t.Fatal(err)
	}
	recipientRuntime := &tui.Runtime{Client: client, Store: recipientStore, Version: "v0.3.0-test"}
	terminal = startTUI(t, tui.NewOnline(recipientRuntime))
	terminal.wait("Create an identity")
	terminal.input("\r")
	terminal.wait("A quiet post office")
	terminal.input("\r")
	terminal.wait("invite code")
	terminal.input(recipientInvite)
	terminal.input("\r")
	terminal.input("cedar")
	terminal.input("\r")
	terminal.input("y")
	if terminal.waitAny("owner-only local file", "Save this delete-only credential") == 0 {
		terminal.input("y")
		terminal.wait("Save this delete-only credential")
	}
	terminal.wait("I saved it")
	terminal.input("\r")
	terminal.wait("I stored it safely")
	terminal.input("y")
	terminal.wait("Welcome to the branch, cedar")
	terminal.input("j")
	terminal.input("\r")
	terminal.wait("A folded letter arrived")
	terminal.input("\r")
	terminal.wait("A real post office letter.")
	terminal.input("\r")
	terminal.wait("Text mode. Press esc")
	terminal.input("A reply from the second installation.")
	terminal.input("\x1b")
	terminal.input("\r")
	terminal.wait("Release this one reply")
	terminal.input("y")
	terminal.wait("reply was folded into the keepsake")
	terminal.input("\r")
	terminal.input("j")
	terminal.input("j")
	terminal.input("\r")
	terminal.wait("willow")
	terminal.sendFresh("q")
	terminal.waitDone()

	t.Setenv("XDG_CONFIG_HOME", senderConfig)
	store, err = identity.NewStore()
	if err != nil {
		t.Fatal(err)
	}
	runtime.Store = store
	terminal = startTUI(t, tui.NewOnline(runtime))
	terminal.wait("Welcome to the branch, willow")
	terminal.input("j")
	terminal.input("j")
	terminal.input("\r")
	terminal.wait("cedar")
	terminal.input("\r")
	terminal.wait("A reply from the second installation.")
	terminal.input("j")
	terminal.input("\r")
	terminal.wait("Block future matching permanently")
	terminal.input("y")
	terminal.wait("blocked permanently")
	terminal.input("k")
	terminal.input("\r")
	terminal.wait("Why are you reporting")
	terminal.input("\r")
	terminal.wait("Report and burn")
	terminal.input("y")
	terminal.wait("reported, burned, and blocked")
	terminal.sendFresh("q")
	terminal.waitDone()

	t.Setenv("XDG_CONFIG_HOME", recipientConfig)
	recipientStore, err = identity.NewStore()
	if err != nil {
		t.Fatal(err)
	}
	recipientRuntime.Store = recipientStore
	terminal = startTUI(t, tui.NewOnline(recipientRuntime))
	terminal.wait("Welcome to the branch, cedar")
	terminal.sendFresh("q")
	terminal.waitDone()

	credential := httpSecret(72)
	registerHTTPIdentity(t, db, server, httpDeviceKey(t), "lostwillow", credential, 73)
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	store, err = identity.NewStore()
	if err != nil {
		t.Fatal(err)
	}
	runtime.Store = store
	terminal = startTUI(t, tui.NewOnline(runtime))
	terminal.wait("Delete a lost identity")
	terminal.input("j")
	terminal.input("\r")
	terminal.wait("revocation credential")
	terminal.input(credential)
	terminal.input("\r")
	terminal.wait("Delete identity")
	terminal.input("y")
	terminal.wait("delete request was accepted")
	terminal.sendFresh("q")
	terminal.waitDone()

	var deleted int
	if err := raw.QueryRow(ctx, `SELECT count(*) FROM identities WHERE deleted_at IS NOT NULL`).Scan(&deleted); err != nil || deleted != 1 {
		t.Fatalf("deleted identities = %d, %v", deleted, err)
	}
}

func newTUIHTTPServer(t *testing.T, db *database.DB) *httptest.Server {
	t.Helper()
	accessKey := httpRSAKey(t)
	certs := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(w).Encode(jose.JSONWebKeySet{Keys: []jose.JSONWebKey{{
			Key: &accessKey.PublicKey, KeyID: "access", Algorithm: string(jose.RS256), Use: "sig",
		}}})
	}))
	t.Cleanup(certs.Close)
	access, err := NewAccessVerifier(certs.URL, "moderation-audience")
	if err != nil {
		t.Fatal(err)
	}
	server := httptest.NewUnstartedServer(nil)
	origin := "http://" + server.Listener.Addr().String()
	verifier, err := auth.NewVerifier(origin)
	if err != nil {
		t.Fatal(err)
	}
	cipher, err := envelope.New(newHTTPKMS(), rand.Reader, httpMessageKeyARN, httpEvidenceKeyARN)
	if err != nil {
		t.Fatal(err)
	}
	config := postoffice.DefaultConfig()
	config.LatestTUIVersion = "v0.3.1-test"
	service, err := postoffice.New(db, verifier, cipher, config)
	if err != nil {
		t.Fatal(err)
	}
	handler, err := New(service, db, access, Config{
		Logger: slog.New(slog.NewTextHandler(io.Discard, nil)), ModerationOrigin: origin,
	})
	if err != nil {
		t.Fatal(err)
	}
	server.Config.Handler = handler
	server.Start()
	t.Cleanup(server.Close)
	return server
}

type tuiTerminal struct {
	t       *testing.T
	master  *os.File
	done    <-chan error
	mu      sync.Mutex
	output  bytes.Buffer
	timeout time.Duration
}

func startTUI(t *testing.T, model tui.Model) *tuiTerminal {
	t.Helper()
	master, slave, err := pty.Open()
	if err != nil {
		t.Fatal(err)
	}
	if err := pty.Setsize(master, &pty.Winsize{Rows: 30, Cols: 100}); err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	program := tea.NewProgram(model, tea.WithInput(slave), tea.WithOutput(slave))
	go func() {
		_, runErr := program.Run()
		_ = slave.Close()
		done <- runErr
	}()
	terminal := &tuiTerminal{t: t, master: master, done: done, timeout: 12 * time.Second}
	t.Cleanup(func() { _ = master.Close() })
	go func() {
		buffer := make([]byte, 4096)
		for {
			count, readErr := master.Read(buffer)
			if count > 0 {
				terminal.mu.Lock()
				_, _ = terminal.output.Write(buffer[:count])
				terminal.mu.Unlock()
			}
			if readErr != nil {
				return
			}
		}
	}()
	return terminal
}

func (terminal *tuiTerminal) send(value string) {
	terminal.t.Helper()
	if _, err := io.WriteString(terminal.master, value); err != nil {
		terminal.t.Fatal(err)
	}
}

func (terminal *tuiTerminal) sendFresh(value string) {
	terminal.mu.Lock()
	terminal.output.Reset()
	terminal.mu.Unlock()
	terminal.send(value)
}

func (terminal *tuiTerminal) input(value string) {
	terminal.sendFresh(value)
	terminal.waitQuiet()
}

func (terminal *tuiTerminal) wait(value string) { terminal.waitAny(value) }

func (terminal *tuiTerminal) waitQuiet() {
	terminal.t.Helper()
	deadline := time.Now().Add(terminal.timeout)
	lastLength := -1
	stableSince := time.Now()
	for time.Now().Before(deadline) {
		terminal.mu.Lock()
		length := terminal.output.Len()
		terminal.mu.Unlock()
		if length != lastLength {
			lastLength = length
			stableSince = time.Now()
		}
		if length > 0 && time.Since(stableSince) >= 75*time.Millisecond {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	terminal.t.Fatal("timed out waiting for terminal input to settle")
}

func (terminal *tuiTerminal) waitAny(values ...string) int {
	terminal.t.Helper()
	deadline := time.Now().Add(terminal.timeout)
	for time.Now().Before(deadline) {
		terminal.mu.Lock()
		output := terminal.output.String()
		terminal.mu.Unlock()
		plain := ansi.Strip(output)
		for index, value := range values {
			if strings.Contains(output, value) || strings.Contains(plain, value) {
				return index
			}
		}
		select {
		case err := <-terminal.done:
			terminal.t.Fatalf("TUI exited before %q: %v\n%s", values, err, output)
		default:
		}
		time.Sleep(10 * time.Millisecond)
	}
	terminal.mu.Lock()
	defer terminal.mu.Unlock()
	terminal.t.Fatalf("timed out waiting for %q\n%s", values, terminal.output.String())
	return -1
}

func (terminal *tuiTerminal) waitDone() {
	terminal.t.Helper()
	select {
	case err := <-terminal.done:
		if err != nil {
			terminal.t.Fatal(err)
		}
	case <-time.After(terminal.timeout):
		terminal.t.Fatal("timed out waiting for TUI exit")
	}
}
