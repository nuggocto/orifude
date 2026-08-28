package tui

import (
	"context"
	"crypto/rand"
	"errors"
	"fmt"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/colorprofile"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/identity"
	"golang.org/x/mod/semver"
)

const (
	onlineOperationTimeout = 25 * time.Second
	bootstrapTimeout       = 5 * time.Second
)

type detailAction uint8

const (
	detailReport detailAction = iota
	detailBlock
	detailWithdraw
	detailRemove
)

type bootstrapMsg struct {
	profile     identity.Profile
	device      *api.DeviceClient
	settings    identity.Settings
	settingsErr error
	me          api.GetMeResponse
	found       bool
	err         error
}

type prepareIdentityMsg struct {
	id            uint64
	registration  *pendingRegistration
	profile       identity.Profile
	needsFallback bool
	err           error
}

type registerMsg struct {
	id        uint64
	profile   identity.Profile
	device    *api.DeviceClient
	me        api.GetMeResponse
	confirmed bool
	uncertain bool
	err       error
}

type reconnectMsg struct {
	id      uint64
	profile identity.Profile
	device  *api.DeviceClient
	me      api.GetMeResponse
	err     error
}

type sendMsg struct {
	id       uint64
	response api.CreateLetterResponse
	err      error
}

type claimMsg struct {
	id       uint64
	response api.ClaimLetterResponse
	err      error
}

type openMsg struct {
	id       uint64
	response api.OpenLetterResponse
	err      error
}

type replyMsg struct {
	id       uint64
	response api.ReplyToLetterResponse
	err      error
}

type keepsakesMsg struct {
	id       uint64
	response api.ListKeepsakesResponse
	append   bool
	err      error
}

type letterMsg struct {
	id       uint64
	index    int
	response api.GetLetterResponse
	err      error
}

type reportMsg struct {
	id       uint64
	response api.ReportLetterResponse
	err      error
}

type blockMsg struct {
	id       uint64
	response api.BlockLetterResponse
	err      error
}

type withdrawMsg struct {
	id       uint64
	response api.WithdrawLetterResponse
	err      error
}

type deleteKeepsakeMsg struct {
	id  uint64
	err error
}

type deleteIdentityMsg struct {
	id       uint64
	err      error
	localErr error
}

type revokeIdentityMsg struct {
	id       uint64
	err      error
	localErr error
}

type settingsSavedMsg struct {
	id  uint64
	err error
}

func commandContext() (context.Context, context.CancelFunc) {
	return context.WithTimeout(context.Background(), onlineOperationTimeout)
}

func startupContext() (context.Context, context.CancelFunc) {
	return context.WithTimeout(context.Background(), bootstrapTimeout)
}

func (m Model) bootstrapCommand() tea.Cmd {
	runtime := m.runtime
	return func() tea.Msg {
		if runtime == nil || runtime.Client == nil || runtime.Store == nil {
			return bootstrapMsg{err: errors.New("online runtime is incomplete")}
		}
		settings, settingsErr := runtime.Store.LoadSettings()
		if settingsErr != nil {
			settings = identity.Settings{Theme: "auto"}
		}
		profile, key, err := runtime.Store.Load()
		if errors.Is(err, identity.ErrNotFound) {
			return bootstrapMsg{settings: settings, settingsErr: settingsErr}
		}
		if err != nil {
			return bootstrapMsg{settings: settings, settingsErr: settingsErr, found: true, err: err}
		}
		device, err := runtime.Client.ForDevice(key)
		if err != nil {
			return bootstrapMsg{profile: profile, settings: settings, settingsErr: settingsErr, found: true, err: err}
		}
		ctx, cancel := startupContext()
		_, err = device.CreateSession(ctx)
		cancel()
		if err != nil {
			return bootstrapMsg{profile: profile, device: device, settings: settings, settingsErr: settingsErr, found: true, err: err}
		}
		ctx, cancel = startupContext()
		me, err := device.Me(ctx)
		cancel()
		if err == nil && !profile.Active {
			profile, err = runtime.Store.Activate(me.Alias)
		}
		return bootstrapMsg{profile: profile, device: device, settings: settings, settingsErr: settingsErr, me: me, found: true, err: err}
	}
}

