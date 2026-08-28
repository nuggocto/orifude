package tui

import (
	"strings"
	"time"

	"charm.land/bubbles/v2/help"
	"charm.land/bubbles/v2/textarea"
	"charm.land/bubbles/v2/viewport"
	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"github.com/charmbracelet/colorprofile"
	"github.com/charmbracelet/x/ansi"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/identity"
	"github.com/nuggocto/orifude/internal/textpolicy"
)

// Screen is one complete TUI destination.
type Screen uint8

const (
	ScreenSplash Screen = iota
	ScreenOnboarding
	ScreenBranch
	ScreenCompose
	ScreenFoldPreview
	ScreenDelivery
	ScreenSearching
	ScreenFoldedDelivery
	ScreenUnfold
	ScreenRead
	ScreenReply
	ScreenReplyPreview
	ScreenKeepsakes
	ScreenKeepsakeDetail
	ScreenReport
	ScreenSettings
	ScreenRevocation
	ScreenRecovery
)

// InputMode separates application navigation from text entry.
type InputMode uint8

const (
	ModeNavigation InputMode = iota
	ModeText
)

type layoutMode uint8

const (
	layoutWide layoutMode = iota
	layoutCompact
	layoutText
	layoutTooSmall
)

type fixtureState uint8

const (
	fixtureAvailable fixtureState = iota
	fixtureFolded
	fixtureOpened
	fixtureConsumed
)

type formKind uint8

const (
	formNone formKind = iota
	formOnboarding
	formRelease
	formReplyRelease
	formReport
	formSettings
	formQuit
	formFallback
	formRevocation
	formDeleteIdentity
	formRevokeIdentity
	formBlock
	formWithdraw
	formDeleteKeepsake
)

type formData struct {
	alias      string
	confirmed  bool
	reason     string
	theme      string
	reduced    bool
	ascii      bool
	accessible bool
	invite     string
	credential string
}

type formThemeState struct {
	dark    bool
	profile colorprofile.Profile
	theme   string
	ascii   bool
}

type statusKind uint8

const (
	statusInfo statusKind = iota
	statusSuccess
	statusError
)

type Identity struct {
	Alias      string
	Thumbprint string
}

type Letter struct {
	ID          string
	Role        api.LetterRole
	State       api.LetterState
	SenderAlias string
	Body        string
	Reply       string
	Age         string
	FoldSeed    uint64
	CreatedAt   time.Time
	ClaimExpiry time.Time
}

type LetterSummary struct {
	ID        string
	Role      api.LetterRole
	State     api.LetterState
	Direction string
	Alias     string
	Letter    Letter
}

type connectionState uint8

const (
	connectionLocal connectionState = iota
	connectionConnecting
	connectionOnline
	connectionOffline
	connectionInvalidIdentity
)

type operationKind uint8

const (
	operationNone operationKind = iota
	operationBootstrap
	operationPrepareIdentity
	operationRegister
	operationReconnect
	operationSend
	operationClaim
	operationOpen
	operationReply
	operationKeepsakes
	operationLetter
	operationReport
	operationBlock
	operationWithdraw
	operationDeleteKeepsake
	operationDeleteIdentity
	operationRevokeIdentity
	operationSettings
)

type pendingOperation struct {
	id         uint64
	kind       operationKind
	screen     Screen
	busy       bool
	mutation   bool
	clientID   string
	body       string
	target     api.ReportTarget
	reason     api.ReportReason
	connection connectionState
}

type pendingRegistration struct {
	alias      string
	invite     string
	credential string
	key        *auth.DeviceKey
	device     *api.DeviceClient
	uncertain  bool
}

// Runtime contains process-long online dependencies. Neither dependency stores
// credentials in the Bubble Tea model itself.
type Runtime struct {
	Client  *api.Client
	Store   *identity.Store
	Version string
}

