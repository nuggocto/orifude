# Changelog

All notable changes to Orifude will be recorded here.

## Unreleased

### Added

- Initial product and technical specification.
- Public project presentation and contributor guidance.
- Apache-2.0 licensing.
- Go module and project-tool pins, sqlc paths, and secret-free baseline CI.
- Go and post-office development prerequisites and configuration documentation.

### Changed

- Settled identity, alias, retention, moderation, hosting, update, and 1.0
  release policy.
- Replaced bearer identity tokens and plaintext message columns with device-key
  authentication, short DPoP-bound sessions, KMS envelope encryption, and
  report-only audited moderation access.
- Deferred frontend scaffolding, tooling, assets, and CI from foundation work to
  landing-page delivery.