func (m Model) prepareIdentityCommand(id uint64, alias, invite string, allowFallback bool) tea.Cmd {
	runtime := m.runtime
	registration := m.registration
	return func() tea.Msg {
		if runtime == nil || runtime.Client == nil || runtime.Store == nil {
			return prepareIdentityMsg{id: id, err: errors.New("online runtime is incomplete")}
		}
		if registration == nil {
			key, err := auth.GenerateDeviceKey(rand.Reader)
			if err != nil {
				return prepareIdentityMsg{id: id, err: err}
			}
			credential, err := auth.GenerateRevocationCredential(rand.Reader)
			if err != nil {
				return prepareIdentityMsg{id: id, err: err}
			}
			device, err := runtime.Client.ForDevice(key)
			if err != nil {
				return prepareIdentityMsg{id: id, err: err}
			}
			registration = &pendingRegistration{alias: alias, invite: invite, credential: credential, key: key, device: device}
		}
		profile, err := runtime.Store.SavePending(registration.alias, registration.key, allowFallback)
		if errors.Is(err, identity.ErrFallbackApproval) {
			return prepareIdentityMsg{id: id, registration: registration, needsFallback: true}
		}
		return prepareIdentityMsg{id: id, registration: registration, profile: profile, err: err}
	}
}

func (m Model) registerCommand(id uint64) tea.Cmd {
	runtime := m.runtime
	registration := m.registration
	return func() tea.Msg {
		if runtime == nil || registration == nil || registration.device == nil {
			return registerMsg{id: id, err: errors.New("registration state is incomplete")}
		}
		request := api.CreateIdentityRequest{
			Alias: registration.alias, InviteCode: registration.invite,
			RevocationHash: auth.EncodeHash(auth.HashRevocationCredential(registration.credential)),
		}
		register := func() error {
			ctx, cancel := commandContext()
			defer cancel()
			challenge, err := registration.device.CreateChallenge(ctx, api.ChallengePurposeRegistration)
			if err != nil {
				return err
			}
			request.ChallengeID = challenge.ChallengeID
			_, err = registration.device.Register(ctx, challenge, request)
			return err
		}
		confirmed := registration.confirmed
		uncertain := registration.uncertain
		var err error
		if !confirmed && uncertain {
			ctx, cancel := commandContext()
			_, err = registration.device.CreateSession(ctx)
			cancel()
			if err == nil {
				confirmed = true
			} else if !registrationMissing(err) {
				return registerMsg{id: id, device: registration.device, confirmed: false, uncertain: true, err: err}
			}
		}
		if !confirmed {
			err = register()
			if err == nil {
				confirmed = true
			} else if ambiguousRegistration(err) {
				uncertain = true
				ctx, cancel := commandContext()
				_, sessionErr := registration.device.CreateSession(ctx)
				cancel()
				if sessionErr == nil {
					confirmed = true
					err = nil
				} else if registrationMissing(sessionErr) {
					err = register()
					if err == nil {
						confirmed = true
					}
				} else {
					err = sessionErr
				}
			}
		}
		if err != nil {
			return registerMsg{id: id, device: registration.device, confirmed: confirmed, uncertain: uncertain, err: err}
		}
		ctx, cancel := commandContext()
		me, err := registration.device.Me(ctx)
		cancel()
		if err != nil {
			return registerMsg{id: id, device: registration.device, confirmed: confirmed, uncertain: uncertain, err: err}
		}
		profile, err := runtime.Store.Activate(me.Alias)
		return registerMsg{id: id, profile: profile, device: registration.device, me: me, confirmed: confirmed, uncertain: uncertain, err: err}
	}
}

func registrationMissing(err error) bool {
	var httpError *api.HTTPError
	return errors.As(err, &httpError) && httpError.API.Code == api.ErrorCodeAuthenticationFailed
}

func ambiguousRegistration(err error) bool {
	if errors.Is(err, api.ErrTransport) || errors.Is(err, api.ErrProtocol) {
		return true
	}
	var httpError *api.HTTPError
	return errors.As(err, &httpError) && httpError.API.Code == api.ErrorCodeServiceUnavailable
}

func (m Model) reconnectCommand(id uint64) tea.Cmd {
	runtime := m.runtime
	device := m.device
	profile := m.localIdentity
	return func() tea.Msg {
		if runtime == nil || runtime.Store == nil || runtime.Client == nil {
			return reconnectMsg{id: id, err: errors.New("online runtime is incomplete")}
		}
		if device == nil {
			loadedProfile, key, err := runtime.Store.Load()
			if err != nil {
				return reconnectMsg{id: id, err: err}
			}
			profile = loadedProfile
			device, err = runtime.Client.ForDevice(key)
			if err != nil {
				return reconnectMsg{id: id, profile: profile, err: err}
			}
		}
		ctx, cancel := commandContext()
		_, err := device.CreateSession(ctx)
		cancel()
		if err != nil {
			return reconnectMsg{id: id, profile: profile, device: device, err: err}
		}
		ctx, cancel = commandContext()
		me, err := device.Me(ctx)
		cancel()
		if err != nil {
			return reconnectMsg{id: id, profile: profile, device: device, err: err}
		}
		profile, err = runtime.Store.Activate(me.Alias)
		return reconnectMsg{id: id, profile: profile, device: device, me: me, err: err}
	}
}