// Model owns all prototype state. It is intentionally process-local.
type Model struct {
	runtime       *Runtime
	device        *api.DeviceClient
	localIdentity identity.Profile
	connection    connectionState
	requestID     uint64
	pending       *pendingOperation
	registration  *pendingRegistration
	nextCursor    string
	latestVersion string
	screen        Screen
	mode          InputMode
	width         int
	height        int
	dark          bool
	profile       colorprofile.Profile
	theme         string
	reducedMotion bool
	accessible    bool
	asciiFallback bool
	ascii         bool
	identity      Identity
	draft         textarea.Model
	replyDraft    textarea.Model
	viewport      viewport.Model
	help          help.Model
	form          *huh.Form
	formKind      formKind
	formData      *formData
	formTheme     *formThemeState
	formID        uint64
	quitReturn    Screen
	reportReturn  Screen
	keepsakes     []LetterSummary
	current       *Letter
	cursor        int
	foldFrame     int
	animationID   uint64
	animating     bool
	pendingG      bool
	showHelp      bool
	status        string
	statusKind    statusKind
	deliveryReply bool
	searchID      uint64
	incomingState fixtureState
	keepsakeIndex int
	reportIndex   int
	reportTarget  string
	styles        Styles
}

// New returns a deterministic offline prototype.
func New() Model {
	draft := newTextarea("Write a letter...")
	reply := newTextarea("Write one reply...")
	view := viewport.New(viewport.WithWidth(56), viewport.WithHeight(9))

	m := Model{
		connection:    connectionLocal,
		screen:        ScreenSplash,
		mode:          ModeNavigation,
		width:         80,
		height:        24,
		profile:       colorprofile.Unknown,
		theme:         "auto",
		incomingState: fixtureAvailable,
		keepsakeIndex: -1,
		reportIndex:   -1,
		draft:         draft,
		replyDraft:    reply,
		viewport:      view,
		help:          help.New(),
		keepsakes: []LetterSummary{
			{
				Direction: "received",
				Alias:     "mori",
				Letter: Letter{
					SenderAlias: "mori",
					Body:        "I found rain caught in the cedar branches today. For a moment the whole path sounded like a small river.",
					Reply:       "Thank you for leaving that moment here.",
					Age:         "three days ago",
					FoldSeed:    0x6f726966756465,
				},
			},
		},
	}
	m.refreshPresentation()
	m.resize(m.width, m.height)
	return m
}

// NewOnline returns the production TUI model with no participant fixtures.
func NewOnline(runtime *Runtime) Model {
	m := New()
	m.runtime = runtime
	m.connection = connectionConnecting
	m.identity = Identity{}
	m.keepsakes = nil
	m.current = nil
	m.incomingState = fixtureConsumed
	m.status = "Connecting to the post office..."
	return m
}

func (m *Model) refreshPresentation() {
	m.styles = newStyles(m.dark, m.profile, m.theme, m.ascii)
	m.help.Styles = help.Styles{
		ShortKey:       m.styles.Label,
		ShortDesc:      m.styles.Help,
		ShortSeparator: m.styles.Muted,
		Ellipsis:       m.styles.Muted,
		FullKey:        m.styles.Label,
		FullDesc:       m.styles.Help,
		FullSeparator:  m.styles.Muted,
	}
	m.draft.SetStyles(m.textareaStyles())
	m.replyDraft.SetStyles(m.textareaStyles())
	if m.formTheme != nil {
		m.formTheme.dark = m.dark
		m.formTheme.profile = m.profile
		m.formTheme.theme = m.theme
		m.formTheme.ascii = m.ascii
	}
	if m.ascii {
		m.help.ShortSeparator = " | "
		m.help.Ellipsis = "..."
	} else {
		m.help.ShortSeparator = " • "
		m.help.Ellipsis = "…"
	}
}

func (m Model) textareaStyles() textarea.Styles {
	focused := textarea.StyleState{
		Base:        m.styles.PaperText,
		Text:        m.styles.PaperText,
		CursorLine:  m.styles.PaperText,
		EndOfBuffer: m.styles.PaperText,
		Placeholder: m.styles.PaperMute,
		Prompt:      m.styles.PaperText,
		Selection:   m.styles.Selection,
	}
	blurred := focused
	blurred.Text = m.styles.PaperMute
	blurred.CursorLine = m.styles.PaperMute
	return textarea.Styles{
		Focused: focused,
		Blurred: blurred,
		Cursor: textarea.CursorStyle{
			Color: m.styles.Label.GetForeground(),
			Shape: tea.CursorBar,
			Blink: true,
		},
	}
}

func (m *Model) setStatus(kind statusKind, text string) {
	hadStatus := m.status != ""
	m.statusKind = kind
	m.status = text
	if m.current != nil && hadStatus != (text != "") {
		m.refreshLetterViewport()
	}
}

func (m Model) formAcceptsText() bool {
	if m.form == nil {
		return false
	}
	_, ok := m.form.GetFocusedField().(*huh.Input)
	return ok
}

