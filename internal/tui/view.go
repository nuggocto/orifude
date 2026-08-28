package tui

import (
	"fmt"
	"strings"

	"charm.land/bubbles/v2/key"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/nuggocto/orifude/internal/api"
)

const unicodeMark = `        ╱╲
   ╭────╯╰────╮
 ╱              ╲
╰──────╮  ╭──────╯
       │  │
       ╲  ╱
        ╲╱`

const asciiMark = `        /\
   +---'  '---+
  /            \
 +-----+  +-----+
       |  |
       \  /
        \/`

// View renders the current state without mutating it.
func (m Model) View() tea.View {
	view := tea.NewView(m.render())
	view.AltScreen = true
	view.WindowTitle = "Orifude"
	return view
}

func (m Model) render() string {
	if m.layout() == layoutTooSmall {
		return m.renderTooSmall()
	}

	body := m.renderHelp()
	if !m.showHelp {
		body = m.renderScreen()
		separator := "\n\n"
		if m.layout() == layoutText {
			separator = "\n"
		}
		if m.status != "" {
			body += separator + m.renderStatus()
		}
		body += separator + m.renderKeyHelp()
	}

	mode := m.layout()
	panelStyle, panelWidth := m.panel()
	panel := panelStyle.Width(panelWidth).Render(body)

	var content string
	switch mode {
	case layoutWide:
		content = lipgloss.JoinHorizontal(lipgloss.Center, m.styles.Art.Render(m.mark()), "      ", panel)
	case layoutCompact:
		content = lipgloss.JoinVertical(lipgloss.Center, m.styles.Art.Render(compactMark(m.ascii)), "", panel)
	case layoutText:
		content = panel
	}
	return lipgloss.Place(m.width, m.height, lipgloss.Center, lipgloss.Center, content)
}

func (m Model) panel() (lipgloss.Style, int) {
	mode := m.layout()
	width := min(max(m.width-6, 24), 70)
	if mode == layoutWide {
		width = min(max(m.width-34, 48), 76)
	}
	style := m.styles.Panel
	if mode == layoutText {
		style = style.BorderLeft(false).Padding(0)
	}
	return style, width
}

func (m Model) panelContentWidth() int {
	style, width := m.panel()
	return min(max(width-style.GetHorizontalFrameSize(), 20), 68)
}