func (m *Model) beginOperation(kind operationKind, mutation bool) *pendingOperation {
	m.requestID++
	pending := &pendingOperation{id: m.requestID, kind: kind, screen: m.screen, busy: true, mutation: mutation}
	m.pending = pending
	return pending
}

func (m Model) operationCurrent(id uint64, kind operationKind) bool {
	return m.pending != nil && m.pending.id == id && m.pending.kind == kind && m.pending.busy && m.pending.screen == m.screen
}

func (m *Model) operationFailed(err error) {
	if m.pending != nil {
		m.pending.busy = false
		m.pending.uncertain = m.pending.uncertain || m.pending.mutation && mutationOutcomeUnknown(err)
	}
	if errors.Is(err, api.ErrTransport) {
		m.connection = connectionOffline
	} else if !errors.Is(err, api.ErrProtocol) {
		m.connection = connectionOnline
	}
	var httpError *api.HTTPError
	if errors.As(err, &httpError) && (httpError.API.Code == api.ErrorCodeAuthenticationFailed || httpError.API.Code == api.ErrorCodeSessionExpired) {
		m.connection = connectionInvalidIdentity
		m.screen = ScreenRecovery
	}
	m.setStatus(statusError, visibleAPIError(err))
}

func mutationOutcomeUnknown(err error) bool {
	return errors.Is(err, api.ErrTransport) || errors.Is(err, api.ErrProtocol) || errors.Is(err, api.ErrResponseTooLarge)
}

func visibleAPIError(err error) string {
	if err == nil {
		return ""
	}
	if errors.Is(err, context.DeadlineExceeded) || errors.Is(err, api.ErrTransport) {
		return "The post office could not be reached. Your current text is still here."
	}
	if errors.Is(err, api.ErrResponseTooLarge) || errors.Is(err, api.ErrProtocol) {
		return "The post office returned an unsafe or invalid response."
	}
	var httpError *api.HTTPError
	if !errors.As(err, &httpError) {
		return "The operation could not be completed."
	}
	switch httpError.API.Code {
	case api.ErrorCodeClockSkew:
		return "Correct this device's clock, then try again."
	case api.ErrorCodeAuthenticationFailed, api.ErrorCodeSessionExpired:
		return "This local identity can no longer authenticate. It cannot be recovered."
	case api.ErrorCodeClaimExpired:
		return "This unopened claim expired and returned to the branch."
	case api.ErrorCodeRateLimited:
		return "The request limit has been reached. Try again later."
	case api.ErrorCodeInviteInvalid:
		return "The invite is invalid, expired, or already used."
	case api.ErrorCodeAliasInvalid:
		return "That alias cannot be used."
	case api.ErrorCodeIdentityConflict:
		return "The identity conflicts with existing state."
	case api.ErrorCodeLetterAlreadyReplied:
		return "This letter already has its one reply."
	case api.ErrorCodeReportAlreadyExists:
		return "This exchange has already been reported."
	case api.ErrorCodeConflict:
		return "The exchange changed before this operation completed."
	case api.ErrorCodeNotFound:
		return "The requested exchange is no longer available."
	case api.ErrorCodeServiceUnavailable:
		return "The post office is temporarily unavailable. Your current text is still here."
	default:
		return "The post office rejected the request."
	}
}

