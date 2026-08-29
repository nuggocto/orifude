package tui

import (
	"errors"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/colorprofile"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/identity"
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

func TestOnlineWaitKeepsBranchStableWhenNoLetterIsAvailable(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.connection = connectionOnline
	m.screen = ScreenBranch
	m.cursor = 1
	m.device = new(api.DeviceClient)

	next, command := m.activate()
	m = next.(Model)
	if command == nil || m.screen != ScreenBranch || m.pending == nil || m.pending.kind != operationClaim {
		t.Fatalf("wait started with screen=%v pending=%+v command=%v", m.screen, m.pending, command != nil)
	}
	if rendered := m.View().Content; !strings.Contains(rendered, "Waiting by the branch") || strings.Contains(rendered, "post office is listening") {
		t.Fatalf("waiting branch content = %q", rendered)
	}

	notFound := &api.HTTPError{Status: 404, API: api.APIError{Code: api.ErrorCodeNotFound}}
	m, _, _ = m.handleOnlineMessage(claimMsg{id: m.pending.id, err: notFound})
	if m.screen != ScreenBranch || m.pending != nil || m.status != "No letter is waiting right now." {
		t.Fatalf("empty wait ended with screen=%v pending=%+v status=%q", m.screen, m.pending, m.status)
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

func TestAmbiguousReleaseCannotLoseItsRetryIdentifier(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.screen = ScreenFoldPreview
	m.draft.SetValue("preserve me")
	pending := m.beginOperation(operationSend, true)
	pending.clientID = "same-client-id"
	pending.body = m.draft.Value()

	m, _, _ = m.handleOnlineMessage(sendMsg{id: pending.id, err: api.ErrTransport})
	m.back()
	if m.screen != ScreenFoldPreview || m.pending == nil || m.pending.clientID != "same-client-id" || !m.pending.uncertain {
		t.Fatalf("back abandoned ambiguous release: screen=%v pending=%+v", m.screen, m.pending)
	}
	next, command := m.requestQuit()
	m = next.(Model)
	if command != nil || m.pending == nil || !strings.Contains(m.status, "original identifier") {
		t.Fatalf("quit abandoned ambiguous release: pending=%+v status=%q", m.pending, m.status)
	}
	m.device = new(api.DeviceClient)
	if retry := m.startSend(); retry == nil || m.pending == nil || !m.pending.busy || m.pending.clientID != "same-client-id" {
		t.Fatalf("release retry did not reuse original state: pending=%+v", m.pending)
	}
	m, _, _ = m.handleOnlineMessage(sendMsg{id: m.pending.id, response: api.CreateLetterResponse{
		LetterID: "same-client-id", State: api.LetterStateWaiting, FoldSeed: api.FoldSeedForLetterID("same-client-id"), CreatedAt: time.Now(),
	}})
	if m.screen != ScreenDelivery || m.current == nil || m.current.ID != "same-client-id" || m.draft.Value() != "" {
		t.Fatalf("reconciled release = screen %v current %+v draft %q", m.screen, m.current, m.draft.Value())
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

func TestAmbiguousRegistrationOffersRetryAndCanQuit(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.screen = ScreenRevocation
	m.registration = &pendingRegistration{uncertain: true}
	pending := m.beginOperation(operationRegister, true)
	pending.busy = false
	pending.uncertain = true

	rendered := m.View().Content
	if !strings.Contains(rendered, "Retry registration with the same device key") || !strings.Contains(rendered, "enter") {
		t.Fatalf("ambiguous registration does not advertise retry: %q", rendered)
	}
	if strings.Contains(rendered, "b back") {
		t.Fatalf("ambiguous registration advertises blocked back action: %q", rendered)
	}

	_, command := m.Update(textKey("q"))
	if command == nil {
		t.Fatal("ambiguous registration blocked quit")
	}
	if _, ok := command().(tea.QuitMsg); !ok {
		t.Fatal("ambiguous registration q did not emit quit")
	}
	_, command = m.Update(tea.KeyPressMsg(tea.Key{Code: 'c', Mod: tea.ModCtrl}))
	if command == nil {
		t.Fatal("ambiguous registration blocked control-c")
	}
	if _, ok := command().(tea.QuitMsg); !ok {
		t.Fatal("ambiguous registration control-c did not emit quit")
	}
}

func TestConfirmedRegistrationPersistenceFailureCannotBeAbandoned(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.screen = ScreenRevocation
	m.registration = &pendingRegistration{}
	pending := m.beginOperation(operationRegister, true)
	m, _, _ = m.handleOnlineMessage(registerMsg{id: pending.id, confirmed: true, err: errors.New("disk full")})
	if m.registration == nil || !m.registration.confirmed || !m.registration.uncertain {
		t.Fatalf("confirmed registration state = %+v", m.registration)
	}
	m.back()
	if m.screen != ScreenRevocation || m.registration == nil {
		t.Fatal("confirmed registration persistence failure was abandoned")
	}
	next, command := m.activate()
	m = next.(Model)
	if command == nil || m.pending == nil || m.pending.kind != operationRegister || !m.pending.busy {
		t.Fatalf("confirmed registration retry = command %v pending %+v", command != nil, m.pending)
	}
}

func TestBootstrapTemporaryFailuresKeepStoredIdentityRetryable(t *testing.T) {
	for _, err := range []error{
		api.ErrTransport,
		&api.HTTPError{API: api.APIError{Code: api.ErrorCodeServiceUnavailable}},
		&api.HTTPError{API: api.APIError{Code: api.ErrorCodeClockSkew}},
	} {
		m := NewOnline(&Runtime{})
		m.screen = ScreenSplash
		device := new(api.DeviceClient)
		next, _, _ := m.handleOnlineMessage(bootstrapMsg{
			profile: identity.Profile{Alias: "willow", Active: true}, device: device, found: true, err: err,
		})
		if next.screen != ScreenBranch || next.connection != connectionOffline || next.device != device {
			t.Errorf("temporary startup error %v produced screen=%v connection=%v", err, next.screen, next.connection)
		}
	}
}

func TestPendingRegistrationTemporaryStartupRequiresReconciliation(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.screen = ScreenSplash
	device := new(api.DeviceClient)
	next, _, _ := m.handleOnlineMessage(bootstrapMsg{
		profile: identity.Profile{Alias: "willow", Active: false}, device: device, found: true, err: api.ErrTransport,
	})
	if next.screen != ScreenRecovery || next.connection != connectionOffline || next.device != device {
		t.Fatalf("pending startup produced screen=%v connection=%v device=%p", next.screen, next.connection, next.device)
	}
	if rendered := next.View().Content; !strings.Contains(rendered, "Identity creation could not be confirmed") || strings.Contains(rendered, "Welcome to the branch") {
		t.Fatalf("pending startup explanation = %q", rendered)
	}
}

func TestApplySettingsRecomputesASCIIFromPersistedPreference(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.profile = colorprofile.TrueColor
	m.ascii = false
	m.applySettings(identity.Settings{Theme: "auto", ASCIIFallback: true})
	if !m.ascii || m.help.ShortSeparator != " | " {
		t.Fatalf("persisted ASCII setting produced ascii=%t separator=%q", m.ascii, m.help.ShortSeparator)
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

func TestOnlinePreviewUsesDurableLetterSeed(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.screen = ScreenCompose
	m.reducedMotion = true
	m.draft.SetValue("same fold for both readers")
	next, _ := m.activate()
	m = next.(Model)
	if m.draftID == "" || m.current == nil || m.current.ID != m.draftID {
		t.Fatalf("preview identity = draft %q current %+v", m.draftID, m.current)
	}
	if m.current.FoldSeed != uint64(api.FoldSeedForLetterID(m.draftID)) {
		t.Fatalf("preview seed = %d, want durable seed", m.current.FoldSeed)
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

func TestClaimedSenderDoesNotSeeBlockAction(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.current = &Letter{ID: "letter", Role: api.LetterRoleSender, State: api.LetterStateClaimed, SenderAlias: "mori"}
	for _, action := range m.detailActions() {
		if action == detailBlock {
			t.Fatal("claimed sender was offered a block action the server rejects")
		}
	}
}

func TestOnlineExchangeEndClearsBoundReplyDraft(t *testing.T) {
	m := NewOnline(&Runtime{})
	m.screen = ScreenRead
	m.current = &Letter{ID: "letter", Role: api.LetterRoleRecipient}
	m.bindReplyDraft("letter")
	m.replyDraft.SetValue("unfinished")
	pending := m.beginOperation(operationDeleteKeepsake, true)
	next := m.handleDeleteKeepsake(deleteKeepsakeMsg{id: pending.id})
	if next.replyDraft.Value() != "" || next.replyDraftID != "" {
		t.Fatalf("removed exchange retained reply draft %q for %q", next.replyDraft.Value(), next.replyDraftID)
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

func TestConnectionSuccessDoesNotOverwriteUpdateNotice(t *testing.T) {
	for _, reconnect := range []bool{false, true} {
		m := NewOnline(&Runtime{Version: "v0.3.0"})
		m.screen = ScreenRevocation
		m.localIdentity = identity.Profile{Thumbprint: "thumbprint"}
		if reconnect {
			pending := m.beginOperation(operationReconnect, false)
			m, _, _ = m.handleOnlineMessage(reconnectMsg{id: pending.id, me: api.GetMeResponse{Alias: "willow", LatestTUIVersion: "v0.3.1"}})
		} else {
			m.registration = &pendingRegistration{}
			pending := m.beginOperation(operationRegister, true)
			m, _, _ = m.handleOnlineMessage(registerMsg{id: pending.id, confirmed: true, me: api.GetMeResponse{Alias: "willow", LatestTUIVersion: "v0.3.1"}})
		}
		if !strings.Contains(m.status, "newer Orifude release") {
			t.Errorf("reconnect=%t status = %q", reconnect, m.status)
		}
	}
}
