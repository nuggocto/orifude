package tui

import (
	"fmt"
	"io"
	"strings"
	"testing"
	"unicode/utf8"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/colorprofile"
)

func TestPrintableNavigationKeysRemainTextWhileComposerIsFocused(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenCompose
	m.mode = ModeText
	m.draft.Focus()
	m = updateModel(t, m, textKey("q"))
	m = updateModel(t, m, textKey("b"))
	if got := m.draft.Value(); got != "qb" {
		t.Fatalf("draft = %q, want qb", got)
	}
	if m.screen != ScreenCompose {
		t.Fatalf("q left composer for screen %v", m.screen)
	}

	m = updateModel(t, m, specialKey(tea.KeyEscape))
	if m.mode != ModeNavigation {
		t.Fatalf("escape left mode %v, want navigation", m.mode)
	}
}

func TestNeovimNavigationSelectsBranchActions(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenBranch
	m.identity.Alias = "sora"
	m = updateModel(t, m, textKey("j"))
	if m.cursor != 1 {
		t.Fatalf("j cursor = %d, want 1", m.cursor)
	}
	m = updateModel(t, m, textKey("k"))
	if m.cursor != 0 {
		t.Fatalf("k cursor = %d, want 0", m.cursor)
	}
	m = updateModel(t, m, textKey("G"))
	if m.cursor != 3 {
		t.Fatalf("G cursor = %d, want 3", m.cursor)
	}
	m = updateModel(t, m, textKey("g"))
	m = updateModel(t, m, textKey("g"))
	if m.cursor != 0 {
		t.Fatalf("g g cursor = %d, want 0", m.cursor)
	}
}

func TestSettingsApplyDisplayAndAccessibilityPreferences(t *testing.T) {
	t.Parallel()

	m := New()
	next, _ := m.finishForm(formSettings, formData{theme: "auto", reduced: true, ascii: true, accessible: true})
	m = next.(Model)
	if !m.reducedMotion || !m.ascii {
		t.Fatalf("settings applied reduced=%v ascii=%v", m.reducedMotion, m.ascii)
	}
	if !m.asciiFallback {
		t.Fatal("printable fallback preference was not retained")
	}
	if !m.accessible {
		t.Fatal("accessible form preference was not applied")
	}
	if m.screen != ScreenBranch {
		t.Fatalf("settings returned to screen %v, want branch", m.screen)
	}
}

func TestComposeReleaseJourneyPreservesBoundsAndProducesReceipt(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenBranch
	m.identity.Alias = "sora"
	m = updateModel(t, m, specialKey(tea.KeyEnter))
	if m.screen != ScreenCompose || m.mode != ModeText {
		t.Fatalf("enter opened screen=%v mode=%v", m.screen, m.mode)
	}
	m = updateModel(t, m, tea.PasteMsg{Content: "A short letter."})
	m = updateModel(t, m, specialKey(tea.KeyEscape))
	m = updateModel(t, m, specialKey(tea.KeyEnter))
	if m.screen != ScreenFoldPreview || !m.animating {
		t.Fatalf("preview screen=%v animating=%v", m.screen, m.animating)
	}

	for m.animating {
		m = updateModel(t, m, foldTickMsg{id: m.animationID})
	}
	if m.formKind != formRelease {
		t.Fatalf("fold completed with form %v, want release", m.formKind)
	}
	next, _ := m.finishForm(formRelease, formData{confirmed: true})
	m = next.(Model)
	if m.screen != ScreenDelivery {
		t.Fatalf("release screen = %v, want delivery", m.screen)
	}
	if m.draft.Value() != "" {
		t.Fatalf("released draft was retained: %q", m.draft.Value())
	}
	if len(m.keepsakes) != 2 {
		t.Fatalf("keepsake count = %d, want 2", len(m.keepsakes))
	}
}

