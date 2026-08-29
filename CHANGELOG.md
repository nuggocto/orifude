# Changelog

All notable changes to Orifude will be recorded here.

## Unreleased

### Added

- Initial product and technical specification.
- Public project presentation and contributor guidance.
- Apache-2.0 licensing.
- Go module and project-tool pins, sqlc paths, and secret-free baseline CI.
- Go and post-office development prerequisites and configuration documentation.
- Keyboard-only offline participant TUI with synthetic letter journeys,
  Unicode-safe input, responsive layouts, accessible forms, and a VHS check.
- Production post-office HTTP API with PostgreSQL-backed identity, session,
  letter, keepsake, report, moderation, cleanup, replay, and rate-limit state.
- KMS envelope encryption for letters, replies, and moderation evidence, with
  strict authorization and redacted structured logging.
- Disposable PostgreSQL integration coverage for migrations, generated queries,
  concurrency, complete API behavior, and rollback.
- Online participant TUI integration with invite onboarding, device-bound DPoP
  sessions, keyring-first local identity storage, real letter and safety flows,
  reconnect and deletion handling, passive update notices, and durable display
  preferences.
- Strict, bounded participant API client coverage and a real post-office terminal
  journey for two isolated identities through onboarding, release, claim,
  unfold, reply, keepsakes, report, block, restart, and lost-identity deletion.
- Interactive online development with a disposable PostgreSQL database,
  synthetic KMS, test post office, and isolated local identity.

### Changed

- Settled identity, alias, retention, moderation, hosting, update, and 1.0
  release policy.
- Replaced bearer identity tokens and plaintext message columns with device-key
  authentication, short DPoP-bound sessions, KMS envelope encryption, and
  report-only audited moderation access.
- Deferred frontend scaffolding, tooling, assets, and CI from foundation work to
  landing-page delivery.
- Required rate-event retention to cover every enabled cooldown, hourly window,
  and daily window.
- Made the participant binary use the online post office by default while
  retaining an explicit fixture-backed demo for recordings and development.

### Fixed

- Preserved drafts across unrelated reports and quit prompts, aligned contextual
  keyboard help with active controls, refreshed forms after terminal capability
  changes, completed ASCII fallback, and made missing Go packages fail CI.
- Rejected malformed UTF-8 and unpaired JSON surrogates before message handling.
- Bounded and coalesced Cloudflare Access certificate refreshes without holding
  the key-cache lock across network I/O.
- Used one PostgreSQL timestamp when consuming challenges and redeeming invites
  so eligibility and recorded consumption cannot cross an expiry boundary.
- Prevented missing metadata, concurrent onboarding, and post-registration local
  failures from replacing or orphaning a surviving device key.
- Kept ambiguous mutation identifiers until their original operation is
  reconciled, and renewed suspended sessions against wall-clock deadlines.
- Restored persisted ASCII presentation, kept update notices visible, scoped
  reply drafts to one exchange, and hid actions the server would reject.
- Bound fold previews to the same stable seed stored by the post office and
  shown to recipients.
- Replaced the Windows metadata fallback with native file replacement and made
  the online recording wait on its actual database port and a dynamic HTTP port.
- Made ambiguous identity creation retryable and quittable without replacing its
  device key, and kept unconfirmed identities out of the offline branch after a
  restart.
- Kept empty online waits on the branch screen and made the owner-only file
  confirmation keys explicit.