func (m Model) renderScreen() string {
	title := m.styles.Title.Render(m.screenTitle())
	if m.form != nil {
		if m.screen == ScreenFoldPreview || m.screen == ScreenReplyPreview {
			if m.layout() == layoutText {
				return title + "\n" + m.form.View()
			}
			return title + "\n\n" + m.renderFold() + "\n\n" + m.form.View()
		}
		return title + "\n\n" + m.form.View()
	}

	switch m.screen {
	case ScreenSplash:
		if m.runtime != nil {
			return title + "\n\nSend a letter into the quiet.\nLet one stranger find it.\n\n" + m.renderChoices([]string{
				"Create an identity",
				"Delete a lost identity",
			})
		}
		separator := "·"
		if m.ascii {
			separator = "-"
		}
		return title + "\n\nSend a letter into the quiet.\nLet one stranger find it.\n\n" +
			m.styles.Muted.Render("Offline prototype "+separator+" no letter leaves this process")
	case ScreenOnboarding:
		return title + "\n\nPreparing onboarding..."
	case ScreenBranch:
		alias := neutralizeTerminalText(m.identity.Alias)
		connection := ""
		if m.runtime != nil && m.connection != connectionOnline {
			connection = "\n" + m.styles.Muted.Render("Post office: "+m.connectionLabel())
		}
		return title + "\n\nWelcome to the branch, " + alias + "." + connection + "\n\n" + m.renderChoices([]string{
			"Fold a letter",
			"Wait by the branch",
			"Keepsakes",
			"Settings",
		})
	case ScreenCompose:
		return title + "\n\n" + m.renderEditor(m.draft.View(), m.draft.Width(), m.draft.Height()) + "\n" + m.styles.Counter.Render(bodyCounter(m.draft.Value())) +
			"\n\n" + m.editorInstruction()
	case ScreenFoldPreview, ScreenReplyPreview:
		progress := "·"
		if m.ascii {
			progress = "."
		}
		message := m.styles.Muted.Render("folding " + strings.Repeat(progress, m.foldFrame+1))
		if !m.animating {
			message = m.styles.Active.Render("The fold is ready.") + m.styles.Muted.Render("  [enter] continue")
		}
		return title + "\n\n" + m.renderFold() + "\n\n" + message
	case ScreenDelivery:
		if m.deliveryReply {
			return title + "\n\nYour one reply has joined the exchange.\nThe completed letter is available in keepsakes.\n\n[enter] return to the branch"
		}
		return title + "\n\nYour letter is waiting for one unrelated stranger.\nYou can reread it from keepsakes.\n\n[enter] return to the branch"
	case ScreenSearching:
		return title + "\n\nThe post office is listening beneath the branches..."
	case ScreenFoldedDelivery:
		if m.runtime != nil {
			return title + "\n\n" + m.renderFold() + "\n\nA folded letter arrived " + neutralizeTerminalText(m.current.Age) + ".\n\n[enter] unfold"
		}
		return title + "\n\n" + m.renderFold() + "\n\nA letter from " + neutralizeTerminalText(m.current.SenderAlias) + " arrived " + neutralizeTerminalText(m.current.Age) + ".\n\n[enter] unfold"
	case ScreenUnfold:
		return title + "\n\n" + m.renderFold() + "\n\n" + m.styles.Muted.Render("opening the creases...")
	case ScreenRead:
		from := m.styles.Label.Render("From " + neutralizeTerminalText(m.current.SenderAlias))
		choices := []string{"Fold a reply", "Keep without replying", "Report and burn", "Discard"}
		if m.runtime != nil {
			choices = append(choices, "Block future matching")
		}
		if m.layout() == layoutText {
			return title + "\n" + from + "\n" + m.renderLetter() + "\n" + m.renderChoices(choices)
		}
		return title + "\n\n" + from + "\n\n" + m.renderLetter() + "\n\n" + m.renderChoices(choices)
	case ScreenReply:
		return title + "\n\n" + m.renderEditor(m.replyDraft.View(), m.replyDraft.Width(), m.replyDraft.Height()) + "\n" + m.styles.Counter.Render(bodyCounter(m.replyDraft.Value())) +
			"\n\n" + m.editorInstruction()
	case ScreenKeepsakes:
		if len(m.keepsakes) == 0 {
			return title + "\n\nNo keepsakes yet."
		}
		choices := make([]string, 0, len(m.keepsakes)+1)
		separator := " · "
		if m.ascii {
			separator = " - "
		}
		for _, summary := range m.keepsakes {
			choices = append(choices, summary.Direction+separator+neutralizeTerminalText(summary.Alias))
		}
		if m.runtime != nil && m.nextCursor != "" {
			choices = append(choices, "Load more")
		}
		return title + "\n\n" + m.renderChoices(choices)
	case ScreenKeepsakeDetail:
		if m.current == nil {
			return title + "\n\nThis keepsake is unavailable."
		}
		separator := "\n\n"
		if m.layout() == layoutText {
			separator = "\n"
		}
		heading := "From " + neutralizeTerminalText(m.current.SenderAlias)
		if m.runtime != nil && m.current.Role == api.LetterRoleSender {
			heading = "Sent by you"
			if m.current.SenderAlias != "" {
				heading += " · exchange with " + neutralizeTerminalText(m.current.SenderAlias)
			}
		}
		content := title + separator + m.styles.Label.Render(heading) + separator + m.renderLetter()
		if m.runtime != nil {
			actions := m.detailActions()
			choices := make([]string, len(actions))
			for index, action := range actions {
				choices[index] = detailActionLabel(action)
			}
			content += separator + m.renderChoices(choices)
		} else if m.keepsakeReportable() {
			content += separator + m.renderChoices([]string{"Report and burn"})
		}
		return content
	case ScreenReport:
		return title + "\n\nPreparing report reasons..."
	case ScreenSettings:
		if m.runtime != nil {
			return title + "\n\n" + strings.Join([]string{
				m.styles.Label.Render("Connection") + "  " + m.connectionLabel(),
				m.styles.Label.Render("Identity") + "    " + neutralizeTerminalText(m.identity.Alias),
				m.styles.Label.Render("Device") + "      " + shortenedThumbprint(m.identity.Thumbprint),
			}, "\n") + "\n\n" + m.renderChoices([]string{"Display and accessibility", "Reconnect", "Permanently delete identity"})
		}
		return title + "\n\n" + strings.Join([]string{
			m.styles.Label.Render("Connection") + "  offline prototype",
			m.styles.Label.Render("Identity") + "    " + neutralizeTerminalText(m.identity.Alias) + " (process-local)",
			m.styles.Label.Render("Local data") + "  cleared when Orifude exits",
			m.styles.Label.Render("About") + "       keyboard-only folded letters",
		}, "\n") + "\n\n[enter] edit display and accessibility"
	case ScreenRevocation:
		if m.registration != nil && m.registration.uncertain {
			if m.pending != nil && m.pending.busy {
				return title + "\n\nChecking whether identity creation completed..."
			}
			return title + "\n\nThe registration result is unknown. Orifude kept the same device key and registration details in memory.\n\n" +
				m.renderChoices([]string{"Retry registration with the same device key"})
		}
		return title + "\n\nPreparing the delete-only credential..."
	case ScreenRecovery:
		if !m.localIdentity.Active && m.device != nil {
			return title + "\n\nIdentity creation could not be confirmed. Orifude kept the original device key. Check again when the post office is available, or delete the pending identity with the credential you saved.\n\n" +
				m.renderChoices([]string{"Check identity creation", "Delete with revocation credential"})
		}
		choices := []string{"Delete with revocation credential"}
		if m.device != nil {
			choices = []string{"Retry authentication", "Delete with revocation credential"}
		}
		return title + "\n\nThis local identity cannot currently authenticate. Orifude will never replace its key silently.\n\n" + m.renderChoices(choices)
	default:
		return title
	}
}

