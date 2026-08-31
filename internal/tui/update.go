package tui

import (
	"errors"
	"fmt"
	"io"
	"time"
	"unicode/utf8"

	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/colorprofile"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/textpolicy"
)

type searchDoneMsg struct{ id uint64 }

type embeddedFormMsg struct {
	id      uint64
	message tea.Msg
}

type accessibleFormDoneMsg struct {
	id   uint64
	kind formKind
	data formData
	err  error
}

type accessibleFormCommand struct {
	form *huh.Form
}

func (command *accessibleFormCommand) Run() error {
	return command.form.WithAccessible(true).Run()
}

func (command *accessibleFormCommand) SetStdin(reader io.Reader) {
	command.form.WithInput(reader)
}

func (command *accessibleFormCommand) SetStdout(writer io.Writer) {
	command.form.WithOutput(writer)
}

func (command *accessibleFormCommand) SetStderr(io.Writer) {}

// Update applies terminal messages to the root model.
func (m Model) Update(message tea.Msg) (tea.Model, tea.Cmd) {
	if m.runtime != nil {
		if next, command, handled := m.handleOnlineMessage(message); handled {
			return next, command
		}
	}
	switch message := message.(type) {
	case tea.WindowSizeMsg:
		m.resize(message.Width, message.Height)
		return m, nil
	case tea.BackgroundColorMsg:
		m.dark = message.IsDark()
		m.refreshPresentation()
		if m.form != nil {
			form, command := m.form.Update(message)
			m.form = form.(*huh.Form)
			return m, scopeFormCommand(m.formID, command)
		}
		return m, nil
	case tea.ColorProfileMsg:
		m.profile = message.Profile
		m.ascii = m.asciiFallback || m.theme == "mono" || message.Profile == colorprofile.Ascii || message.Profile == colorprofile.NoTTY
		m.refreshPresentation()
		if m.form != nil {
			form, command := m.form.Update(message)
			m.form = form.(*huh.Form)
			return m, scopeFormCommand(m.formID, command)
		}
		return m, nil
	case foldTickMsg:
		return m.updateFold(message)
	case searchDoneMsg:
		if message.id != m.searchID || m.screen != ScreenSearching {
			return m, nil
		}
		if m.incomingState == fixtureConsumed {
			m.screen = ScreenBranch
			m.setStatus(statusInfo, noLetterStatus)
			return m, nil
		}
		letter := incomingFixture()
		m.setCurrent(letter)
		if m.incomingState == fixtureOpened {
			m.screen = ScreenRead
			m.cursor = 0
			m.setStatus(statusInfo, "The opened letter is waiting where you left it.")
			return m, nil
		}
		m.incomingState = fixtureFolded
		m.screen = ScreenFoldedDelivery
		m.setStatus(statusInfo, "A folded letter is waiting.")
		return m, nil
	case accessibleFormDoneMsg:
		if message.id != m.formID || message.kind != m.formKind {
			return m, nil
		}
		m.formKind = formNone
		m.formTheme = nil
		m.mode = ModeNavigation
		if message.err != nil {
			if !errors.Is(message.err, huh.ErrUserAborted) {
				m.setStatus(statusError, "The form could not be completed.")
			}
			return m, m.back()
		}
		return m.finishForm(message.kind, message.data)
	case embeddedFormMsg:
		if message.id != m.formID || m.form == nil {
			return m, nil
		}
		return m.forwardInput(message.message)
	case tea.PasteMsg:
		if err := m.validatePaste(message.Content); err != nil {
			m.setStatus(statusError, err.Error())
			return m, nil
		}
		return m.forwardInput(message)
	case tea.KeyPressMsg:
		return m.updateKey(message)
	}

	if m.form != nil || m.mode == ModeText {
		return m.forwardInput(message)
	}
	return m, nil
}

func (m Model) updateKey(message tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	key := message.String()
	if key != "g" {
		m.pendingG = false
	}
	if m.runtime != nil && m.pending != nil && m.pending.busy && m.pending.mutation && (key == "q" || key == "ctrl+c") {
		m.setStatus(statusInfo, "Wait for the current operation to finish.")
		return m, nil
	}
	if key == "ctrl+c" {
		if m.formKind == formQuit {
			return m, nil
		}
		return m.requestQuit()
	}
	if key == "ctrl+v" {
		m.setStatus(statusInfo, "Use the terminal's paste action so input can be checked.")
		return m, nil
	}

	if m.form != nil && m.formAcceptsText() {
		if text := message.Key().Text; text != "" {
			if err := m.validatePaste(text); err != nil {
				m.setStatus(statusError, err.Error())
				return m, nil
			}
		}
		if key == "esc" {
			return m, scopeFormCommand(m.formID, m.form.NextField())
		}
		return m.forwardInput(message)
	}

	if m.form == nil && m.mode == ModeText {
		insertion := message.Key().Text
		if key == "enter" || key == "ctrl+m" {
			insertion = "\n"
		}
		if insertion != "" {
			if err := m.validatePaste(insertion); err != nil {
				m.setStatus(statusError, err.Error())
				return m, nil
			}
		}
		if key == "esc" {
			m.blurEditor()
			m.mode = ModeNavigation
			m.setStatus(statusInfo, "")
			return m, nil
		}
		return m.forwardInput(message)
	}

	if m.layout() == layoutTooSmall {
		if m.formKind == formQuit {
			return m.forwardInput(message)
		}
		if key == "q" {
			return m.requestQuit()
		}
		return m, nil
	}

	if m.showHelp {
		if key == "?" || key == "b" {
			m.showHelp = false
			return m, nil
		}
		if key == "q" {
			return m.requestQuit()
		}
		return m, nil
	}

	if m.form != nil {
		switch key {
		case "?":
			m.showHelp = true
			return m, nil
		case "q":
			return m.requestQuit()
		case "esc":
			return m, nil
		case "b":
			m.cancelForm()
			return m, nil
		}
		return m.forwardInput(message)
	}

	if key == "g" {
		if m.pendingG {
			m.pendingG = false
			m.goTop()
		} else {
			m.pendingG = true
		}
		return m, nil
	}
	switch key {
	case "?":
		m.showHelp = true
	case "q":
		return m.requestQuit()
	case "a":
		if m.screen == ScreenSplash {
			m.accessible = !m.accessible
			m.setStatus(statusInfo, fmt.Sprintf("Screen-reader forms: %s", onOff(m.accessible)))
		}
	case "i":
		if m.screen == ScreenCompose {
			m.mode = ModeText
			return m, m.draft.Focus()
		}
		if m.screen == ScreenReply {
			m.mode = ModeText
			return m, m.replyDraft.Focus()
		}
	case "b":
		return m, m.back()
	case "j", "down", "tab":
		m.move(1)
	case "k", "up", "shift+tab":
		m.move(-1)
	case "enter":
		return m.activate()
	case "G", "end":
		m.goBottom()
	case "home":
		m.goTop()
	case "ctrl+u":
		m.viewport.HalfPageUp()
	case "ctrl+d":
		m.viewport.HalfPageDown()
	case "ctrl+b":
		m.viewport.PageUp()
	case "ctrl+f":
		m.viewport.PageDown()
	}
	return m, nil
}

