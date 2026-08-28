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
			m.setStatus(statusInfo, "No letter is waiting right now.")
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
			m.back()
			return m, nil
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
		m.back()
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
	switch m.screen {
	case ScreenSplash:
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
			m.screen = ScreenSearching
			m.searchID++
			searchID := m.searchID
			m.setStatus(statusInfo, "Waiting by the branch...")
			return m, tea.Tick(300*time.Millisecond, func(time.Time) tea.Msg { return searchDoneMsg{id: searchID} })
		case 2:
			m.screen = ScreenKeepsakes
			m.cursor = 0
		case 3:
			m.screen = ScreenSettings
		}
	case ScreenCompose:
		if err := textpolicy.ValidateBody(m.draft.Value()); err != nil {
			m.setStatus(statusError, err.Error())
			return m, nil
		}
		letter := Letter{SenderAlias: m.identity.Alias, Body: m.draft.Value(), Age: "just now", FoldSeed: 0x6f726966756465}
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
		return m, m.startAnimation(ScreenUnfold, true)
	case ScreenRead:
		switch m.cursor {
		case 0:
			m.screen = ScreenReply
			m.mode = ModeText
			m.setStatus(statusInfo, "")
			return m, m.replyDraft.Focus()
		case 1:
			m.keepCurrent()
			m.consumeCurrentFixture()
			m.replyDraft.Reset()
			m.screen = ScreenBranch
			m.setStatus(statusSuccess, "The exchange is now a keepsake.")
		case 2:
			m.reportReturn = ScreenRead
			m.reportIndex = -1
			m.reportTarget = "original"
			m.screen = ScreenReport
			return m, m.beginForm(formReport)
		case 3:
			m.consumeCurrentFixture()
			m.current = nil
			m.replyDraft.Reset()
			m.screen = ScreenBranch
			m.cursor = 0
			m.setStatus(statusInfo, "The exchange was discarded.")
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
		if len(m.keepsakes) > 0 {
			m.setKeepsake(m.cursor)
			m.screen = ScreenKeepsakeDetail
			m.cursor = 0
		}
	case ScreenKeepsakeDetail:
		if m.keepsakeReportable() {
			m.reportReturn = ScreenKeepsakeDetail
			m.reportIndex = m.keepsakeIndex
			m.reportTarget = m.keepsakeReportTarget()
			m.screen = ScreenReport
			return m, m.beginForm(formReport)
		}
	case ScreenSettings:
		return m, m.beginForm(formSettings)
	}
	return m, nil
}

func (m *Model) move(delta int) {
	if m.screen == ScreenKeepsakeDetail {
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
		m.viewport.GotoTop()
		return
	}
	m.cursor = 0
}

func (m *Model) goBottom() {
	if m.screen == ScreenKeepsakeDetail {
		m.viewport.GotoBottom()
		return
	}
	if count := m.selectionCount(); count > 0 {
		m.cursor = count - 1
	}
}

func (m Model) selectionCount() int {
	switch m.screen {
	case ScreenBranch:
		return 4
	case ScreenRead:
		return 4
	case ScreenKeepsakes:
		return len(m.keepsakes)
	case ScreenKeepsakeDetail:
		if m.keepsakeReportable() {
			return 1
		}
	}
	return 0
}

func (m *Model) back() {
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
	case ScreenCompose:
		m.screen = ScreenBranch
	case ScreenFoldPreview:
		m.screen = ScreenCompose
	case ScreenDelivery, ScreenFoldedDelivery, ScreenRead, ScreenKeepsakes, ScreenSettings:
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
	}
}

func (m Model) requestQuit() (tea.Model, tea.Cmd) {
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
	case formOnboarding:
		m.screen = ScreenSplash
	case formRelease:
		m.screen = ScreenCompose
	case formReplyRelease:
		m.screen = ScreenReply
	case formReport:
		m.screen = m.reportReturn
	case formSettings:
		m.screen = ScreenSettings
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
		form = huh.NewForm(
			huh.NewGroup(
				huh.NewNote().
					Title("A quiet post office").
					Description("This offline prototype stores nothing and contacts no service.").
					Next(true).
					NextLabel("continue"),
			),
			huh.NewGroup(
				aliasInput,
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
					}),
			),
		)
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
					huh.NewOption("Hateful content", "hateful content"),
					huh.NewOption("Sexual content", "sexual content"),
					huh.NewOption("Threats", "threats"),
					huh.NewOption("Spam or scams", "spam or scams"),
					huh.NewOption("Exposed personal information", "personal information"),
					huh.NewOption("Other unsafe content", "other unsafe content"),
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
		m.identity.Alias = alias
		m.screen = ScreenBranch
		m.cursor = 0
		m.setStatus(statusSuccess, "Welcome, "+alias+".")
	case formRelease:
		if !data.confirmed {
			m.screen = ScreenCompose
			return m, nil
		}
		if m.current != nil {
			m.keepsakes = append(m.keepsakes, LetterSummary{Direction: "sent", Alias: "waiting for a stranger", Letter: *m.current})
		}
		m.draft.Reset()
		m.deliveryReply = false
		m.screen = ScreenDelivery
		m.setStatus(statusSuccess, "The letter was released into the quiet.")
	case formReplyRelease:
		if !data.confirmed {
			m.screen = ScreenReply
			return m, nil
		}
		if m.current != nil {
			m.current.Reply = m.replyDraft.Value()
			m.keepCurrent()
			m.consumeCurrentFixture()
		}
		m.replyDraft.Reset()
		m.deliveryReply = true
		m.screen = ScreenDelivery
		m.setStatus(statusSuccess, "Your reply was folded into the keepsake.")
	case formReport:
		if !data.confirmed {
			m.screen = m.reportReturn
			return m, nil
		}
		if m.reportIndex >= 0 && m.reportIndex < len(m.keepsakes) {
			m.keepsakes = append(m.keepsakes[:m.reportIndex], m.keepsakes[m.reportIndex+1:]...)
		} else {
			m.replyDraft.Reset()
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
		m.screen = ScreenBranch
		m.setStatus(statusSuccess, "Settings applied for this run.")
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
		m.incomingState = fixtureOpened
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
	if m.keepsakeIndex < 0 || m.keepsakeIndex >= len(m.keepsakes) {
		return false
	}
	summary := m.keepsakes[m.keepsakeIndex]
	return summary.Direction == "received" || summary.Direction == "sent" && summary.Letter.Reply != ""
}

func (m Model) keepsakeReportTarget() string {
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