func (m Model) renderTooSmall() string {
	if m.width <= 0 || m.height <= 0 {
		return ""
	}
	quitHelp := "q quit"
	if (m.form == nil && m.mode == ModeText) || m.formAcceptsText() {
		quitHelp = "esc then q"
	}
	lines := []string{"Resize terminal", "Need 56x18", fmt.Sprintf("Now %dx%d", m.width, m.height), quitHelp}
	if m.formKind == formQuit {
		lines = []string{"Draft exists", "y quit", "n keep"}
	}
	if len(lines) > m.height {
		lines = lines[:m.height]
	}
	for index, line := range lines {
		runes := []rune(line)
		if len(runes) > m.width {
			lines[index] = string(runes[:m.width])
		}
	}
	return lipgloss.Place(m.width, m.height, lipgloss.Center, lipgloss.Center, strings.Join(lines, "\n"))
}

func (m Model) renderHelp() string {
	groups := [][]key.Binding{m.navigationBindings()}
	return m.styles.Title.Render("Help") + "\n\n" + m.help.FullHelpView(groups) + "\n\n[? or b] close help"
}

func (m Model) renderKeyHelp() string {
	if m.form != nil {
		bindings := []key.Binding{
			binding("tab", "next"),
			binding("shift+tab", "previous"),
			binding("enter", "select"),
		}
		if m.formAcceptsText() {
			bindings = append(bindings, binding("esc", "finish typing"))
		} else {
			bindings = append(bindings, binding("b", "back"), binding("?", "help"), binding("q/ctrl+c", "quit"))
		}
		return m.styles.Help.Render(m.help.ShortHelpView(bindings))
	}
	if m.mode == ModeText {
		return m.styles.Help.Render(m.help.ShortHelpView([]key.Binding{
			binding("type", "write"),
			binding("esc", "navigation"),
		}))
	}
	return m.styles.Help.Render(m.help.ShortHelpView(m.navigationBindings()))
}