func (m Model) forwardInput(message tea.Msg) (tea.Model, tea.Cmd) {
	if m.form != nil {
		form, command := m.form.Update(message)
		m.form = form.(*huh.Form)
		command = scopeFormCommand(m.formID, command)
		if m.form.State == huh.StateCompleted {
			kind := m.formKind
			data := *m.formData
			m.form = nil
			m.formData = nil
			m.formTheme = nil
			m.formKind = formNone
			m.mode = ModeNavigation
			next, nextCommand := m.finishForm(kind, data)
			return next, tea.Batch(command, nextCommand)
		}
		if m.form.State == huh.StateAborted {
			m.cancelForm()
		}
		return m, command
	}

	switch m.screen {
	case ScreenCompose:
		var command tea.Cmd
		m.draft, command = m.draft.Update(message)
		m.setStatus(statusInfo, "")
		return m, command
	case ScreenReply:
		var command tea.Cmd
		m.replyDraft, command = m.replyDraft.Update(message)
		m.setStatus(statusInfo, "")
		return m, command
	}
	return m, nil
}

func scopeFormCommand(id uint64, command tea.Cmd) tea.Cmd {
	if command == nil {
		return nil
	}
	return func() tea.Msg {
		message := command()
		if message == nil {
			return nil
		}
		if batch, ok := message.(tea.BatchMsg); ok {
			scoped := make(tea.BatchMsg, 0, len(batch))
			for _, command := range batch {
				if command := scopeFormCommand(id, command); command != nil {
					scoped = append(scoped, command)
				}
			}
			return scoped
		}
		return embeddedFormMsg{id: id, message: message}
	}
}