func (m Model) handleOnlineMessage(message tea.Msg) (Model, tea.Cmd, bool) {
	switch message := message.(type) {
	case bootstrapMsg:
		if m.screen != ScreenSplash || m.form != nil {
			return m, nil, true
		}
		m.applySettings(message.settings)
		if !message.found && message.err == nil {
			m.connection = connectionOffline
			if message.settingsErr != nil {
				m.setStatus(statusError, "Local display settings could not be loaded; defaults are active.")
			} else {
				m.setStatus(statusInfo, "Create an identity to connect to the post office.")
			}
			return m, nil, true
		}
		m.localIdentity, m.device = message.profile, message.device
		m.identity = Identity{Alias: message.profile.Alias, Thumbprint: message.profile.Thumbprint}
		if message.err != nil {
			if message.device != nil && temporaryStartupFailure(message.err) {
				m.connection = connectionOffline
				if message.profile.Active {
					m.screen = ScreenBranch
					m.setStatus(statusError, visibleAPIError(message.err))
				} else {
					m.screen = ScreenRecovery
					m.setStatus(statusError, "The post office could not be reached. Identity creation has not been confirmed.")
				}
			} else {
				m.connection = connectionInvalidIdentity
				m.screen = ScreenRecovery
				m.setStatus(statusError, "The stored device identity cannot be used.")
			}
			return m, nil, true
		}
		m.connected(message.me)
		if message.settingsErr != nil {
			m.setStatus(statusError, "Local display settings could not be loaded; defaults are active.")
		}
		return m, nil, true
	case prepareIdentityMsg:
		if !m.operationCurrent(message.id, operationPrepareIdentity) {
			return m, nil, true
		}
		m.pending = nil
		m.registration = message.registration
		if message.err != nil {
			if errors.Is(message.err, identity.ErrAlreadyExists) {
				m.registration = nil
				m.screen = ScreenSplash
				m.setStatus(statusInfo, "Another process created local identity state. Loading that identity...")
				return m, m.bootstrapCommand(), true
			}
			m.setStatus(statusError, visibleAPIError(message.err))
			return m, nil, true
		}
		if message.needsFallback {
			m.setStatus(statusInfo, "The operating-system credential store is unavailable.")
			return m, m.beginForm(formFallback), true
		}
		m.localIdentity = message.profile
		m.device = message.registration.device
		m.screen = ScreenRevocation
		return m, m.beginForm(formRevocation), true
	case registerMsg:
		if !m.operationCurrent(message.id, operationRegister) {
			return m, nil, true
		}
		if message.err != nil {
			if m.registration != nil {
				m.registration.confirmed = m.registration.confirmed || message.confirmed
				m.registration.uncertain = m.registration.uncertain || message.uncertain || message.confirmed || ambiguousRegistration(message.err)
			}
			m.operationFailed(message.err)
			return m, nil, true
		}
		m.localIdentity, m.device = message.profile, message.device
		m.pending = nil
		if m.registration != nil {
			m.registration.credential = ""
			m.registration.invite = ""
		}
		m.registration = nil
		if !m.connected(message.me) {
			m.setStatus(statusSuccess, "Welcome, "+message.me.Alias+".")
		}
		return m, nil, true
	case reconnectMsg:
		if !m.operationCurrent(message.id, operationReconnect) {
			return m, nil, true
		}
		if message.err != nil {
			m.operationFailed(message.err)
			m.pending = nil
			return m, nil, true
		}
		m.localIdentity, m.device = message.profile, message.device
		m.pending = nil
		if !m.connected(message.me) {
			m.setStatus(statusSuccess, "Connected to the post office.")
		}
		return m, nil, true
	case sendMsg:
		if !m.operationCurrent(message.id, operationSend) {
			return m, nil, true
		}
		if message.err != nil {
			m.operationFailed(message.err)
			return m, nil, true
		}
		m.connection = connectionOnline
		body := m.pending.body
		m.pending = nil
		m.setCurrent(Letter{ID: message.response.LetterID, Role: api.LetterRoleSender, State: message.response.State,
			SenderAlias: m.identity.Alias, Body: body, FoldSeed: uint64(message.response.FoldSeed), CreatedAt: message.response.CreatedAt})
		m.draft.Reset()
		m.draftID = ""
		m.deliveryReply = false
		m.screen = ScreenDelivery
		m.setStatus(statusSuccess, "The letter was released into the quiet.")
		return m, nil, true
	case claimMsg:
		if !m.operationCurrent(message.id, operationClaim) {
			return m, nil, true
		}
		if message.err != nil {
			m.pending = nil
			var httpError *api.HTTPError
			if errors.As(message.err, &httpError) && httpError.API.Code == api.ErrorCodeNotFound {
				m.connection = connectionOnline
				m.screen = ScreenBranch
				m.setStatus(statusInfo, "No letter is waiting right now.")
				return m, nil, true
			}
			m.operationFailed(message.err)
			if m.screen == ScreenSearching {
				m.screen = ScreenBranch
			}
			return m, nil, true
		}
		m.connection = connectionOnline
		m.pending = nil
		m.setCurrent(Letter{ID: message.response.LetterID, Role: api.LetterRoleRecipient, State: api.LetterStateClaimed,
			FoldSeed: uint64(message.response.FoldSeed), CreatedAt: message.response.CreatedAt, ClaimExpiry: message.response.ClaimExpiresAt,
			Age: humanAge(message.response.CreatedAt)})
		m.screen = ScreenFoldedDelivery
		m.setStatus(statusInfo, "A folded letter is waiting.")
		return m, nil, true
	case openMsg:
		if !m.operationCurrent(message.id, operationOpen) {
			return m, nil, true
		}
		if message.err != nil {
			m.operationFailed(message.err)
			var httpError *api.HTTPError
			if errors.As(message.err, &httpError) && httpError.API.Code == api.ErrorCodeClaimExpired {
				m.current = nil
				m.screen = ScreenBranch
			}
			return m, nil, true
		}
		m.connection = connectionOnline
		m.pending = nil
		m.current.State = api.LetterStateOpened
		m.current.SenderAlias = message.response.Original.Alias
		m.current.Body = message.response.Original.Body
		m.refreshLetterViewport()
		return m, m.startAnimation(ScreenUnfold, true), true
	case replyMsg:
		if !m.operationCurrent(message.id, operationReply) {
			return m, nil, true
		}
		if message.err != nil {
			m.operationFailed(message.err)
			return m, nil, true
		}
		m.connection = connectionOnline
		body := m.pending.body
		m.pending = nil
		if m.current != nil {
			m.current.Reply = body
			m.current.State = api.LetterStateReplied
		}
		m.clearReplyDraft()
		m.deliveryReply = true
		m.screen = ScreenDelivery
		m.setStatus(statusSuccess, "Your reply was folded into the keepsake.")
		return m, nil, true
	case keepsakesMsg:
		if !m.operationCurrent(message.id, operationKeepsakes) {
			return m, nil, true
		}
		if message.err != nil {
			m.operationFailed(message.err)
			return m, nil, true
		}
		m.connection = connectionOnline
		m.pending = nil
		items := make([]LetterSummary, 0, len(message.response.Keepsakes))
		for _, summary := range message.response.Keepsakes {
			items = append(items, summaryFromAPI(summary))
		}
		if message.append {
			m.keepsakes = append(m.keepsakes, items...)
		} else {
			m.keepsakes = items
		}
		m.nextCursor = message.response.NextCursor
		m.screen = ScreenKeepsakes
		m.cursor = 0
		return m, nil, true
	case letterMsg:
		if !m.operationCurrent(message.id, operationLetter) {
			return m, nil, true
		}
		if message.err != nil {
			m.operationFailed(message.err)
			return m, nil, true
		}
		m.connection = connectionOnline
		m.pending = nil
		letter := letterFromAPI(message.response)
		m.setCurrent(letter)
		m.keepsakeIndex = message.index
		m.refreshLetterViewport()
		m.screen = ScreenKeepsakeDetail
		m.cursor = 0
		return m, nil, true
	case reportMsg:
		return m.handleReport(message), nil, true
	case blockMsg:
		return m.handleBlock(message), nil, true
	case withdrawMsg:
		return m.handleWithdraw(message), nil, true
	case deleteKeepsakeMsg:
		return m.handleDeleteKeepsake(message), nil, true
	case deleteIdentityMsg:
		return m.handleDeleteIdentity(message), nil, true
	case revokeIdentityMsg:
		return m.handleRevokeIdentity(message), nil, true
	case settingsSavedMsg:
		if !m.operationCurrent(message.id, operationSettings) {
			return m, nil, true
		}
		m.pending = nil
		if message.err != nil {
			m.setStatus(statusError, "Settings are active, but could not be saved locally.")
		} else {
			m.setStatus(statusSuccess, "Settings saved.")
		}
		return m, nil, true
	default:
		return m, nil, false
	}
}

