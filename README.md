# Orifude

> Send a letter into the quiet and let a stranger find it.

Orifude is a pseudonymous, one-to-one letter exchange for the terminal. Choose a
private alias, write a short letter, and leave it for one unrelated recipient.
They may send one reply, and the exchange becomes a keepsake for both people.

Aliases are visible only to matched strangers and cannot be searched. There is
no public feed, profile, follower count, recipient picker, or endless chat. The
terminal interface is the application. [orifude.com](https://orifude.com) will
explain the project and distribute releases without access to letters.

Orifude is not end-to-end encrypted. TLS protects transport, and the post office
envelope-encrypts letter and reply bodies and retained report evidence with
externally held AWS KMS keys before PostgreSQL storage. The service decrypts
ordinary bodies for authorized participant reads. Cloudflare Access
authenticates and logs the supported evidence-retrieval path, the post office
audits each authorization to release ciphertext, and CloudTrail records each
decrypt by a human moderator role that the Railway runtime does not have.
Role-session records link the decrypt to the human SSO principal. Operational
metadata and pseudonymous aliases remain visible to the service, and a
compromised running post office could decrypt ordinary messages.

An identity belongs to one device key and uses short DPoP-bound sessions. There
is no recovery or second device. A separate credential may be stored offline to
delete a lost identity, but it cannot read letters or restore access.

Orifude is currently being designed and built. The product and technical
decisions live in [PROJECT.md](PROJECT.md).

## Development

Install Go 1.27.0, clone the repository, and download the pinned modules:

```sh
go mod download
```

The Go module also pins sqlc, Goose, and govulncheck. Run them through Go so no
separate global installation is needed:

```sh
go tool sqlc version
go tool goose --version
go tool govulncheck -version
```

The baseline checks do not need PostgreSQL, AWS, Cloudflare, or production
secrets:

```sh
if test -n "$(git ls-files 'sql/queries/*.sql')"; then go tool sqlc generate; fi
git diff --exit-code -- internal/database/dbgen
test -z "$(git status --porcelain --untracked-files=all -- internal/database/dbgen)"
if test -n "$(go list -e ./...)"; then
  go test ./...
  go test -race ./...
  go vet ./...
  go tool govulncheck ./...
fi
```

The guards skip database generation while there are no SQL queries and skip Go
package checks while there are no Go packages. They start running as soon as
those inputs exist.

Post-office development will require PostgreSQL and an AWS account with separate
message and evidence KMS keys. Moderation routes will also require a Cloudflare
Access application. Goose reads SQL migrations from `sql/migrations`:

```sh
go tool goose -dir sql/migrations postgres "$DATABASE_URL" up
```

The post office will read these environment variables once its server entrypoint
exists:

| Variable | Purpose |
| --- | --- |
| `DATABASE_URL` | PostgreSQL connection string |
| `LISTEN_ADDR` | HTTP listen address |
| `PUBLIC_ORIGIN` | Exact public origin used for DPoP validation |
| `MODERATION_ORIGIN` | Exact Access-protected moderation origin |
| `AWS_REGION` | Region containing both KMS keys |
| `MESSAGE_KMS_KEY_ARN` | Message-key ARN |
| `EVIDENCE_KMS_KEY_ARN` | Separate evidence-key ARN |
| `AWS_ACCESS_KEY_ID` | Restricted runtime AWS access key ID |
| `AWS_SECRET_ACCESS_KEY` | Restricted runtime AWS secret access key |
| `CF_ACCESS_ISSUER` | Exact Cloudflare Access issuer |
| `CF_ACCESS_AUDIENCE` | Moderation application audience |
| `LATEST_TUI_VERSION` | Version returned for passive update notices |
| `LOG_LEVEL` | Structured log threshold |

The internal moderator tool will use `AWS_PROFILE` for the operator's IAM
Identity Center profile and `MODERATOR_ROLE_ARN` for its 15-minute assumed role.
Production injects secrets through Railway; do not put real credentials in local
files or commits. Invite administration and rate-limit variable names will be
fixed with the server configuration that consumes them.

## Planned distribution

- GitHub Releases for Linux, macOS, and Windows binaries and checksums
- Homebrew, Scoop, and AUR packages
- Checksum-verifying shell and PowerShell installers

## License

Licensed under the [Apache License 2.0](LICENSE).