func TestResizeChoosesEverySupportedLayoutWithoutLosingDraft(t *testing.T) {
	t.Parallel()

	tests := []struct {
		width  int
		height int
		want   layoutMode
	}{
		{width: 100, height: 30, want: layoutWide},
		{width: 72, height: 24, want: layoutCompact},
		{width: 56, height: 18, want: layoutText},
		{width: 55, height: 17, want: layoutTooSmall},
	}

	for _, test := range tests {
		t.Run(test.want.String(), func(t *testing.T) {
			m := New()
			m.screen = ScreenCompose
			m.draft.SetValue("keep me")
			m = updateModel(t, m, tea.WindowSizeMsg{Width: test.width, Height: test.height})
			if got := m.layout(); got != test.want {
				t.Fatalf("layout = %v, want %v", got, test.want)
			}
			if got := m.draft.Value(); got != "keep me" {
				t.Fatalf("resize changed draft to %q", got)
			}
			rendered := m.View().Content
			if width := lipgloss.Width(rendered); width > test.width {
				t.Fatalf("render width = %d, terminal width = %d", width, test.width)
			}
			if height := lipgloss.Height(rendered); height > test.height {
				t.Fatalf("render height = %d, terminal height = %d", height, test.height)
			}
		})
	}
}

func TestEveryScreenFitsMinimumSupportedTerminal(t *testing.T) {
	t.Parallel()

	screens := []Screen{
		ScreenSplash, ScreenBranch, ScreenCompose, ScreenFoldPreview, ScreenDelivery,
		ScreenSearching, ScreenFoldedDelivery, ScreenUnfold, ScreenRead, ScreenReply,
		ScreenReplyPreview, ScreenKeepsakes, ScreenKeepsakeDetail, ScreenReport, ScreenSettings,
	}
	for _, screen := range screens {
		t.Run(fmt.Sprintf("screen-%d", screen), func(t *testing.T) {
			m := New()
			m.identity.Alias = "sora"
			m.resize(56, 18)
			m.screen = screen
			m.status = "Ready."
			m.setCurrent(incomingFixture())
			if screen == ScreenKeepsakeDetail {
				m.setKeepsake(0)
			}
			rendered := m.View().Content
			if lipgloss.Width(rendered) > m.width || lipgloss.Height(rendered) > m.height {
				t.Fatalf("screen %v renders %dx%d in %dx%d", screen, lipgloss.Width(rendered), lipgloss.Height(rendered), m.width, m.height)
			}
		})
	}
}

func TestEmbeddedFormsFitMinimumSupportedTerminal(t *testing.T) {
	t.Parallel()

	forms := []struct {
		kind   formKind
		screen Screen
	}{
		{kind: formOnboarding, screen: ScreenOnboarding},
		{kind: formRelease, screen: ScreenFoldPreview},
		{kind: formReplyRelease, screen: ScreenReplyPreview},
		{kind: formReport, screen: ScreenReport},
		{kind: formSettings, screen: ScreenSettings},
		{kind: formQuit, screen: ScreenCompose},
	}
	for _, test := range forms {
		t.Run(fmt.Sprintf("form-%d", test.kind), func(t *testing.T) {
			m := New()
			m.resize(56, 18)
			m.screen = test.screen
			m.setCurrent(incomingFixture())
			m.beginForm(test.kind)
			rendered := m.View().Content
			if lipgloss.Width(rendered) > m.width || lipgloss.Height(rendered) > m.height {
				t.Fatalf("form %v renders %dx%d in %dx%d", test.kind, lipgloss.Width(rendered), lipgloss.Height(rendered), m.width, m.height)
			}
		})
	}
}

func TestTypedInputStopsAtCodePointLimit(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenCompose
	m.mode = ModeText
	m.draft.SetValue(strings.Repeat("界", maxBodyCodePoints))
	m = updateModel(t, m, textKey("界"))
	if got := utf8.RuneCountInString(m.draft.Value()); got != maxBodyCodePoints {
		t.Fatalf("draft has %d code points, want %d", got, maxBodyCodePoints)
	}
	if m.status != errBodyCodePoints.Error() {
		t.Fatalf("status = %q, want %q", m.status, errBodyCodePoints)
	}
}

func TestStaleSearchCannotReplaceLaterNavigation(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenBranch
	m.cursor = 1
	next, _ := m.activate()
	m = next.(Model)
	searchID := m.searchID
	m = updateModel(t, m, textKey("b"))
	m = updateModel(t, m, specialKey(tea.KeyEnter))
	m = updateModel(t, m, searchDoneMsg{id: searchID})
	if m.screen != ScreenCompose {
		t.Fatalf("stale search changed screen to %v, want compose", m.screen)
	}
}