func temporaryStartupFailure(err error) bool {
	if errors.Is(err, api.ErrTransport) {
		return true
	}
	var httpError *api.HTTPError
	return errors.As(err, &httpError) && (httpError.API.Code == api.ErrorCodeServiceUnavailable || httpError.API.Code == api.ErrorCodeClockSkew)
}

func (m *Model) connected(me api.GetMeResponse) bool {
	m.connection = connectionOnline
	m.identity = Identity{Alias: me.Alias, Thumbprint: m.localIdentity.Thumbprint}
	m.latestVersion = me.LatestTUIVersion
	m.screen = ScreenBranch
	m.cursor = 0
	if m.runtime != nil && semver.IsValid(m.runtime.Version) && semver.IsValid(me.LatestTUIVersion) && semver.Compare(me.LatestTUIVersion, m.runtime.Version) > 0 {
		m.setStatus(statusInfo, "A newer Orifude release is available: "+me.LatestTUIVersion+".")
		return true
	} else {
		m.setStatus(statusInfo, "")
	}
	return false
}

func (m *Model) applySettings(settings identity.Settings) {
	if settings.Theme != "" {
		m.theme = settings.Theme
	}
	m.reducedMotion = settings.ReducedMotion
	m.asciiFallback = settings.ASCIIFallback
	m.accessible = settings.Accessible
	m.ascii = m.asciiFallback || m.theme == "mono" || m.profile == colorprofile.Ascii || m.profile == colorprofile.NoTTY
	m.refreshPresentation()
}

func humanAge(created time.Time) string {
	age := time.Since(created)
	if age < time.Minute {
		return "just now"
	}
	if age < time.Hour {
		return fmt.Sprintf("%d minutes ago", int(age/time.Minute))
	}
	if age < 24*time.Hour {
		return fmt.Sprintf("%d hours ago", int(age/time.Hour))
	}
	return fmt.Sprintf("%d days ago", int(age/(24*time.Hour)))
}

