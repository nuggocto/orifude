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

### Fixed

- Preserved drafts across unrelated reports and quit prompts, aligned contextual
  keyboard help with active controls, refreshed forms after terminal capability
  changes, completed ASCII fallback, and made missing Go packages fail CI.
- Rejected malformed UTF-8 and unpaired JSON surrogates before message handling.
- Bounded and coalesced Cloudflare Access certificate refreshes without holding
  the key-cache lock across network I/O.
- Used one PostgreSQL timestamp when consuming challenges and redeeming invites
  so eligibility and recorded consumption cannot cross an expiry boundary.