func (m Model) activate() (tea.Model, tea.Cmd) {
	if m.runtime != nil && m.pending != nil && m.pending.busy && m.pending.mutation {
		if m.pending.kind != operationClaim || m.screen != ScreenBranch || m.cursor != 1 {
			m.setStatus(statusInfo, "That operation is already in progress.")
		}
		return m, nil
	}
	switch m.screen {
	case ScreenSplash:
		if m.runtime != nil && m.connection == connectionConnecting {
			m.setStatus(statusInfo, "Checking the local identity...")
			return m, nil
		}
		if m.runtime != nil && m.cursor == 1 {
			m.screen = ScreenRecovery
			return m, m.beginForm(formRevokeIdentity)
		}
		m.screen = ScreenOnboarding
		return m, m.beginForm(formOnboarding)
	case ScreenBranch:
		switch m.cursor {
		case 0:
			m.screen = ScreenCompose
			m.mode = ModeText
			m.setStatus(statusInfo, "")
			return m, m.draft.Focus()
		case 1:
			if m.runtime != nil {
				if m.status != noLetterStatus {
					m.setStatus(statusInfo, "Waiting by the branch...")
				}
				return m, m.startClaim()
			}
			m.screen = ScreenSearching
			m.searchID++
			searchID := m.searchID
			m.setStatus(statusInfo, "Waiting by the branch...")
			return m, tea.Tick(300*time.Millisecond, func(time.Time) tea.Msg { return searchDoneMsg{id: searchID} })
		case 2:
			m.screen = ScreenKeepsakes
			m.cursor = 0
			if m.runtime != nil {
				m.setStatus(statusInfo, "Loading keepsakes...")
				return m, m.startKeepsakes(false)
			}
		case 3:
			m.screen = ScreenSettings
		}
	case ScreenCompose:
		if err := textpolicy.ValidateBody(m.draft.Value()); err != nil {
			m.setStatus(statusError, err.Error())
			return m, nil
		}
		if err := m.prepareLetterPreview(); err != nil {
			m.setStatus(statusError, "A release identifier could not be created.")
			return m, nil
		}
		seed := uint64(0x6f726966756465)
		if m.runtime != nil {
			seed = uint64(api.FoldSeedForLetterID(m.draftID))
		}
		letter := Letter{ID: m.draftID, SenderAlias: m.identity.Alias, Body: m.draft.Value(), Age: "just now", FoldSeed: seed}
		m.setCurrent(letter)
		command := m.startAnimation(ScreenFoldPreview, false)
		return m, command
	case ScreenFoldPreview:
		if m.animating {
			return m, nil
		}
		return m, m.beginForm(formRelease)
	case ScreenDelivery:
		m.screen = ScreenBranch
		m.cursor = 0
		m.setStatus(statusInfo, "")
	case ScreenFoldedDelivery:
		if m.runtime != nil {
			m.setStatus(statusInfo, "Opening the folded letter...")
			return m, m.startOpen()
		}
		return m, m.startAnimation(ScreenUnfold, true)
	case ScreenRead:
		switch m.cursor {
		case 0:
			if m.current != nil {
				m.bindReplyDraft(m.current.ID)
			}
			m.screen = ScreenReply
			m.mode = ModeText
			m.setStatus(statusInfo, "")
			return m, m.replyDraft.Focus()
		case 1:
			if m.runtime != nil {
				m.current = nil
				m.clearReplyDraft()
				m.screen = ScreenBranch
				m.setStatus(statusSuccess, "The opened exchange is in keepsakes.")
				return m, nil
			}
			m.keepCurrent()
			m.consumeCurrentFixture()
			m.clearReplyDraft()
			m.screen = ScreenBranch
			m.setStatus(statusSuccess, "The exchange is now a keepsake.")
		case 2:
			m.reportReturn = ScreenRead
			m.reportIndex = -1
			m.reportTarget = "original"
			m.screen = ScreenReport
			return m, m.beginForm(formReport)
		case 3:
			if m.runtime != nil {
				return m, m.beginForm(formDeleteKeepsake)
			}
			m.consumeCurrentFixture()
			m.current = nil
			m.clearReplyDraft()
			m.screen = ScreenBranch
			m.cursor = 0
			m.setStatus(statusInfo, "The exchange was discarded.")
		case 4:
			return m, m.beginForm(formBlock)
		}
	case ScreenReply:
		if err := textpolicy.ValidateBody(m.replyDraft.Value()); err != nil {
			m.setStatus(statusError, err.Error())
			return m, nil
		}
		command := m.startAnimation(ScreenReplyPreview, false)
		return m, command
	case ScreenReplyPreview:
		if m.animating {
			return m, nil
		}
		return m, m.beginForm(formReplyRelease)
	case ScreenKeepsakes:
		if m.runtime != nil && m.cursor == len(m.keepsakes) && m.nextCursor != "" {
			m.setStatus(statusInfo, "Loading more keepsakes...")
			return m, m.startKeepsakes(true)
		}
		if len(m.keepsakes) > 0 && m.cursor < len(m.keepsakes) {
			if m.runtime != nil {
				m.setStatus(statusInfo, "Loading the exchange...")
				return m, m.startLetter(m.keepsakes[m.cursor])
			}
			m.setKeepsake(m.cursor)
			m.screen = ScreenKeepsakeDetail
			m.cursor = 0
		}
	case ScreenKeepsakeDetail:
		if m.runtime != nil {
			actions := m.detailActions()
			if m.cursor >= len(actions) {
				return m, nil
			}
			switch actions[m.cursor] {
			case detailReport:
				m.reportReturn = ScreenKeepsakeDetail
				m.reportIndex = m.keepsakeIndex
				m.reportTarget = m.keepsakeReportTarget()
				m.screen = ScreenReport
				return m, m.beginForm(formReport)
			case detailBlock:
				return m, m.beginForm(formBlock)
			case detailWithdraw:
				return m, m.beginForm(formWithdraw)
			case detailRemove:
				return m, m.beginForm(formDeleteKeepsake)
			}
		}
		if m.keepsakeReportable() {
			m.reportReturn = ScreenKeepsakeDetail
			m.reportIndex = m.keepsakeIndex
			m.reportTarget = m.keepsakeReportTarget()
			m.screen = ScreenReport
			return m, m.beginForm(formReport)
		}
	case ScreenSettings:
		if m.runtime == nil || m.cursor == 0 {
			return m, m.beginForm(formSettings)
		}
		if m.cursor == 1 {
			previous := m.connection
			m.connection = connectionConnecting
			m.setStatus(statusInfo, "Reconnecting to the post office...")
			pending := m.beginOperation(operationReconnect, false)
			pending.connection = previous
			return m, m.reconnectCommand(pending.id)
		}
		return m, m.beginForm(formDeleteIdentity)
	case ScreenRevocation:
		if m.registration != nil && m.registration.uncertain {
			m.setStatus(statusInfo, "Retrying registration with the same device key...")
			pending := m.beginOperation(operationRegister, true)
			return m, m.registerCommand(pending.id)
		}
		return m, m.beginForm(formRevocation)
	case ScreenRecovery:
		if m.cursor == 0 && m.device != nil {
			previous := m.connection
			m.connection = connectionConnecting
			m.setStatus(statusInfo, "Retrying authentication...")
			pending := m.beginOperation(operationReconnect, false)
			pending.connection = previous
			return m, m.reconnectCommand(pending.id)
		}
		return m, m.beginForm(formRevokeIdentity)
	}
	return m, nil
}

func (m *Model) move(delta int) {
	if m.screen == ScreenKeepsakeDetail {
		if m.runtime != nil {
			limit := m.selectionCount()
			if limit > 0 {
				m.cursor = (m.cursor + delta + limit) % limit
			}
			return
		}
		if delta > 0 {
			m.viewport.ScrollDown(delta)
		} else {
			m.viewport.ScrollUp(-delta)
		}
		return
	}
	limit := m.selectionCount()
	if limit == 0 {
		return
	}
	m.cursor = (m.cursor + delta + limit) % limit
}