func summaryFromAPI(summary api.LetterSummary) LetterSummary {
	direction := "received"
	if summary.Role == api.LetterRoleSender {
		direction = "sent"
	}
	return LetterSummary{ID: summary.LetterID, Role: summary.Role, State: summary.State, Direction: direction, Alias: summary.OtherAlias,
		Letter: Letter{ID: summary.LetterID, Role: summary.Role, State: summary.State, SenderAlias: summary.OtherAlias,
			FoldSeed: uint64(summary.FoldSeed), CreatedAt: summary.CreatedAt, Age: humanAge(summary.CreatedAt)}}
}

func letterFromAPI(response api.GetLetterResponse) Letter {
	letter := Letter{ID: response.LetterID, Role: response.Role, State: response.State, SenderAlias: response.OtherAlias,
		FoldSeed: uint64(response.FoldSeed), CreatedAt: response.CreatedAt, Age: humanAge(response.CreatedAt)}
	if response.Original != nil {
		letter.Body = response.Original.Body
		if response.Role == api.LetterRoleRecipient {
			letter.SenderAlias = response.Original.Alias
		}
	}
	if response.Reply != nil {
		letter.Reply = response.Reply.Body
	}
	if response.ClaimExpiresAt != nil {
		letter.ClaimExpiry = *response.ClaimExpiresAt
	}
	return letter
}

func (m Model) detailActions() []detailAction {
	if m.current == nil {
		return nil
	}
	actions := make([]detailAction, 0, 3)
	if m.current.Role == api.LetterRoleSender && m.current.State == api.LetterStateWaiting {
		actions = append(actions, detailWithdraw)
	}
	if m.keepsakeReportable() {
		actions = append(actions, detailReport)
	}
	if m.current.SenderAlias != "" && (m.current.State == api.LetterStateOpened || m.current.State == api.LetterStateReplied) {
		actions = append(actions, detailBlock)
	}
	return append(actions, detailRemove)
}

func detailActionLabel(action detailAction) string {
	switch action {
	case detailReport:
		return "Report and burn"
	case detailBlock:
		return "Block future matching"
	case detailWithdraw:
		return "Withdraw unclaimed letter"
	case detailRemove:
		return "Remove keepsake"
	default:
		return "Unavailable"
	}
}

func (m *Model) startSend() tea.Cmd {
	if m.device == nil {
		m.setStatus(statusError, "Reconnect before releasing this letter.")
		return nil
	}
	if m.pending != nil && m.pending.kind == operationSend && !m.pending.busy {
		m.requestID++
		m.pending.id, m.pending.busy = m.requestID, true
	} else {
		pending := m.beginOperation(operationSend, true)
		if m.draftID == "" {
			m.pending = nil
			m.setStatus(statusError, "The release identifier is missing. Preview the letter again.")
			return nil
		}
		pending.clientID, pending.body = m.draftID, m.draft.Value()
	}
	pending := *m.pending
	device := m.device
	return func() tea.Msg {
		ctx, cancel := commandContext()
		defer cancel()
		response, err := device.CreateLetter(ctx, api.CreateLetterRequest{LetterID: pending.clientID, Body: pending.body})
		return sendMsg{id: pending.id, response: response, err: err}
	}
}

func (m *Model) prepareLetterPreview() error {
	if m.runtime == nil || m.draftID != "" {
		return nil
	}
	id, err := auth.GenerateClientID(rand.Reader)
	if err != nil {
		return err
	}
	m.draftID = id
	return nil
}

func (m *Model) startClaim() tea.Cmd {
	if m.device == nil {
		m.setStatus(statusError, "Reconnect before waiting by the branch.")
		return nil
	}
	pending := m.beginOperation(operationClaim, true)
	device := m.device
	return func() tea.Msg {
		ctx, cancel := commandContext()
		defer cancel()
		response, err := device.ClaimLetter(ctx)
		return claimMsg{id: pending.id, response: response, err: err}
	}
}

func (m *Model) startOpen() tea.Cmd {
	if m.device == nil || m.current == nil {
		return nil
	}
	pending := m.beginOperation(operationOpen, true)
	device, letterID := m.device, m.current.ID
	return func() tea.Msg {
		ctx, cancel := commandContext()
		defer cancel()
		response, err := device.OpenLetter(ctx, letterID)
		return openMsg{id: pending.id, response: response, err: err}
	}
}

func (m *Model) startReply() tea.Cmd {
	if m.device == nil || m.current == nil {
		return nil
	}
	if m.pending != nil && m.pending.kind == operationReply && !m.pending.busy {
		m.requestID++
		m.pending.id, m.pending.busy = m.requestID, true
	} else {
		pending := m.beginOperation(operationReply, true)
		id, err := auth.GenerateClientID(rand.Reader)
		if err != nil {
			m.pending = nil
			m.setStatus(statusError, "A reply identifier could not be created.")
			return nil
		}
		pending.clientID, pending.body = id, m.replyDraft.Value()
	}
	pending := *m.pending
	device, letterID := m.device, m.current.ID
	return func() tea.Msg {
		ctx, cancel := commandContext()
		defer cancel()
		response, err := device.Reply(ctx, letterID, api.ReplyToLetterRequest{ReplyID: pending.clientID, Body: pending.body})
		return replyMsg{id: pending.id, response: response, err: err}
	}
}