func TestWaitResumesFoldedAndOpenedFixture(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenSearching
	m.searchID = 1
	m = updateModel(t, m, searchDoneMsg{id: 1})
	if m.screen != ScreenFoldedDelivery || m.incomingState != fixtureFolded {
		t.Fatalf("first claim screen=%v state=%v", m.screen, m.incomingState)
	}
	m = updateModel(t, m, textKey("b"))
	m.setCurrent(Letter{SenderAlias: "sora", Body: "other", FoldSeed: 99})
	m.screen = ScreenSearching
	m.searchID++
	m = updateModel(t, m, searchDoneMsg{id: m.searchID})
	if m.screen != ScreenFoldedDelivery || m.current.SenderAlias != "aoi" {
		t.Fatalf("folded claim resumed screen=%v sender=%q", m.screen, m.current.SenderAlias)
	}
	m.reducedMotion = true
	m.startAnimation(ScreenUnfold, true)
	m = updateModel(t, m, textKey("b"))
	m.setCurrent(Letter{SenderAlias: "sora", Body: "other", FoldSeed: 99})
	m.screen = ScreenSearching
	m.searchID++
	m = updateModel(t, m, searchDoneMsg{id: m.searchID})
	if m.screen != ScreenRead || m.current.SenderAlias != "aoi" {
		t.Fatalf("opened claim resumed screen=%v sender=%q", m.screen, m.current.SenderAlias)
	}
}

func TestEnterCannotRaceFoldAnimation(t *testing.T) {
	t.Parallel()

	m := New()
	m.setCurrent(Letter{Body: "letter", FoldSeed: 1})
	m.startAnimation(ScreenFoldPreview, false)
	next, _ := m.activate()
	m = next.(Model)
	if m.form != nil || !m.animating {
		t.Fatalf("early enter form=%v animating=%v", m.form != nil, m.animating)
	}
}

func TestReducedMotionUsesFinalFoldAndFirstReadAction(t *testing.T) {
	t.Parallel()

	m := New()
	m.reducedMotion = true
	m.setCurrent(incomingFixture())
	m.startAnimation(ScreenFoldPreview, false)
	wantLast := len(foldFrames(m.currentSeed(), m.width, m.ascii)) - 1
	if m.foldFrame != wantLast || m.screen != ScreenFoldPreview || m.form != nil {
		t.Fatalf("reduced fold frame=%d screen=%v form=%v", m.foldFrame, m.screen, m.form != nil)
	}
	if !strings.Contains(m.View().Content, "[enter] continue") {
		t.Fatal("reduced-motion final fold is not shown before confirmation")
	}
	next, _ := m.activate()
	m = next.(Model)
	if m.formKind != formRelease {
		t.Fatalf("reduced fold opened form %v, want release", m.formKind)
	}
	m.form = nil
	m.formKind = formNone
	m.cursor = 3
	m.startAnimation(ScreenUnfold, true)
	if m.screen != ScreenRead || m.cursor != 0 {
		t.Fatalf("reduced unfold screen=%v cursor=%d, want read cursor 0", m.screen, m.cursor)
	}
}

func TestReportBurnsExchangeAndConsumesFixture(t *testing.T) {
	t.Parallel()

	m := New()
	m.incomingState = fixtureConsumed
	m.setCurrent(incomingFixture())
	m.replyDraft.SetValue("unfinished")
	m.reportReturn = ScreenRead
	m.reportIndex = -1
	m.reportTarget = "original letter"
	next, _ := m.finishForm(formReport, formData{confirmed: true, reason: "spam"})
	m = next.(Model)
	if m.current != nil || m.replyDraft.Value() != "" {
		t.Fatalf("report retained current=%v reply=%q", m.current != nil, m.replyDraft.Value())
	}
	m.screen = ScreenSearching
	m.searchID++
	m = updateModel(t, m, searchDoneMsg{id: m.searchID})
	if m.screen != ScreenBranch || m.current != nil {
		t.Fatalf("blocked fixture returned on screen=%v current=%v", m.screen, m.current != nil)
	}
}