func (m Model) navigationBindings() []key.Binding {
	bindings := []key.Binding{binding("?", "help"), binding("q/ctrl+c", "quit")}
	selection := []key.Binding{
		binding("j/down", "next"),
		binding("k/up", "previous"),
		binding("enter", "select item"),
		binding("g g/home", "first"),
		binding("G/end", "last"),
	}
	switch m.screen {
	case ScreenSplash:
		if m.runtime != nil {
			bindings = append(append(selection, binding("a", "screen reader")), bindings...)
		} else {
			bindings = append([]key.Binding{binding("enter", "begin"), binding("a", "screen reader")}, bindings...)
		}
	case ScreenBranch:
		bindings = append(selection, bindings...)
	case ScreenRead:
		bindings = append(append(selection,
			binding("ctrl+u/ctrl+d", "half page"),
			binding("ctrl+b/ctrl+f", "full page"),
			binding("b", "back")), bindings...)
	case ScreenKeepsakes:
		bindings = append(append(selection, binding("b", "back")), bindings...)
	case ScreenCompose, ScreenReply:
		bindings = append([]key.Binding{binding("i", "edit"), binding("enter", "preview"), binding("b", "back")}, bindings...)
	case ScreenFoldedDelivery, ScreenDelivery:
		bindings = append([]key.Binding{binding("enter", "continue"), binding("b", "back")}, bindings...)
	case ScreenKeepsakeDetail:
		if m.runtime != nil {
			bindings = append(append(selection, binding("b", "back")), bindings...)
			break
		}
		detail := []key.Binding{
			binding("b", "back"),
			binding("j/down", "scroll down"),
			binding("k/up", "scroll up"),
			binding("ctrl+u/ctrl+d", "half page"),
			binding("ctrl+b/ctrl+f", "full page"),
			binding("g g/home", "top"),
			binding("G/end", "bottom"),
		}
		if m.keepsakeReportable() {
			detail = append(detail, binding("enter", "report"))
		}
		bindings = append(detail, bindings...)
	case ScreenSettings:
		bindings = append(append(selection, binding("b", "back")), bindings...)
	case ScreenRecovery:
		if !m.localIdentity.Active && m.device != nil {
			bindings = append(selection, bindings...)
		} else {
			bindings = append(append(selection, binding("b", "back")), bindings...)
		}
	case ScreenRevocation:
		if m.registration != nil && m.registration.uncertain {
			bindings = append([]key.Binding{binding("enter", "retry registration")}, bindings...)
		} else {
			bindings = append([]key.Binding{binding("b", "back")}, bindings...)
		}
	default:
		bindings = append([]key.Binding{binding("b", "back")}, bindings...)
	}
	return bindings
}

func binding(keys, description string) key.Binding {
	return key.NewBinding(key.WithKeys(strings.Fields(keys)...), key.WithHelp(keys, description))
}

func (m Model) renderChoices(choices []string) string {
	var out strings.Builder
	for index, choice := range choices {
		marker := "·"
		if m.ascii {
			marker = "-"
		}
		markerStyle := m.styles.Muted
		style := m.styles.Body
		dangerous := m.screen == ScreenRead && index == 2
		if dangerous {
			style = m.styles.Danger
		}
		if index == m.cursor {
			marker = "›"
			if m.ascii {
				marker = ">"
			}
			markerStyle = m.styles.Active
			style = m.styles.Active
			if dangerous {
				markerStyle = m.styles.Danger
				style = m.styles.Danger.Underline(true)
			}
		}
		fmt.Fprintf(&out, "%s %s", markerStyle.Render(marker), style.Render(choice))
		if index < len(choices)-1 {
			out.WriteByte('\n')
		}
	}
	return out.String()
}