func (m *Model) startKeepsakes(appendPage bool) tea.Cmd {
	if m.device == nil {
		m.setStatus(statusError, "Reconnect before opening keepsakes.")
		return nil
	}
	cursor := ""
	if appendPage {
		cursor = m.nextCursor
	}
	pending := m.beginOperation(operationKeepsakes, false)
	device := m.device
	return func() tea.Msg {
		ctx, cancel := commandContext()
		defer cancel()
		response, err := device.Keepsakes(ctx, cursor, 20)
		return keepsakesMsg{id: pending.id, response: response, append: appendPage, err: err}
	}
}

func (m *Model) startLetter(summary LetterSummary) tea.Cmd {
	pending := m.beginOperation(operationLetter, false)
	device, letterID, index := m.device, summary.ID, m.cursor
	return func() tea.Msg {
		ctx, cancel := commandContext()
		defer cancel()
		response, err := device.Letter(ctx, letterID)
		return letterMsg{id: pending.id, index: index, response: response, err: err}
	}
}

func (m *Model) startReport(reason api.ReportReason) tea.Cmd {
	if m.device == nil || m.current == nil {
		return nil
	}
	if m.pending != nil && m.pending.kind == operationReport && !m.pending.busy {
		m.requestID++
		m.pending.id, m.pending.busy = m.requestID, true
	} else {
		pending := m.beginOperation(operationReport, true)
		id, err := auth.GenerateClientID(rand.Reader)
		if err != nil {
			m.pending = nil
			m.setStatus(statusError, "A report identifier could not be created.")
			return nil
		}
		pending.clientID, pending.reason = id, reason
		if m.reportTarget == "reply" {
			pending.target = api.ReportTargetReply
		} else {
			pending.target = api.ReportTargetOriginal
		}
	}
	pending := *m.pending
	device, letterID := m.device, m.current.ID
	return func() tea.Msg {
		ctx, cancel := commandContext()
		defer cancel()
		response, err := device.Report(ctx, letterID, api.ReportLetterRequest{
			ReportID: pending.clientID, Target: pending.target, Reason: pending.reason,
		})
		return reportMsg{id: pending.id, response: response, err: err}
	}
}

func (m *Model) startBlock() tea.Cmd {
	if m.device == nil || m.current == nil {
		return nil
	}
	pending := m.beginOperation(operationBlock, true)
	device, letterID := m.device, m.current.ID
	return func() tea.Msg {
		ctx, cancel := commandContext()
		defer cancel()
		response, err := device.Block(ctx, letterID)
		return blockMsg{id: pending.id, response: response, err: err}
	}
}

func (m *Model) startWithdraw() tea.Cmd {
	if m.device == nil || m.current == nil {
		return nil
	}
	pending := m.beginOperation(operationWithdraw, true)
	device, letterID := m.device, m.current.ID
	return func() tea.Msg {
		ctx, cancel := commandContext()
		defer cancel()
		response, err := device.Withdraw(ctx, letterID)
		return withdrawMsg{id: pending.id, response: response, err: err}
	}
}

func (m *Model) startDeleteKeepsake() tea.Cmd {
	if m.device == nil || m.current == nil {
		return nil
	}
	pending := m.beginOperation(operationDeleteKeepsake, true)
	device, letterID := m.device, m.current.ID
	return func() tea.Msg {
		ctx, cancel := commandContext()
		defer cancel()
		return deleteKeepsakeMsg{id: pending.id, err: device.DeleteKeepsake(ctx, letterID)}
	}
}

func (m *Model) startDeleteIdentity() tea.Cmd {
	if m.device == nil || m.runtime == nil || m.runtime.Store == nil {
		return nil
	}
	pending := m.beginOperation(operationDeleteIdentity, true)
	device, store := m.device, m.runtime.Store
	return func() tea.Msg {
		ctx, cancel := commandContext()
		defer cancel()
		err := device.DeleteIdentity(ctx)
		var localErr error
		if err == nil {
			localErr = store.Delete()
		}
		return deleteIdentityMsg{id: pending.id, err: err, localErr: localErr}
	}
}