func newTextarea(placeholder string) textarea.Model {
	t := textarea.New()
	t.Placeholder = placeholder
	t.Prompt = ""
	t.ShowLineNumbers = false
	t.CharLimit = textpolicy.MaxBodyBytes
	t.KeyMap.Paste.SetEnabled(false)
	t.MaxHeight = 8
	t.MaxContentHeight = textpolicy.MaxBodyCodePoints
	t.SetWidth(56)
	t.SetHeight(8)
	return t
}

// Init requests terminal capabilities without starting background work.
func (m Model) Init() tea.Cmd {
	commands := []tea.Cmd{tea.RequestBackgroundColor, tea.RequestWindowSize}
	if m.runtime != nil {
		commands = append(commands, m.bootstrapCommand())
	}
	return tea.Batch(commands...)
}

func (m *Model) resize(width, height int) {
	m.width = width
	m.height = height
	contentWidth := m.panelContentWidth()
	contentHeight := min(max(height-13, 3), 10)
	paperWidth := max(contentWidth-m.styles.Paper.GetHorizontalFrameSize(), 16)
	m.draft.SetWidth(paperWidth)
	m.draft.SetHeight(max(contentHeight-2, 3))
	m.replyDraft.SetWidth(paperWidth)
	m.replyDraft.SetHeight(max(contentHeight-2, 3))
	m.viewport.SetWidth(paperWidth)
	m.viewport.SetHeight(max(contentHeight-2, 3))
	m.refreshLetterViewport()
	m.help.SetWidth(contentWidth)
	if m.form != nil {
		m.form.WithWidth(contentWidth)
		if formHeight := formHeight(m.formKind, height); formHeight > 0 {
			m.form.WithHeight(formHeight)
		}
	}
}

func formHeight(kind formKind, terminalHeight int) int {
	height := max(terminalHeight-8, 4)
	switch kind {
	case formReport:
		return min(height, 11)
	case formSettings:
		return min(height, 12)
	default:
		return 0
	}
}

func (m *Model) setCurrent(letter Letter) {
	m.current = &letter
	m.keepsakeIndex = -1
	m.refreshLetterViewport()
	m.viewport.GotoTop()
}

func (m *Model) setKeepsake(index int) {
	summary := m.keepsakes[index]
	m.current = &summary.Letter
	m.keepsakeIndex = index
	m.refreshLetterViewport()
	m.viewport.GotoTop()
}

func (m *Model) refreshLetterViewport() {
	if m.current == nil {
		return
	}
	content := neutralizeTerminalText(m.current.Body)
	if m.keepsakeIndex >= 0 {
		prefix := "Received - "
		if m.keepsakes[m.keepsakeIndex].Direction == "sent" {
			prefix = "Sent - "
		}
		content = prefix + content
		if m.current.Reply != "" {
			content += "\n\nReply - " + neutralizeTerminalText(m.current.Reply)
		}
	}
	content = ansi.Wrap(content, m.viewport.Width(), "")
	lineCount := strings.Count(content, "\n") + 1
	contentHeight := min(max(m.height-13, 3), 10)
	viewportHeight := min(max(contentHeight-2, 3), max(lineCount, 2))
	if m.layout() == layoutCompact && m.status != "" && viewportHeight > 3 {
		viewportHeight--
	}
	m.viewport.SetHeight(viewportHeight)
	m.viewport.SetContent(content)
}

func (m *Model) startAnimation(screen Screen, reverse bool) tea.Cmd {
	m.screen = screen
	m.mode = ModeNavigation
	m.animationID++
	m.animating = true
	if reverse {
		m.foldFrame = len(foldFrames(m.currentSeed(), m.width, m.ascii)) - 1
	} else {
		m.foldFrame = 0
	}
	if m.reducedMotion {
		m.animating = false
		if screen == ScreenUnfold {
			m.screen = ScreenRead
			m.cursor = 0
			if m.runtime == nil {
				m.incomingState = fixtureOpened
			}
		} else {
			m.foldFrame = len(foldFrames(m.currentSeed(), m.width, m.ascii)) - 1
		}
		return nil
	}
	return nextFoldTick(m.animationID)
}

func (m Model) currentSeed() uint64 {
	if m.current != nil {
		return m.current.FoldSeed
	}
	return 0x6f726966756465
}

type foldTickMsg struct {
	id uint64
}