func (m *Model) goTop() {
	if m.screen == ScreenKeepsakeDetail {
		if m.runtime != nil {
			m.cursor = 0
			return
		}
		m.viewport.GotoTop()
		return
	}
	m.cursor = 0
}

func (m *Model) goBottom() {
	if m.screen == ScreenKeepsakeDetail {
		if m.runtime != nil {
			if count := m.selectionCount(); count > 0 {
				m.cursor = count - 1
			}
			return
		}
		m.viewport.GotoBottom()
		return
	}
	if count := m.selectionCount(); count > 0 {
		m.cursor = count - 1
	}
}

func (m Model) selectionCount() int {
	switch m.screen {
	case ScreenSplash:
		if m.runtime != nil {
			return 2
		}
	case ScreenBranch:
		return 4
	case ScreenRead:
		if m.runtime != nil {
			return 5
		}
		return 4
	case ScreenKeepsakes:
		if m.runtime != nil && m.nextCursor != "" {
			return len(m.keepsakes) + 1
		}
		return len(m.keepsakes)
	case ScreenKeepsakeDetail:
		if m.runtime != nil {
			return len(m.detailActions())
		}
		if m.keepsakeReportable() {
			return 1
		}
	case ScreenSettings:
		if m.runtime != nil {
			return 3
		}
	case ScreenRevocation:
		if m.registration != nil && m.registration.uncertain {
			return 1
		}
	case ScreenRecovery:
		if m.device != nil {
			return 2
		}
		return 1
	}
	return 0
}

func (m *Model) back() tea.Cmd {
	if m.screen == ScreenRevocation && m.registration != nil && (m.registration.confirmed || m.registration.uncertain) {
		m.setStatus(statusInfo, "Retry registration with the same device key before leaving this screen.")
		return nil
	}
	if m.screen == ScreenRecovery && !m.localIdentity.Active && m.device != nil {
		m.setStatus(statusInfo, "Check identity creation or delete the pending identity before going back.")
		return nil
	}
	if m.runtime != nil && m.pending != nil && m.pending.busy && m.pending.mutation {
		m.setStatus(statusInfo, "Wait for the current operation to finish.")
		return nil
	}
	if m.runtime != nil && m.pending != nil && m.pending.uncertain {
		m.setStatus(statusInfo, "Retry this operation with its original identifier before leaving.")
		return nil
	}
	if m.runtime != nil && m.pending != nil {
		if m.pending.kind == operationReconnect {
			m.connection = m.pending.connection
		}
		m.pending = nil
		m.requestID++
	}
	m.animationID++
	m.searchID++
	m.formID++
	m.animating = false
	m.form = nil
	m.formData = nil
	m.formTheme = nil
	m.formKind = formNone
	m.mode = ModeNavigation
	m.cursor = 0
	switch m.screen {
	case ScreenOnboarding:
		m.screen = ScreenSplash
	case ScreenRevocation:
		if m.registration != nil {
			m.setStatus(statusInfo, "Removing the pending local identity...")
			pending := m.beginOperation(operationAbandonRegistration, true)
			return m.abandonRegistrationCommand(pending.id, false)
		}
		m.screen = ScreenSplash
	case ScreenCompose:
		m.screen = ScreenBranch
	case ScreenFoldPreview:
		m.screen = ScreenCompose
	case ScreenDelivery, ScreenFoldedDelivery, ScreenKeepsakes, ScreenSettings:
		m.screen = ScreenBranch
	case ScreenRead:
		m.clearReplyDraft()
		m.screen = ScreenBranch
	case ScreenSearching:
		m.screen = ScreenBranch
		m.setStatus(statusInfo, "Search cancelled.")
	case ScreenUnfold:
		m.screen = ScreenFoldedDelivery
	case ScreenReply:
		m.screen = ScreenRead
	case ScreenReport:
		m.screen = m.reportReturn
	case ScreenReplyPreview:
		m.screen = ScreenReply
	case ScreenKeepsakeDetail:
		m.screen = ScreenKeepsakes
	case ScreenRecovery:
		m.screen = ScreenSplash
	}
	return nil
}

func (m Model) requestQuit() (tea.Model, tea.Cmd) {
	registrationCanResume := m.screen == ScreenRevocation && m.registration != nil && (m.registration.confirmed || m.registration.uncertain)
	if m.runtime != nil && m.pending != nil && m.pending.uncertain && !registrationCanResume {
		m.setStatus(statusInfo, "Retry this operation with its original identifier before quitting.")
		return m, nil
	}
	if m.runtime != nil && m.screen == ScreenRevocation && m.registration != nil && !registrationCanResume {
		m.formID++
		m.form = nil
		m.formData = nil
		m.formTheme = nil
		m.formKind = formNone
		m.mode = ModeNavigation
		m.setStatus(statusInfo, "Removing the pending local identity...")
		pending := m.beginOperation(operationAbandonRegistration, true)
		return m, m.abandonRegistrationCommand(pending.id, true)
	}
	if m.draft.Value() == "" && m.replyDraft.Value() == "" {
		return m, tea.Quit
	}
	m.showHelp = false
	m.animationID++
	m.searchID++
	m.animating = false
	m.quitReturn = m.screen
	switch m.formKind {
	case formRelease:
		m.quitReturn = ScreenCompose
	case formReplyRelease:
		m.quitReturn = ScreenReply
	case formReport:
		m.quitReturn = m.reportReturn
	case formSettings:
		m.quitReturn = ScreenSettings
	}
	if m.quitReturn == ScreenSearching {
		m.quitReturn = ScreenBranch
		m.setStatus(statusInfo, "Search cancelled.")
	}
	return m, m.beginForm(formQuit)
}

