package tui

import (
	"strings"
	"testing"
	"time"

	"github.com/nuggocto/orifude/internal/api"
)

func TestOnlineMutationIgnoresDuplicatesAndStaleResults(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.screen = ScreenFoldPreview
	m.draft.SetValue("still local")
	pending := m.beginOperation(operationSend, true)
	pending.clientID = "client-id"
	pending.body = m.draft.Value()

	next, command := m.activate()
	m = next.(Model)
	if command != nil || m.pending == nil || !m.pending.busy {
		t.Fatal("duplicate activation changed an active mutation")
	}

	nextModel, _, handled := m.handleOnlineMessage(sendMsg{id: pending.id + 1, response: api.CreateLetterResponse{LetterID: "wrong"}})
	if !handled || nextModel.draft.Value() != "still local" || nextModel.current != nil {
		t.Fatal("stale mutation result changed current state")
	}

	m.screen = ScreenCompose
	nextModel, _, _ = m.handleOnlineMessage(sendMsg{id: pending.id, response: api.CreateLetterResponse{LetterID: "late"}})
	if nextModel.draft.Value() != "still local" || nextModel.current != nil {
		t.Fatal("result from a departed screen changed current state")
	}
}

func TestOnlineFailuresPreserveDraftAndRetryIdentifier(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.screen = ScreenFoldPreview
	m.draft.SetValue("preserve me")
	pending := m.beginOperation(operationSend, true)
	pending.clientID = "same-client-id"
	pending.body = m.draft.Value()
	err := &api.HTTPError{Status: 503, API: api.APIError{Code: api.ErrorCodeServiceUnavailable, Message: "unavailable"}}

	next, _, _ := m.handleOnlineMessage(sendMsg{id: pending.id, err: err})
	if next.draft.Value() != "preserve me" || next.pending == nil || next.pending.clientID != "same-client-id" || next.pending.busy {
		t.Fatalf("recoverable failure lost retry state: %+v", next.pending)
	}
	if next.screen != ScreenFoldPreview {
		t.Fatalf("failure moved to screen %v", next.screen)
	}
}

func TestAmbiguousRegistrationCannotAbandonDeviceKey(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.screen = ScreenRevocation
	m.registration = &pendingRegistration{uncertain: true}
	m.back()
	if m.screen != ScreenRevocation || m.registration == nil {
		t.Fatal("ambiguous registration abandoned its device state")
	}
}

func TestOnlineOpenUsesRealResponseAndReducedMotion(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.reducedMotion = true
	m.screen = ScreenFoldedDelivery
	m.current = &Letter{ID: "letter", FoldSeed: 42, State: api.LetterStateClaimed}
	pending := m.beginOperation(operationOpen, true)
	response := api.OpenLetterResponse{
		LetterID: "letter",
		OpenedAt: time.Now(),
		Original: api.Message{Alias: "mori", Body: "hello from the server", CreatedAt: time.Now()},
	}

	next, command, handled := m.handleOnlineMessage(openMsg{id: pending.id, response: response})
	if !handled || command != nil || next.screen != ScreenRead {
		t.Fatalf("open handled=%v command=%v screen=%v", handled, command, next.screen)
	}
	if next.current == nil || next.current.Body != response.Original.Body || next.current.SenderAlias != response.Original.Alias {
		t.Fatalf("opened letter = %+v", next.current)
	}
}

func TestVisibleAPIErrorsAreActionable(t *testing.T) {
	tests := []struct {
		err  error
		want string
	}{
		{api.ErrTransport, "post office could not be reached"},
		{&api.HTTPError{API: api.APIError{Code: api.ErrorCodeClockSkew}}, "device's clock"},
		{&api.HTTPError{API: api.APIError{Code: api.ErrorCodeClaimExpired}}, "claim expired"},
		{&api.HTTPError{API: api.APIError{Code: api.ErrorCodeRateLimited}}, "limit has been reached"},
	}
	for _, test := range tests {
		if got := visibleAPIError(test.err); !strings.Contains(strings.ToLower(got), strings.ToLower(test.want)) {
			t.Errorf("visibleAPIError(%v) = %q, want %q", test.err, got, test.want)
		}
	}
}

func TestOnlineKeepsakeDetailRetainsSelectionAndActions(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.screen = ScreenKeepsakes
	m.cursor = 0
	m.keepsakes = []LetterSummary{{ID: "letter", Role: api.LetterRoleSender, Direction: "sent"}}
	pending := m.beginOperation(operationLetter, false)
	response := api.GetLetterResponse{
		LetterID: "letter", Role: api.LetterRoleSender, State: api.LetterStateReplied,
		OtherAlias: "mori", FoldSeed: 4, CreatedAt: time.Now(),
		Original: &api.Message{Alias: "willow", Body: "original"},
		Reply:    &api.Message{Alias: "mori", Body: "reply"},
	}

	next, _, _ := m.handleOnlineMessage(letterMsg{id: pending.id, index: 0, response: response})
	if next.screen != ScreenKeepsakeDetail || next.keepsakeIndex != 0 || !next.keepsakeReportable() || next.keepsakeReportTarget() != "reply" {
		t.Fatalf("keepsake state = screen %v index %d actions %v target %q", next.screen, next.keepsakeIndex, next.detailActions(), next.keepsakeReportTarget())
	}
}

func TestDiscardedOnlineLetterReturnsToBranch(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.screen = ScreenRead
	m.current = &Letter{ID: "letter", Role: api.LetterRoleRecipient}
	pending := m.beginOperation(operationDeleteKeepsake, true)

	next := m.handleDeleteKeepsake(deleteKeepsakeMsg{id: pending.id})
	if next.screen != ScreenBranch || next.current != nil {
		t.Fatalf("discard returned to screen %v with current %+v", next.screen, next.current)
	}
}

func TestUpdateNoticeOnlyShowsForNewerSemanticVersion(t *testing.T) {
	for _, test := range []struct {
		current string
		latest  string
		shown   bool
	}{
		{"v0.3.0", "v0.3.1", true},
		{"v0.3.1", "v0.3.1", false},
		{"v0.4.0", "v0.3.9", false},
		{"dev", "v0.3.1", false},
	} {
		m := NewOnline(&Runtime{Version: test.current})
		m.connected(api.GetMeResponse{Alias: "willow", LatestTUIVersion: test.latest})
		if shown := strings.Contains(m.status, "newer Orifude release"); shown != test.shown {
			t.Errorf("current %q latest %q notice = %v", test.current, test.latest, shown)
		}
	}
}