func (m Model) renderStatus() string {
	prefix := "· "
	style := m.styles.Label
	if m.ascii {
		prefix = "- "
	}
	switch m.statusKind {
	case statusSuccess:
		prefix = "✓ "
		if m.ascii {
			prefix = "+ "
		}
		style = m.styles.Active
	case statusError:
		prefix = "! "
		style = m.styles.Danger
	}
	return style.Render(prefix + neutralizeTerminalText(m.status))
}

func (m Model) renderFold() string {
	frames := foldFrames(m.currentSeed(), m.width, m.ascii)
	index := min(max(m.foldFrame, 0), len(frames)-1)
	seal := "◆"
	if m.ascii {
		seal = "*"
	}
	sealStyle := m.styles.Active.Underline(false)
	frame := strings.Replace(frames[index], seal, sealStyle.Render(seal), 1)
	return m.styles.Label.Render(frame)
}

func (m Model) renderLetter() string {
	style := m.styles.Paper
	return style.Width(m.viewport.Width() + style.GetHorizontalFrameSize()).Render(m.viewport.View())
}

func (m Model) renderEditor(content string, width, height int) string {
	style := m.styles.Paper
	if m.mode == ModeText {
		style = style.BorderForeground(m.styles.Active.GetForeground())
	}
	return style.Width(width + style.GetHorizontalFrameSize()).Height(height).Render(content)
}

func (m Model) editorInstruction() string {
	if m.mode == ModeText {
		return m.styles.Muted.Render("Text mode. Press esc when the letter is ready.")
	}
	return m.styles.Muted.Render("Navigation mode. Press i to edit or enter to preview.")
}

func (m Model) layout() layoutMode {
	switch {
	case m.width < 56 || m.height < 18:
		return layoutTooSmall
	case m.width >= 100 && m.height >= 30:
		return layoutWide
	case m.width >= 72 && m.height >= 24:
		return layoutCompact
	default:
		return layoutText
	}
}

func (m Model) mark() string {
	if m.ascii {
		return normalizeArt(asciiMark)
	}
	return normalizeArt(unicodeMark)
}

func compactMark(ascii bool) string {
	if ascii {
		return "ORIFUDE  -  folded beneath the branch"
	}
	return "ORIFUDE  ·  folded beneath the branch"
}

func (m Model) connectionLabel() string {
	switch m.connection {
	case connectionConnecting:
		return "connecting"
	case connectionOnline:
		return "connected"
	case connectionOffline:
		return "offline"
	case connectionInvalidIdentity:
		return "identity unavailable"
	default:
		return "local demo"
	}
}

func shortenedThumbprint(value string) string {
	value = neutralizeTerminalText(value)
	if len(value) <= 14 {
		return value
	}
	return value[:8] + "..." + value[len(value)-4:]
}

func (m Model) screenTitle() string {
	switch m.screen {
	case ScreenSplash:
		return "Orifude"
	case ScreenOnboarding:
		return "First run"
	case ScreenBranch:
		return "The branch"
	case ScreenCompose:
		return "Fold a letter"
	case ScreenFoldPreview:
		return "Fold preview"
	case ScreenDelivery:
		return "Delivery receipt"
	case ScreenSearching:
		return "Wait by the branch"
	case ScreenFoldedDelivery:
		return "Folded delivery"
	case ScreenUnfold:
		return "Unfold"
	case ScreenRead:
		return "A letter"
	case ScreenReply:
		return "Fold a reply"
	case ScreenReplyPreview:
		return "Reply preview"
	case ScreenKeepsakes:
		return "Keepsakes"
	case ScreenKeepsakeDetail:
		return "Keepsake"
	case ScreenReport:
		return "Report and burn"
	case ScreenSettings:
		return "Settings"
	case ScreenRevocation:
		return "Delete-only credential"
	case ScreenRecovery:
		return "Identity unavailable"
	default:
		return "Orifude"
	}
}