func (m *Model) cancelForm() {
	kind := m.formKind
	m.formID++
	m.form = nil
	m.formData = nil
	m.formTheme = nil
	m.formKind = formNone
	m.mode = ModeNavigation
	switch kind {
	case formOnboarding, formFallback, formRevokeIdentity:
		m.screen = ScreenSplash
	case formRevocation:
		m.screen = ScreenRevocation
	case formRelease:
		m.screen = ScreenCompose
	case formReplyRelease:
		m.screen = ScreenReply
	case formReport:
		m.screen = m.reportReturn
	case formSettings:
		m.screen = ScreenSettings
	case formDeleteIdentity:
		m.screen = ScreenSettings
	case formBlock, formWithdraw, formDeleteKeepsake:
		m.screen = m.reportReturn
		if m.current != nil && m.keepsakeIndex >= 0 {
			m.screen = ScreenKeepsakeDetail
		} else if m.current != nil {
			m.screen = ScreenRead
		}
	case formQuit:
		m.screen = m.quitReturn
		if m.screen == ScreenFoldPreview {
			m.screen = ScreenCompose
		} else if m.screen == ScreenReplyPreview {
			m.screen = ScreenReply
		} else if m.screen == ScreenUnfold {
			m.screen = ScreenFoldedDelivery
		}
	}
}

func (m *Model) blurEditor() {
	m.draft.Blur()
	m.replyDraft.Blur()
}

func (m *Model) beginForm(kind formKind) tea.Cmd {
	data := m.initialFormData(kind)
	m.formTheme = &formThemeState{dark: m.dark, profile: m.profile, theme: m.theme, ascii: m.ascii}
	form := m.buildForm(kind, data)
	m.formKind = kind
	m.formID++
	formID := m.formID
	m.mode = ModeText
	if m.accessible {
		m.form = nil
		m.formData = nil
		return tea.Exec(&accessibleFormCommand{form: form}, func(err error) tea.Msg {
			return accessibleFormDoneMsg{id: formID, kind: kind, data: *data, err: err}
		})
	}
	m.form = form
	m.formData = data
	return form.Init()
}

func (m Model) initialFormData(kind formKind) *formData {
	data := &formData{}
	switch kind {
	case formOnboarding:
		data.alias = m.identity.Alias
	case formRevokeIdentity:
		data.credential = ""
	case formSettings:
		data.theme = m.theme
		data.reduced = m.reducedMotion
		data.ascii = m.asciiFallback
		data.accessible = m.accessible
	}
	return data
}