func TestReceivedKeepsakeCanBeReportedAndRemoved(t *testing.T) {
	t.Parallel()

	m := New()
	m.setKeepsake(0)
	m.screen = ScreenKeepsakeDetail
	next, _ := m.activate()
	m = next.(Model)
	if m.formKind != formReport || m.reportIndex != 0 || m.reportTarget != "original letter" {
		t.Fatalf("keepsake report form=%v index=%d target=%q", m.formKind, m.reportIndex, m.reportTarget)
	}
	next, _ = m.finishForm(formReport, formData{confirmed: true, reason: "harassment"})
	m = next.(Model)
	if len(m.keepsakes) != 0 || m.screen != ScreenBranch {
		t.Fatalf("reported keepsake count=%d screen=%v", len(m.keepsakes), m.screen)
	}
}

func TestReportingKeepsakePreservesUnrelatedReplyDraft(t *testing.T) {
	t.Parallel()

	m := New()
	m.incomingState = fixtureOpened
	m.replyDraft.SetValue("unfinished reply")
	m.setKeepsake(0)
	m.reportIndex = 0
	m.reportTarget = "original letter"
	next, _ := m.finishForm(formReport, formData{confirmed: true, reason: "harassment"})
	m = next.(Model)
	if got := m.replyDraft.Value(); got != "unfinished reply" {
		t.Fatalf("unrelated report changed reply draft to %q", got)
	}
	if m.incomingState != fixtureOpened {
		t.Fatalf("unrelated report changed incoming state to %v", m.incomingState)
	}

	m.screen = ScreenSearching
	m.searchID++
	m = updateModel(t, m, searchDoneMsg{id: m.searchID})
	if m.screen != ScreenRead || m.replyDraft.Value() != "unfinished reply" {
		t.Fatalf("resumed exchange screen=%v reply=%q", m.screen, m.replyDraft.Value())
	}
}

func TestDiscardRemovesCurrentExchange(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenRead
	m.cursor = 3
	m.setCurrent(incomingFixture())
	m.replyDraft.SetValue("unfinished")
	next, _ := m.activate()
	m = next.(Model)
	if m.screen != ScreenBranch || m.current != nil || m.replyDraft.Value() != "" {
		t.Fatalf("discard left screen=%v current=%v reply=%q", m.screen, m.current != nil, m.replyDraft.Value())
	}
}

func TestKeepClearsAbandonedReplyWithoutDuplicatingKeepsake(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenRead
	m.cursor = 1
	m.setCurrent(incomingFixture())
	m.replyDraft.SetValue("unfinished")
	next, _ := m.activate()
	m = next.(Model)
	if m.replyDraft.Value() != "" || len(m.keepsakes) != 2 {
		t.Fatalf("first keep reply=%q keepsakes=%d", m.replyDraft.Value(), len(m.keepsakes))
	}
	if m.incomingState != fixtureConsumed {
		t.Fatalf("kept fixture state = %v, want consumed", m.incomingState)
	}
	m.screen = ScreenRead
	m.cursor = 1
	next, _ = m.activate()
	m = next.(Model)
	if len(m.keepsakes) != 2 {
		t.Fatalf("duplicate keep produced %d keepsakes", len(m.keepsakes))
	}
}

func TestKeepsakeDetailKeepsLongReplyInsideViewport(t *testing.T) {
	t.Parallel()

	m := New()
	m.keepsakes = []LetterSummary{{
		Direction: "sent",
		Alias:     "aoi",
		Letter: Letter{
			SenderAlias: "sora",
			Body:        "original",
			Reply:       strings.Repeat("reply\n", maxBodyCodePoints/6) + "last reply line",
		},
	}}
	m.setKeepsake(0)
	m.screen = ScreenKeepsakeDetail
	m.resize(56, 18)
	m.viewport.GotoBottom()
	rendered := m.View().Content
	if !strings.Contains(rendered, "last reply line") {
		t.Fatal("bottom of reply is not reachable in keepsake viewport")
	}
	if lipgloss.Width(rendered) > m.width || lipgloss.Height(rendered) > m.height {
		t.Fatalf("keepsake render exceeds %dx%d", m.width, m.height)
	}
}

func TestLetterViewWrapsProseWithoutSplittingWords(t *testing.T) {
	t.Parallel()

	m := New()
	m.resize(120, 36)
	letter := incomingFixture()
	m.setCurrent(letter)

	wrapped := m.viewport.GetContent()
	if got := strings.ReplaceAll(wrapped, "\n", " "); got != letter.Body {
		t.Fatalf("wrapped letter changed word boundaries:\n%s", wrapped)
	}
}