func (m *Model) startRevokeIdentity(credential string) tea.Cmd {
	if m.runtime == nil || m.runtime.Client == nil || m.runtime.Store == nil {
		return nil
	}
	pending := m.beginOperation(operationRevokeIdentity, true)
	client, store := m.runtime.Client, m.runtime.Store
	return func() tea.Msg {
		ctx, cancel := commandContext()
		defer cancel()
		err := client.RevokeIdentity(ctx, credential)
		var localErr error
		if err == nil {
			localErr = store.Delete()
		}
		return revokeIdentityMsg{id: pending.id, err: err, localErr: localErr}
	}
}

func (m Model) saveSettingsCommand(id uint64) tea.Cmd {
	store := m.runtime.Store
	settings := identity.Settings{Theme: m.theme, ReducedMotion: m.reducedMotion, ASCIIFallback: m.asciiFallback, Accessible: m.accessible}
	return func() tea.Msg { return settingsSavedMsg{id: id, err: store.SaveSettings(settings)} }
}

func (m Model) handleReport(message reportMsg) Model {
	if !m.operationCurrent(message.id, operationReport) {
		return m
	}
	if message.err != nil {
		m.operationFailed(message.err)
		return m
	}
	m.connection = connectionOnline
	m.pending = nil
	m.removeCurrentSummary()
	m.current = nil
	m.clearReplyDraft()
	m.screen = ScreenBranch
	m.cursor = 0
	m.setStatus(statusSuccess, "The exchange was reported, burned, and blocked from future matching.")
	return m
}

func (m Model) handleBlock(message blockMsg) Model {
	if !m.operationCurrent(message.id, operationBlock) {
		return m
	}
	if message.err != nil {
		m.operationFailed(message.err)
		return m
	}
	m.connection = connectionOnline
	m.pending = nil
	m.setStatus(statusSuccess, "Future matching with this person is blocked permanently.")
	return m
}

func (m Model) handleWithdraw(message withdrawMsg) Model {
	if !m.operationCurrent(message.id, operationWithdraw) {
		return m
	}
	if message.err != nil {
		m.operationFailed(message.err)
		return m
	}
	m.connection = connectionOnline
	m.pending = nil
	if m.current != nil {
		m.current.State = api.LetterStateWithdrawn
	}
	m.setStatus(statusSuccess, "The unclaimed letter was withdrawn.")
	return m
}

func (m Model) handleDeleteKeepsake(message deleteKeepsakeMsg) Model {
	if !m.operationCurrent(message.id, operationDeleteKeepsake) {
		return m
	}
	if message.err != nil {
		m.operationFailed(message.err)
		return m
	}
	m.connection = connectionOnline
	returnScreen := m.pending.screen
	m.pending = nil
	m.removeCurrentSummary()
	m.current = nil
	m.clearReplyDraft()
	m.screen = ScreenKeepsakes
	if returnScreen == ScreenRead {
		m.screen = ScreenBranch
	}
	m.cursor = 0
	m.setStatus(statusSuccess, "The keepsake was removed from this identity.")
	return m
}

func (m Model) handleDeleteIdentity(message deleteIdentityMsg) Model {
	if !m.operationCurrent(message.id, operationDeleteIdentity) {
		return m
	}
	if message.err != nil {
		m.operationFailed(message.err)
		return m
	}
	if message.localErr != nil {
		m.pending = nil
		m.connection = connectionInvalidIdentity
		m.screen = ScreenRecovery
		m.setStatus(statusError, "The server identity was deleted, but local key removal failed.")
		return m
	}
	m.resetIdentity()
	m.setStatus(statusSuccess, "The identity was permanently deleted.")
	return m
}

func (m Model) handleRevokeIdentity(message revokeIdentityMsg) Model {
	if !m.operationCurrent(message.id, operationRevokeIdentity) {
		return m
	}
	if message.err != nil {
		m.operationFailed(message.err)
		return m
	}
	if message.localErr != nil {
		m.pending = nil
		m.connection = connectionInvalidIdentity
		m.screen = ScreenRecovery
		m.setStatus(statusError, "The delete request succeeded, but local key removal failed.")
		return m
	}
	m.resetIdentity()
	m.setStatus(statusSuccess, "The delete request was accepted.")
	return m
}

func (m *Model) removeCurrentSummary() {
	if m.current == nil {
		return
	}
	for index := range m.keepsakes {
		if m.keepsakes[index].ID == m.current.ID || m.keepsakes[index].Letter.ID == m.current.ID {
			m.keepsakes = append(m.keepsakes[:index], m.keepsakes[index+1:]...)
			return
		}
	}
}

func (m *Model) resetIdentity() {
	if m.device != nil {
		m.device.ClearSession()
	}
	m.device = nil
	m.localIdentity = identity.Profile{}
	m.identity = Identity{}
	m.registration = nil
	m.pending = nil
	m.current = nil
	m.clearReplyDraft()
	m.keepsakes = nil
	m.nextCursor = ""
	m.connection = connectionOffline
	m.screen = ScreenSplash
	m.cursor = 0
}