func (m Model) buildForm(kind formKind, data *formData) *huh.Form {
	var form *huh.Form
	switch kind {
	case formOnboarding:
		aliasInput := huh.NewInput().
			Title("Choose a private alias").
			Description("2-24 characters, one writing system, never searchable").
			CharLimit(4 * textpolicy.MaxAliasCodePoints).
			Value(&data.alias)
		if m.accessible {
			aliasInput.Validate(func(value string) error {
				_, _, err := textpolicy.NormalizeAlias(value)
				return err
			})
		}
		description := "This offline prototype stores nothing and contacts no service."
		fields := []huh.Field{aliasInput}
		if m.runtime != nil {
			description = "Create one pseudonymous identity. Your alias is visible only to matched strangers."
			fields = append([]huh.Field{huh.NewInput().Title("Private-alpha invite code").EchoMode(huh.EchoModePassword).CharLimit(64).Value(&data.invite)}, fields...)
		}
		fields = append(fields,
			huh.NewConfirm().
				Title("Losing the device key means losing access permanently.").
				Description("There is no password recovery or second device.").
				Affirmative("I understand").
				Negative("Go back").
				Value(&data.confirmed).
				Validate(func(confirmed bool) error {
					if !confirmed {
						return nil
					}
					_, _, err := textpolicy.NormalizeAlias(data.alias)
					return err
				}))
		form = huh.NewForm(
			huh.NewGroup(huh.NewNote().Title("A quiet post office").Description(description).Next(true).NextLabel("continue")),
			huh.NewGroup(fields...),
		)
	case formFallback:
		form = huh.NewForm(huh.NewGroup(
			huh.NewConfirm().
				Title("Use an owner-only local file?").
				Description("The operating-system credential store is unavailable. Orifude will restrict the device-key file to your user account.").
				Affirmative("Use owner-only file").Negative("Cancel").Value(&data.confirmed),
		))
	case formRevocation:
		credential := ""
		if m.registration != nil {
			credential = m.registration.credential
		}
		form = huh.NewForm(huh.NewGroup(
			huh.NewNote().Title("Save this delete-only credential away from this device").Description(credential).Next(true).NextLabel("I saved it"),
			huh.NewConfirm().Title("This credential will not be shown or stored again.").Affirmative("I stored it safely").Negative("Go back").Value(&data.confirmed),
		))
	case formRelease:
		form = huh.NewForm(huh.NewGroup(
			huh.NewConfirm().Title("Release this folded letter?").Affirmative("Release").Negative("Keep editing").Value(&data.confirmed),
		))
	case formReplyRelease:
		form = huh.NewForm(huh.NewGroup(
			huh.NewConfirm().Title("Release this one reply?").Affirmative("Release").Negative("Keep editing").Value(&data.confirmed),
		))
	case formReport:
		form = huh.NewForm(huh.NewGroup(
			huh.NewSelect[string]().
				Title("Why are you reporting this letter?").
				Options(
					huh.NewOption("Harassment", "harassment"),
					huh.NewOption("Hateful content", string(api.ReportReasonHatefulContent)),
					huh.NewOption("Sexual content", string(api.ReportReasonSexualContent)),
					huh.NewOption("Threats", "threats"),
					huh.NewOption("Spam or scams", string(api.ReportReasonSpamOrScams)),
					huh.NewOption("Exposed personal information", string(api.ReportReasonExposedPersonalInformation)),
					huh.NewOption("Other unsafe content", string(api.ReportReasonOtherUnsafeContent)),
				).
				Height(7).
				Value(&data.reason),
			huh.NewConfirm().
				Title("Report, burn this exchange, and block future matching?").
				Affirmative("Report and burn").
				Negative("Cancel").
				Value(&data.confirmed),
		))
	case formSettings:
		form = huh.NewForm(huh.NewGroup(
			huh.NewSelect[string]().Title("Theme and contrast").Options(
				huh.NewOption("Automatic", "auto"),
				huh.NewOption("Light", "light"),
				huh.NewOption("Dark", "dark"),
				huh.NewOption("Monochrome", "mono"),
			).Value(&data.theme),
			huh.NewConfirm().Title("Reduced motion").Affirmative("On").Negative("Off").Value(&data.reduced),
			huh.NewConfirm().Title("Printable ASCII fallback").Affirmative("On").Negative("Off").Value(&data.ascii),
			huh.NewConfirm().Title("Screen-reader forms").Affirmative("On").Negative("Off").Value(&data.accessible),
		))
	case formQuit:
		form = huh.NewForm(huh.NewGroup(
			huh.NewConfirm().Title("Discard the current draft and quit?").Affirmative("Quit").Negative("Keep writing").Value(&data.confirmed),
		))
	case formDeleteIdentity:
		form = huh.NewForm(huh.NewGroup(huh.NewConfirm().Title("Permanently delete this identity?").Description("This cannot be reversed. Your alias and device key remain reserved.").Affirmative("Delete permanently").Negative("Cancel").Value(&data.confirmed)))
	case formRevokeIdentity:
		form = huh.NewForm(huh.NewGroup(
			huh.NewInput().Title("Delete-only revocation credential").EchoMode(huh.EchoModePassword).CharLimit(64).Value(&data.credential),
			huh.NewConfirm().Title("Submit this permanent deletion request?").Affirmative("Delete identity").Negative("Cancel").Value(&data.confirmed),
		))
	case formBlock:
		form = huh.NewForm(huh.NewGroup(huh.NewConfirm().Title("Block future matching permanently?").Affirmative("Block").Negative("Cancel").Value(&data.confirmed)))
	case formWithdraw:
		form = huh.NewForm(huh.NewGroup(huh.NewConfirm().Title("Withdraw this unclaimed letter?").Affirmative("Withdraw").Negative("Cancel").Value(&data.confirmed)))
	case formDeleteKeepsake:
		form = huh.NewForm(huh.NewGroup(huh.NewConfirm().Title("Remove this keepsake from your identity?").Description("The other participant keeps their copy until they remove it.").Affirmative("Remove").Negative("Cancel").Value(&data.confirmed)))
	}
	form = form.WithTheme(m.huhTheme()).WithShowHelp(false).WithWidth(m.panelContentWidth())
	if formHeight := formHeight(kind, m.height); formHeight > 0 {
		form.WithHeight(formHeight)
	}
	return form
}

func (m Model) huhTheme() huh.Theme {
	state := m.formTheme
	if state == nil {
		state = &formThemeState{dark: m.dark, profile: m.profile, theme: m.theme, ascii: m.ascii}
	}
	return huh.ThemeFunc(func(bool) *huh.Styles {
		styles := newStyles(state.dark, state.profile, state.theme, state.ascii)
		theme := huh.ThemeBase(false)
		border := lipgloss.ThickBorder()
		selector, next, previous := "› ", "→", "←"
		selected, unselected := "✓ ", "· "
		if state.ascii {
			border = asciiBorder
			selector, next, previous = "> ", ">", "<"
			selected, unselected = "[x] ", "[ ] "
		}

		theme.Form.Base = styles.Body
		theme.Group.Title = styles.Title
		theme.Group.Description = styles.Muted
		theme.Focused.Base = styles.Body.PaddingLeft(1).BorderStyle(border).BorderLeft(true).BorderForeground(styles.Label.GetForeground())
		theme.Focused.Card = theme.Focused.Base
		theme.Focused.Title = styles.Label
		theme.Focused.NoteTitle = styles.Title.MarginBottom(1)
		theme.Focused.Description = styles.Muted
		theme.Focused.ErrorIndicator = styles.Danger.SetString(" !")
		theme.Focused.ErrorMessage = styles.Danger.SetString(" !")
		theme.Focused.SelectSelector = styles.Active.SetString(selector)
		theme.Focused.Option = styles.Body
		theme.Focused.NextIndicator = styles.Label.MarginLeft(1).SetString(next)
		theme.Focused.PrevIndicator = styles.Label.MarginRight(1).SetString(previous)
		theme.Focused.MultiSelectSelector = styles.Active.SetString(selector)
		theme.Focused.SelectedOption = styles.Active
		theme.Focused.UnselectedOption = styles.Body
		theme.Focused.SelectedPrefix = styles.Active.SetString(selected)
		theme.Focused.UnselectedPrefix = styles.Muted.SetString(unselected)
		theme.Focused.TextInput.Cursor = styles.Active
		theme.Focused.TextInput.CursorText = styles.Body
		theme.Focused.TextInput.Placeholder = styles.Muted
		theme.Focused.TextInput.Prompt = styles.Label
		theme.Focused.TextInput.Text = styles.Body

		button := styles.Active.Underline(false).Reverse(true).Padding(0, 2).MarginRight(1)
		theme.Focused.FocusedButton = button
		theme.Focused.BlurredButton = styles.Muted.Padding(0, 2).MarginRight(1)
		theme.Focused.Next = button

		theme.Blurred = theme.Focused
		theme.Blurred.Base = styles.Body.PaddingLeft(1).BorderStyle(lipgloss.HiddenBorder()).BorderLeft(true)
		theme.Blurred.Card = theme.Blurred.Base
		theme.Blurred.Title = styles.Body.Bold(true)
		theme.Blurred.SelectSelector = styles.Body.SetString("  ")
		theme.Blurred.NextIndicator = lipgloss.NewStyle()
		theme.Blurred.PrevIndicator = lipgloss.NewStyle()
		theme.Blurred.FocusedButton = styles.Label.Underline(true).Padding(0, 2).MarginRight(1)
		return theme
	})
}