func TestJKScrollKeepsakeWithoutReportAction(t *testing.T) {
	t.Parallel()

	m := New()
	m.keepsakes = []LetterSummary{{
		Direction: "sent",
		Alias:     "waiting",
		Letter:    Letter{SenderAlias: "sora", Body: strings.Repeat("line\n", 30)},
	}}
	m.setKeepsake(0)
	m.screen = ScreenKeepsakeDetail
	m.resize(56, 18)
	m = updateModel(t, m, textKey("j"))
	if m.viewport.YOffset() == 0 {
		t.Fatal("j did not scroll a keepsake without actions")
	}
	m = updateModel(t, m, textKey("k"))
	if m.viewport.YOffset() != 0 {
		t.Fatalf("k left viewport offset %d, want 0", m.viewport.YOffset())
	}
}

func TestClipboardShortcutCannotBypassValidatedPaste(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenCompose
	m.mode = ModeText
	m.draft.Focus()
	m = updateModel(t, m, tea.KeyPressMsg(tea.Key{Code: 'v', Mod: tea.ModCtrl}))
	if m.draft.Value() != "" {
		t.Fatalf("clipboard shortcut changed draft to %q", m.draft.Value())
	}
	if !strings.Contains(m.status, "terminal's paste") {
		t.Fatalf("clipboard shortcut status = %q", m.status)
	}
}

func TestEnterCannotExceedCodePointLimit(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenCompose
	m.mode = ModeText
	m.draft.SetValue(strings.Repeat("a", maxBodyCodePoints))
	m.draft.Focus()
	m = updateModel(t, m, specialKey(tea.KeyEnter))
	if got := utf8.RuneCountInString(m.draft.Value()); got != maxBodyCodePoints {
		t.Fatalf("enter produced %d code points, want %d", got, maxBodyCodePoints)
	}
	if m.status != errBodyCodePoints.Error() {
		t.Fatalf("enter status = %q, want %q", m.status, errBodyCodePoints)
	}
}

func TestPasteCanReplaceSelectionAtLimit(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenCompose
	m.mode = ModeText
	m.draft.SetValue(strings.Repeat("a", maxBodyCodePoints))
	m.draft.Focus()
	m.draft.SelectAll()
	if err := m.validatePaste("x"); err != nil {
		t.Fatalf("replacement paste rejected: %v", err)
	}
	m = updateModel(t, m, tea.PasteMsg{Content: "x"})
	if m.draft.Value() != "x" {
		t.Fatalf("replacement paste produced %q, want x", m.draft.Value())
	}
}

func TestTooSmallModeKeepsTextFocusedAndQuitConfirmationVisible(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenCompose
	m.mode = ModeText
	m.draft.SetValue("keep me")
	m.draft.Focus()
	m.resize(20, 10)
	m = updateModel(t, m, textKey("q"))
	if m.formKind != formNone || m.draft.Value() != "keep meq" {
		t.Fatalf("too-small q form=%v draft=%q", m.formKind, m.draft.Value())
	}
	m = updateModel(t, m, tea.KeyPressMsg(tea.Key{Code: 'c', Mod: tea.ModCtrl}))
	rendered := m.View().Content
	if !strings.Contains(rendered, "Draft exists") {
		t.Fatalf("quit confirmation is hidden: %q", rendered)
	}
	if lipgloss.Width(rendered) > m.width || lipgloss.Height(rendered) > m.height {
		t.Fatalf("too-small render exceeds %dx%d", m.width, m.height)
	}
}

func TestRepeatedControlCDoesNotBypassDraftConfirmation(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenCompose
	m.draft.SetValue("keep me")
	controlC := tea.KeyPressMsg(tea.Key{Code: 'c', Mod: tea.ModCtrl})
	m = updateModel(t, m, controlC)
	if m.formKind != formQuit {
		t.Fatalf("first ctrl+c opened form %v, want quit", m.formKind)
	}
	next, command := m.Update(controlC)
	m = next.(Model)
	if command != nil || m.formKind != formQuit {
		t.Fatalf("second ctrl+c command=%v form=%v, want confirmation retained", command != nil, m.formKind)
	}
}

func TestQuitDuringFoldCannotBeReplacedByLateTick(t *testing.T) {
	t.Parallel()

	m := New()
	m.draft.SetValue("keep me")
	m.setCurrent(Letter{Body: "keep me", FoldSeed: 1})
	m.startAnimation(ScreenFoldPreview, false)
	animationID := m.animationID
	next, _ := m.requestQuit()
	m = next.(Model)
	m = updateModel(t, m, foldTickMsg{id: animationID})
	if m.formKind != formQuit {
		t.Fatalf("late fold tick replaced form with %v", m.formKind)
	}
	next, _ = m.finishForm(formQuit, formData{confirmed: false})
	m = next.(Model)
	if m.screen != ScreenCompose {
		t.Fatalf("declining quit returned to %v, want compose", m.screen)
	}
}

func TestDecliningQuitFromReportReturnsToLetter(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenReport
	m.reportReturn = ScreenRead
	m.replyDraft.SetValue("keep me")
	m.beginForm(formReport)
	next, _ := m.requestQuit()
	m = next.(Model)
	if m.quitReturn != ScreenRead {
		t.Fatalf("report quit return = %v, want read", m.quitReturn)
	}
	next, _ = m.finishForm(formQuit, formData{confirmed: false})
	m = next.(Model)
	if m.screen != ScreenRead {
		t.Fatalf("declining report quit returned to %v, want read", m.screen)
	}
}

func TestDecliningQuitCancelsSearch(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenSearching
	m.draft.SetValue("keep me")
	m.searchID = 7
	next, _ := m.requestQuit()
	m = next.(Model)
	if m.quitReturn != ScreenBranch {
		t.Fatalf("search quit return = %v, want branch", m.quitReturn)
	}
	next, _ = m.finishForm(formQuit, formData{confirmed: false})
	m = next.(Model)
	m = updateModel(t, m, searchDoneMsg{id: 7})
	if m.screen != ScreenBranch {
		t.Fatalf("declining search quit left screen %v, want branch", m.screen)
	}
}

func TestAccessibleFormsUseExternalCommand(t *testing.T) {
	t.Parallel()

	m := New()
	m.accessible = true
	command := m.beginForm(formSettings)
	if command == nil || m.form != nil || m.formKind != formSettings {
		t.Fatalf("accessible begin command=%v form=%v kind=%v", command != nil, m.form != nil, m.formKind)
	}
}

func TestAccessibleConfirmationCompletesWithoutTerminalUI(t *testing.T) {
	t.Parallel()

	m := New()
	data := formData{}
	command := accessibleFormCommand{form: m.buildForm(formRelease, &data)}
	command.SetStdin(strings.NewReader("y\n"))
	command.SetStdout(io.Discard)
	if err := command.Run(); err != nil {
		t.Fatalf("accessible confirmation: %v", err)
	}
	if !data.confirmed {
		t.Fatal("accessible confirmation did not update bound form data")
	}
}

func TestStaleAccessibleFormCompletionIsIgnored(t *testing.T) {
	t.Parallel()

	m := New()
	m.accessible = true
	m.screen = ScreenSettings
	m.beginForm(formSettings)
	staleID := m.formID
	m.cancelForm()
	m.beginForm(formSettings)
	m = updateModel(t, m, accessibleFormDoneMsg{id: staleID, kind: formSettings, data: formData{theme: "dark"}})
	if m.theme != "auto" || m.formKind != formSettings {
		t.Fatalf("stale completion applied theme=%q form=%v", m.theme, m.formKind)
	}
}

func TestBBacksOutOfNonTextForms(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenFoldPreview
	m.beginForm(formRelease)
	m = updateModel(t, m, textKey("b"))
	if m.form != nil || m.screen != ScreenCompose {
		t.Fatalf("b left form=%v screen=%v, want compose without form", m.form != nil, m.screen)
	}
}

func TestNonTextFormGlobalKeysAndHelpRemainActive(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenSettings
	m.draft.SetValue("keep me")
	m.beginForm(formSettings)
	m = updateModel(t, m, textKey("?"))
	if !m.showHelp || m.formKind != formSettings {
		t.Fatalf("form help visible=%v form=%v", m.showHelp, m.formKind)
	}
	if help := m.View().Content; !strings.Contains(help, "q/ctrl+c") {
		t.Fatalf("help omits active quit binding: %q", help)
	}
	m = updateModel(t, m, textKey("q"))
	if m.showHelp || m.formKind != formQuit {
		t.Fatalf("help q visible=%v form=%v, want quit confirmation", m.showHelp, m.formKind)
	}
}