func (m Model) finishForm(kind formKind, data formData) (tea.Model, tea.Cmd) {
	switch kind {
	case formOnboarding:
		alias, _, err := textpolicy.NormalizeAlias(data.alias)
		if err != nil || !data.confirmed {
			m.setStatus(statusInfo, "Identity creation was not completed.")
			m.screen = ScreenSplash
			return m, nil
		}
		if m.runtime != nil {
			m.registration = nil
			m.screen = ScreenOnboarding
			m.setStatus(statusInfo, "Preparing a protected local identity...")
			pending := m.beginOperation(operationPrepareIdentity, true)
			return m, m.prepareIdentityCommand(pending.id, alias, data.invite, false)
		}
		m.identity.Alias = alias
		m.screen = ScreenBranch
		m.cursor = 0
		m.setStatus(statusSuccess, "Welcome, "+alias+".")
	case formRelease:
		if !data.confirmed {
			m.screen = ScreenCompose
			return m, nil
		}
		if m.runtime != nil {
			m.setStatus(statusInfo, "Releasing the folded letter...")
			return m, m.startSend()
		}
		if m.current != nil {
			m.keepsakes = append(m.keepsakes, LetterSummary{Direction: "sent", Alias: "waiting for a stranger", Letter: *m.current})
		}
		m.draft.Reset()
		m.draftID = ""
		m.deliveryReply = false
		m.screen = ScreenDelivery
		m.setStatus(statusSuccess, "The letter was released into the quiet.")
	case formReplyRelease:
		if !data.confirmed {
			m.screen = ScreenReply
			return m, nil
		}
		if m.runtime != nil {
			m.setStatus(statusInfo, "Releasing the reply...")
			return m, m.startReply()
		}
		if m.current != nil {
			m.current.Reply = m.replyDraft.Value()
			m.keepCurrent()
			m.consumeCurrentFixture()
		}
		m.clearReplyDraft()
		m.deliveryReply = true
		m.screen = ScreenDelivery
		m.setStatus(statusSuccess, "Your reply was folded into the keepsake.")
	case formReport:
		if !data.confirmed {
			m.screen = m.reportReturn
			return m, nil
		}
		if m.runtime != nil {
			m.screen = m.reportReturn
			m.setStatus(statusInfo, "Reporting and burning the exchange...")
			return m, m.startReport(api.ReportReason(data.reason))
		}
		if m.reportIndex >= 0 && m.reportIndex < len(m.keepsakes) {
			m.keepsakes = append(m.keepsakes[:m.reportIndex], m.keepsakes[m.reportIndex+1:]...)
		} else {
			m.clearReplyDraft()
		}
		m.consumeCurrentFixture()
		m.current = nil
		m.keepsakeIndex = -1
		m.reportIndex = -1
		m.screen = ScreenBranch
		m.cursor = 0
		m.setStatus(statusSuccess, "Reported "+m.reportTarget+" as "+data.reason+"; the exchange was burned and future matching blocked.")
	case formSettings:
		m.theme = data.theme
		m.reducedMotion = data.reduced
		m.asciiFallback = data.ascii
		m.accessible = data.accessible
		m.ascii = m.asciiFallback || m.theme == "mono" || m.profile == colorprofile.Ascii || m.profile == colorprofile.NoTTY
		m.refreshPresentation()
		if m.runtime != nil {
			m.screen = ScreenSettings
			pending := m.beginOperation(operationSettings, true)
			return m, m.saveSettingsCommand(pending.id)
		}
		m.screen = ScreenBranch
		m.setStatus(statusSuccess, "Settings applied for this run.")
	case formFallback:
		if !data.confirmed || m.registration == nil {
			m.registration = nil
			m.screen = ScreenSplash
			m.setStatus(statusInfo, "Identity creation was not completed.")
			return m, nil
		}
		m.screen = ScreenOnboarding
		m.setStatus(statusInfo, "Creating the owner-only local key file...")
		pending := m.beginOperation(operationPrepareIdentity, true)
		return m, m.prepareIdentityCommand(pending.id, m.registration.alias, m.registration.invite, true)
	case formRevocation:
		if !data.confirmed || m.registration == nil {
			m.setStatus(statusInfo, "Store the credential before continuing.")
			m.screen = ScreenRevocation
			return m, m.beginForm(formRevocation)
		}
		m.screen = ScreenRevocation
		m.setStatus(statusInfo, "Creating the identity...")
		pending := m.beginOperation(operationRegister, true)
		return m, m.registerCommand(pending.id)
	case formDeleteIdentity:
		if !data.confirmed {
			m.screen = ScreenSettings
			return m, nil
		}
		m.screen = ScreenSettings
		m.setStatus(statusInfo, "Deleting the identity...")
		return m, m.startDeleteIdentity()
	case formRevokeIdentity:
		credential := data.credential
		data.credential = ""
		if !data.confirmed || credential == "" {
			m.screen = ScreenSplash
			return m, nil
		}
		m.screen = ScreenRecovery
		m.setStatus(statusInfo, "Submitting the delete request...")
		return m, m.startRevokeIdentity(credential)
	case formBlock:
		if data.confirmed {
			m.setStatus(statusInfo, "Blocking future matching...")
			return m, m.startBlock()
		}
	case formWithdraw:
		if data.confirmed {
			m.setStatus(statusInfo, "Withdrawing the letter...")
			return m, m.startWithdraw()
		}
	case formDeleteKeepsake:
		if data.confirmed {
			m.setStatus(statusInfo, "Removing the keepsake...")
			return m, m.startDeleteKeepsake()
		}
	case formQuit:
		if data.confirmed {
			return m, tea.Quit
		}
		m.screen = m.quitReturn
		if m.screen == ScreenFoldPreview {
			m.screen = ScreenCompose
		} else if m.screen == ScreenReplyPreview {
			m.screen = ScreenReply
		} else if m.screen == ScreenUnfold {
			m.screen = ScreenFoldedDelivery
		}
	}
	return m, nil
}