func TestFullHelpOnlyShowsCurrentScreenActions(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenBranch
	m.showHelp = true
	branchHelp := m.View().Content
	if strings.Contains(branchHelp, "half page") {
		t.Fatalf("branch help advertises viewport action: %q", branchHelp)
	}
	for _, action := range []string{"j/down", "k/up", "g g/home", "G/end"} {
		if !strings.Contains(branchHelp, action) {
			t.Fatalf("branch help omits %q: %q", action, branchHelp)
		}
	}

	m.setKeepsake(0)
	m.screen = ScreenKeepsakeDetail
	keepsakeHelp := m.View().Content
	for _, action := range []string{"half page", "full page", "scroll down", "report"} {
		if !strings.Contains(keepsakeHelp, action) {
			t.Fatalf("keepsake help omits %q: %q", action, keepsakeHelp)
		}
	}
}

func TestEscapeDoesNotBackOutOfForms(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenFoldPreview
	m.beginForm(formRelease)
	m = updateModel(t, m, specialKey(tea.KeyEscape))
	if m.form == nil || m.formKind != formRelease || m.screen != ScreenFoldPreview {
		t.Fatalf("escape left form=%v kind=%v screen=%v, want release form on fold preview", m.form != nil, m.formKind, m.screen)
	}
}

func TestEscapeLeavesEmbeddedTextInputWithoutLeavingForm(t *testing.T) {
	t.Parallel()

	m := New()
	m.screen = ScreenOnboarding
	m.beginForm(formOnboarding)
	m.form.NextGroup()
	if !m.formAcceptsText() {
		t.Fatal("onboarding alias input is not focused")
	}
	m = updateModel(t, m, specialKey(tea.KeyEscape))
	if m.form == nil || m.screen != ScreenOnboarding {
		t.Fatalf("escape left form=%v screen=%v, want onboarding form", m.form != nil, m.screen)
	}
	if m.formAcceptsText() {
		t.Fatal("escape left the alias input focused")
	}
}

func TestActiveFormRefreshesForASCIITerminal(t *testing.T) {
	t.Parallel()

	m := New()
	m.profile = colorprofile.TrueColor
	m.refreshPresentation()
	m.screen = ScreenSettings
	m.beginForm(formSettings)
	m = updateModel(t, m, tea.ColorProfileMsg{Profile: colorprofile.Ascii})
	for _, r := range m.View().Content {
		if r > 0x7f {
			t.Fatalf("refreshed form rendered structural rune %U", r)
		}
	}
}

func TestUserASCIIFallbackUsesOnlyASCIIStructure(t *testing.T) {
	t.Parallel()

	for _, screen := range []Screen{ScreenSplash, ScreenFoldPreview, ScreenSettings} {
		t.Run(fmt.Sprintf("screen-%d", screen), func(t *testing.T) {
			m := New()
			m.profile = colorprofile.TrueColor
			m.ascii = true
			m.refreshPresentation()
			m.screen = screen
			if screen == ScreenFoldPreview {
				m.animating = true
			}
			if screen == ScreenSettings {
				m.beginForm(formSettings)
			}
			for _, r := range m.View().Content {
				if r > 0x7f {
					t.Fatalf("ASCII fallback rendered structural rune %U", r)
				}
			}
		})
	}
}

func updateModel(t *testing.T, model Model, message tea.Msg) Model {
	t.Helper()
	next, _ := model.Update(message)
	updated, ok := next.(Model)
	if !ok {
		t.Fatalf("Update returned %T, want tui.Model", next)
	}
	return updated
}

func textKey(text string) tea.KeyPressMsg {
	r, _ := utf8.DecodeRuneInString(text)
	return tea.KeyPressMsg(tea.Key{Code: r, Text: text})
}

func specialKey(code rune) tea.KeyPressMsg {
	return tea.KeyPressMsg(tea.Key{Code: code})
}

func (mode layoutMode) String() string {
	switch mode {
	case layoutWide:
		return "wide"
	case layoutCompact:
		return "compact"
	case layoutText:
		return "text"
	case layoutTooSmall:
		return "too-small"
	default:
		return "unknown"
	}
}