func (m Model) updateFold(message foldTickMsg) (tea.Model, tea.Cmd) {
	if !m.animating || message.id != m.animationID {
		return m, nil
	}
	frames := foldFrames(m.currentSeed(), m.width, m.ascii)
	switch m.screen {
	case ScreenFoldPreview, ScreenReplyPreview:
		if m.foldFrame < len(frames)-1 {
			m.foldFrame++
			return m, nextFoldTick(message.id)
		}
		m.animating = false
		if m.screen == ScreenFoldPreview {
			return m, m.beginForm(formRelease)
		}
		return m, m.beginForm(formReplyRelease)
	case ScreenUnfold:
		if m.foldFrame > 0 {
			m.foldFrame--
			return m, nextFoldTick(message.id)
		}
		m.animating = false
		m.screen = ScreenRead
		m.cursor = 0
		if m.runtime == nil {
			m.incomingState = fixtureOpened
		}
	}
	return m, nil
}

func (m *Model) keepCurrent() {
	if m.current == nil {
		return
	}
	for _, summary := range m.keepsakes {
		if summary.Letter.FoldSeed == m.current.FoldSeed {
			return
		}
	}
	m.keepsakes = append(m.keepsakes, LetterSummary{Direction: "received", Alias: m.current.SenderAlias, Letter: *m.current})
}

func (m *Model) consumeCurrentFixture() {
	if m.current != nil && m.current.FoldSeed == incomingFixture().FoldSeed {
		m.incomingState = fixtureConsumed
	}
}

func (m Model) keepsakeReportable() bool {
	if m.runtime != nil {
		if m.current == nil {
			return false
		}
		return m.current.Role == api.LetterRoleRecipient && m.current.Body != "" || m.current.Role == api.LetterRoleSender && m.current.Reply != ""
	}
	if m.keepsakeIndex < 0 || m.keepsakeIndex >= len(m.keepsakes) {
		return false
	}
	summary := m.keepsakes[m.keepsakeIndex]
	return summary.Direction == "received" || summary.Direction == "sent" && summary.Letter.Reply != ""
}

func (m Model) keepsakeReportTarget() string {
	if m.runtime != nil && m.current != nil {
		if m.current.Role == api.LetterRoleSender {
			return "reply"
		}
		return "original letter"
	}
	if m.keepsakeIndex >= 0 && m.keepsakeIndex < len(m.keepsakes) && m.keepsakes[m.keepsakeIndex].Direction == "sent" {
		return "reply"
	}
	return "original letter"
}

func (m Model) validatePaste(content string) error {
	if !utf8.ValidString(content) {
		return textpolicy.ErrBodyUTF8
	}
	for _, r := range content {
		if unsafeTextRune(r) && (r != '\n' || m.form != nil) {
			return textpolicy.ErrBodyControl
		}
	}
	if m.form != nil {
		return nil
	}
	var current, selected string
	if m.screen == ScreenCompose {
		current = m.draft.Value()
		selected = m.draft.SelectedText()
	} else if m.screen == ScreenReply {
		current = m.replyDraft.Value()
		selected = m.replyDraft.SelectedText()
	} else {
		return nil
	}
	if len(current)-len(selected)+len(content) > textpolicy.MaxBodyBytes {
		return textpolicy.ErrBodyBytes
	}
	if utf8.RuneCountInString(current)-utf8.RuneCountInString(selected)+utf8.RuneCountInString(content) > textpolicy.MaxBodyCodePoints {
		return textpolicy.ErrBodyCodePoints
	}
	return nil
}

func incomingFixture() Letter {
	return Letter{
		SenderAlias: "aoi",
		Body:        "There is a bench near the last train platform where someone leaves a paper crane each Friday. Today it was blue.",
		Age:         "a few minutes ago",
		FoldSeed:    0x6272616e6368,
	}
}

func bodyCounter(body string) string {
	return fmt.Sprintf("%d/%d code points", utf8.RuneCountInString(body), textpolicy.MaxBodyCodePoints)
}

func onOff(value bool) string {
	if value {
		return "on"
	}
	return "off"
}
