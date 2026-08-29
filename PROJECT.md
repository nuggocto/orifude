# Orifude

> Send a letter into the quiet and let a stranger find it.

Orifude is an online, pseudonymous, one-to-one letter exchange experienced only
through a terminal user interface. A person writes a short letter, folds it,
and releases it. The post office assigns it to one unrelated recipient. That
recipient unfolds it and may send one reply. The exchange then becomes a
keepsake for both people.

The public website is a separate, static landing page. It explains the project,
shows the TUI, and distributes builds. It never reads, writes, or displays
letters.

This document is the product and technical baseline. Product and operational
policy is settled here before implementation begins.

## Product identity

- Product name: `orifude`
- TUI repository and Go module: `orifude`
- Landing-page repository: `orifude-front`
- Public domain: `https://orifude.com`
- Working tagline: `Send a letter into the quiet and let a stranger find it.`
- Participant client: the Orifude TUI only
- Public presentation: a static website only
- Primary motif: folded paper carried through an ink-painted garden

The name is a coined brand inspired by folding and brushwork. Marketing must not
present it as a dictionary Japanese word or claim cultural authenticity.

## Product promise

Orifude creates a small exchange between two people without turning that
exchange into content for an audience.

Each person chooses one unique alias. It is visible only to matched strangers
and is never searchable. The product has no public feed, follower graph,
profiles, likes, search, trending page, recipient picker, attachments, or
unrestricted direct messages. Those omissions are product rules, not missing
features.

### Core rules

1. Every letter has exactly one sender.
2. A letter can be claimed by at most one active recipient at a time.
3. The sender can never claim their own letter.
4. The recipient is selected by the server, not by the sender.
5. The body remains hidden until the assigned recipient explicitly unfolds it.
6. The recipient may send zero or one reply.
7. A reply cannot receive another reply.
8. Each participant sees the other's alias, but no profile, identifier, or
   activity history.
9. A claimed letter is one-reader, not one-view. Its recipient may reopen it.
10. The website cannot participate in the exchange.

### Goals

- Make receiving a small pseudonymous letter feel deliberate rather than noisy.
- Make the folding, carrying, and unfolding interactions memorable in a TUI.
- Let real users exchange letters online through one authoritative service.
- Work well with keyboards, Unicode text, narrow terminals, and limited color.
- Keep the implementation small enough to understand and operate alone.
- Protect participants from unauthorized reads and basic abuse from the start.

### Non-goals

- Real-time chat
- Multi-message conversations
- Social profiles or reputation scores
- Algorithmic engagement ranking
- Publicly browsable content
- File, image, audio, or link attachments
- End-to-end encryption; the post office decrypts authorized delivery and
  reported content while PostgreSQL stores only application-encrypted ciphertext
- A browser version of the letter application
- Push notifications or background notification services
- Native desktop wrappers
- Mouse interaction in the participant TUI; Orifude is keyboard-only
- A participant-facing command interface or family of CLI subcommands; the
  internal moderation command is operator tooling, not a product client
- Offline delivery between users

The participant program is launched as a binary, but all participant interaction
after launch is inside the TUI. Development, moderation, and server
administration may still use restricted commands and environment variables.

## User journeys

### First launch

1. The TUI displays the Orifude mark and checks terminal capabilities.
2. The TUI generates a P-256 device key and stores the private key in the
   operating system credential store, with an owner-only file fallback.
3. During private alpha, the person enters a single-use invite code.
4. The person chooses a unique, non-searchable alias.
5. The TUI creates a separate 32-byte delete-only revocation credential, shows
   it, and requires confirmation that the person stored it away from the device.
6. The TUI sends only its SHA-256 hash while proving possession of the device
   key with a server challenge. The post office creates the identity and returns
   a 15-minute DPoP-bound access token.
7. The TUI keeps the access token only in memory and never persists the
   revocation credential.
8. The person enters the branch screen. After an ambiguous registration result,
   the TUI tries a session challenge with the same device key instead of creating
   another identity or credential.

The alias is immutable, globally unique, and shown only to matched strangers.
It contains 2 to 24 Unicode code points after NFC normalization. Letters and
marks come from one Unicode script; Japanese may combine Han, Hiragana, and
Katakana. ASCII digits, single spaces, hyphens, and underscores are allowed.
Unicode TR39 confusable skeletons enforce uniqueness. Controls, invisible
formatting characters, emoji, and other script mixing are rejected. Deleted
aliases are never reused. There is no password, profile, biography, avatar,
email, recovery, private-key export, or second device. Losing the device key
loses access permanently. The offline revocation credential can only delete the
identity; it cannot read data or create a session. The service also treats an
identity as deleted after one year without an authenticated request.

### Delete an identity

1. An active person opens `Identity and local data` and chooses permanent
   deletion. A person who lost the device key chooses `Delete a lost identity`
   from the first-launch screen.
2. Active deletion uses the current DPoP session. Lost-identity deletion accepts
   the offline revocation credential inside the TUI, never through a flag or URL.
3. The TUI explains that deletion cannot be reversed and asks for confirmation.
4. The server applies the same deletion transaction for either path. Revocation
   always returns `204` whether or not the credential matched an active identity.
5. The TUI clears the entered credential from its model and returns to the
   first-launch screen. It does not create a replacement identity.

### Send a letter

1. Select `Fold a letter` from the branch screen.
2. Write between 1 and 2,000 Unicode code points.
3. Preview the folded form generated from the letter's `fold_seed`.
4. Confirm release.
5. The TUI sends the letter with a client-generated opaque ID.
6. The post office stores it in the waiting queue.
7. The sender receives a release receipt and can see its delivery state.

The client-generated ID makes a retried submission idempotent. A timeout after
a successful write must not create a duplicate letter. The sender can reread
the original while they retain their keepsake access.

### Receive a letter

1. Select `Wait by the branch`.
2. The TUI asks the post office for one available letter.
3. The post office returns an existing unexpired claim for this identity or
   atomically claims the oldest eligible letter.
4. The TUI displays only its folded form and age.
5. The person explicitly unfolds it.
6. The post office records the open and returns the body.
7. The person may reply, keep it without replying, report it, or discard it.

An unopened claim expires after 24 hours. After expiry, the letter returns to
the queue and can be claimed by someone else. An unclaimed letter expires and
is deleted seven days after release. Once opened, assignment is permanent.

### Reply

1. The recipient selects `Fold a reply` on an opened letter.
2. The reply accepts between 1 and 2,000 Unicode code points.
3. The recipient previews and confirms it.
4. The server writes it only if no reply already exists.
5. The sender sees the reply in the same keepsake.

The client supplies an opaque reply ID. Retrying that ID returns the existing
result without comparing plaintext or ciphertext. Any other reply ID is
rejected after the first reply succeeds.

### Report and block

1. The receiver of an original letter or reply selects `Report and burn`.
2. The TUI asks for one reason from a fixed list. It collects no free-form report
   text.
3. The post office decrypts the reported message and immediately re-encrypts an
   evidence copy under the separate moderation KMS key.
4. The server records whether the evidence is the original or reply, hides the
   exchange from the reporter, and permanently blocks future matching.
5. The block is hidden from the other person and cannot be reversed.
6. A moderator may decrypt only evidence attached to a report. The service
   audits each authorization to release ciphertext. CloudTrail records every
   role assumption and KMS decrypt attempt; their session identifiers attribute
   the report ID, fixed purpose, and time to the human SSO principal.
7. Closing the case starts retention. Live evidence is deleted after 90 days;
   disaster-recovery copies expire no more than 30 days later. Report metadata
   remains for one year after closure; each audit event remains for one year.

### Moderator review

1. The moderator opens the report queue through Cloudflare Access and submits an
   idempotent review claim for a known report ID or the oldest unreviewed report.
   The service marks the first authorized review and returns only metadata plus
   the encrypted envelope.
2. The moderator authenticates to AWS IAM Identity Center. The internal tool
   assumes the 15-minute evidence-decrypt role and displays the evidence in the
   attached terminal.
3. The moderator closes the case through Cloudflare Access with one fixed
   disposition. There are no free-form moderation notes.
4. The close transaction records its audit event and sets evidence and report
   purge times. Closing the same case with the same disposition is idempotent;
   changing a closed disposition is rejected.
5. If the disposition disables the reported identity, the transaction applies
   the standard identity-deletion cleanup to the report's snapshotted target ID.
   The request cannot name another identity.

The recipient sees the sender's alias but never receives an internal identifier.
Blocking is applied server-side using the letter relationship.

## Letter lifecycle

State is derived from timestamps and ownership columns rather than duplicated
in a mutable status string.

```text
                  24-hour claim expires
             +---------------------------------+
             |                                 |
             v                                 |
[waiting] --claim--> [folded/claimed] --unfold--> [opened] --reply--> [replied]
    |      |                                     |                    |
withdraw  7 days                                report               report
    |      |                                     |                    |
    v      v                                     v                    v
[withdrawn] [expired]                        [reported]           [reported]
```

Derived states:

| State | Required facts |
| --- | --- |
| Waiting | No recipient, not withdrawn, not reported, not expired |
| Claimed | Recipient and claim timestamps exist, not opened |
| Opened | Recipient and opened timestamp exist, no reply |
| Replied | Reply ciphertext, wrapped key, reply ID, and timestamp exist |
| Withdrawn | Sender withdrew before a successful claim |
| Expired | Seven days elapsed before claim; cleanup deletes the row |
| Reported | A report exists for the letter |

A state transition is authorized and committed by the post office. The TUI
never decides that a transition succeeded based only on local state.

## System architecture

```text
                         public internet

 +----------------+       HTTPS/JSON       +----------------------+
 | Orifude TUI    | ---------------------> | Go post office       |
 | Bubble Tea     | <--------------------- | Chi + net/http       |
 +----------------+                        +----------+-----------+
                                              |              |
                                          pgx |              | TLS
                                              v              v
                                 +-------------------+  +-------------------+
                                 | PostgreSQL        |  | AWS KMS           |
                                 | ciphertext + state|  | envelope keys     |
                                 +-------------------+  +-------------------+

moderator tool -- Cloudflare Access --> moderation API
               -- AWS SSO ----------> evidence-key decrypt

 +----------------+       Cloudflare Pages
 | orifude-front  | ---------------------> browser
 | Astro + Vite   |       https://orifude.com
 +----------------+

 The landing page has no path to the letter API or database.
```

### Runtime responsibilities

The TUI owns presentation, keyboard input, the device private key, DPoP proofs,
in-memory access tokens, request timeouts, retry prompts, and previewing the
stable fold seed derived from its random opaque letter ID. The post office
derives the same seed, persists it, and returns it for later rendering.

The post office owns device-key authentication, DPoP validation, authorization,
plaintext validation, envelope encryption, bounded decryption, letter state
transitions, recipient assignment, claim leases, rate limits, reports, blocks,
moderation gates, audit events, and persistence.

PostgreSQL owns durable truth, encrypted message records, uniqueness,
referential integrity, session and replay state, audit metadata, and the row
locks that prevent two people claiming one letter.

AWS KMS generates and wraps one AES-256 data key per message or evidence
envelope. The post office holds plaintext data keys only for the current
request. CloudTrail records KMS
use outside Railway. Cloudflare Access authenticates supported moderation API
requests before they return encrypted evidence. A federated human AWS role
decrypts that evidence in the operator tool; Railway never receives
evidence-decrypt permission.

The landing page owns project presentation, downloads, TUI recordings, privacy
information, and documentation links. It contains no application JavaScript
that talks to the post office.

## Technical stack

### Language and runtime

- Go `1.27.0` is the current workspace toolchain and initial module directive.
- CI and releases must use the latest patch of the selected supported Go line.
- `context.Context` is passed through every network and database operation.
- `log/slog` provides structured server logs.
- `encoding/json` defines the HTTP representation.
- `net/http` provides the HTTP server, timeouts, and graceful shutdown.
- `github.com/go-chi/chi/v5` provides routing and narrowly scoped middleware
  while preserving standard `http.Handler` contracts.
- `crypto/rand` creates P-256 device keys, access and revocation credentials,
  data nonces, and public IDs. A letter's random opaque ID deterministically
  derives its non-secret fold seed so pre-release and delivered folds agree.
- `crypto/aes` and `cipher.AEAD` provide AES-256-GCM message encryption.
- `crypto/sha256` hashes opaque credentials and DPoP access tokens.
- `github.com/go-jose/go-jose/v4` handles JWK, JWS, JWT, DPoP, and Cloudflare
  Access JWT verification with an explicit algorithm allowlist.
- `github.com/zalando/go-keyring` stores the device private key in macOS
  Keychain, Windows Credential Manager, or Linux Secret Service when available.

### TUI

- `charm.land/bubbletea/v2` provides the Elm-style `Model`, `Update`, `View`, and
  command runtime.
- `charm.land/bubbles/v2` provides the textarea, viewport, spinner, key bindings,
  and help components that are actually needed.
- `charm.land/lipgloss/v2` provides layout, borders, terminal-aware color
  downsampling, and light/dark styling.
- `charm.land/huh/v2` provides embedded onboarding, confirmation, settings, and
  report forms. Its accessible mode is exposed as a TUI setting.
- No terminal image protocol is required. The supplied art is translated into
  compact ANSI line art for the TUI.
- No animation library is planned. Short fold animations use Bubble Tea tick
  commands and a small fixed frame slice.
- Charm's VHS records deterministic terminal demos for the landing page and
  release notes. VHS is development tooling, not a runtime dependency.

Bubble Tea commands perform HTTP I/O outside `Update`. Results return as typed
messages. The program does not start unmanaged goroutines.

### Post office

- PostgreSQL 18 is the development and CI compatibility target. The initial
  schema uses no extensions; production confirms the same major version before
  provisioning.
- Chi v5 routes a versioned JSON API on top of `net/http`.
- `github.com/jackc/pgx/v5/pgxpool` provides PostgreSQL access and pooling.
- `sqlc` generates typed pgx v5 query code from reviewed SQL in `sql/queries`.
- Generated sqlc code lives in `internal/database/dbgen` and is never edited by
  hand. `internal/database` remains the home of handwritten pool and transaction
  lifecycle code.
- Goose runs numbered SQL migrations from `sql/migrations` as one deployment
  step. Migrations do not run in every server replica at startup.
- One `pgxpool.Pool` is created, pinged during startup, shared by handlers, and
  closed during bounded graceful shutdown.
- AWS SDK for Go v2 calls KMS `GenerateDataKey` and `Decrypt` with a fixed,
  non-secret encryption context.
- The internal moderator tool also uses AWS STS `AssumeRole` after IAM Identity
  Center authentication.
- Each original, reply, and evidence envelope uses a fresh 256-bit data key and
  96-bit AES-GCM nonce. The database stores ciphertext and the KMS-wrapped data
  key, never plaintext.
- Cloudflare Access protects every moderation route; the origin validates
  issuer, audience, signature, expiry, and moderator policy on every request.
  Only review routes return one encrypted report envelope.
- The internal `cmd/moderate` tool reads one encrypted envelope from standard
  input, uses the AWS SDK default credential chain after human SSO, assumes the
  evidence-decrypt role for 15 minutes, and renders plaintext only to an attached
  terminal. It is not included in public release archives.

### Landing page

- Astro in static-output mode with strict TypeScript
- Vite through Astro's supported build pipeline
- Tailwind CSS 4 through the official `@tailwindcss/vite` plugin
- pnpm with a committed lockfile and pinned package-manager version
- Oxlint for JavaScript, TypeScript, and Astro script-block linting
- Oxfmt for Astro, TypeScript, CSS, JSON, YAML, and Markdown formatting
- Zod for build-time release metadata and environment validation
- Ky only when the site gains a real HTTP request. The initial static site does
  not install an HTTP wrapper merely to have one.
- No React, Vue, Svelte, or client island unless an interaction cannot be done
  with Astro, HTML, and CSS.
- Optimized WebP artwork plus a PNG application icon and favicon

### Operations

- One Go post-office service initially
- One Railway service and one Railway PostgreSQL database in the same project
- Railway private networking between the service and database
- No routine human database write access; operator access is read-only unless a
  documented break-glass repair requires otherwise
- Two AWS customer-managed symmetric KMS keys: one for message data keys and one
  for reported evidence, both with annual automatic rotation
- A runtime IAM principal limited to `DescribeKey`, message `GenerateDataKey`
  and `Decrypt`, and evidence `GenerateDataKey`; it cannot manage or delete keys
- A human IAM Identity Center role that may only assume a 15-minute moderator
  role; the moderator role may only decrypt reported evidence, and no
  evidence-decrypt credential exists in Railway
- KMS key policies require the documented message or evidence context on
  `GenerateDataKey` and `Decrypt`. `DescribeKey` is principal-scoped because it
  accepts no encryption context.
- One-year CloudTrail and Cloudflare Access audit retention, with alerts for
  unusual KMS decrypt volume and key administration
- Cloudflare Access on `moderation.orifude.com` with origin JWT validation
- TLS terminated by the deployment platform or reverse proxy
- Cloudflare Pages for `orifude-front`, connected to its GitHub repository
- `orifude.com` as the canonical custom domain and `www.orifude.com` redirected
  to it
- GitHub Releases with SHA-256 checksums; artifact signing is out of scope
- GoReleaser for release archives and checksums once `cmd/orifude` exists
- Homebrew through `nuggocto/homebrew-tap`, Scoop through
  `nuggocto/scoop-bucket`, and AUR through its separate SSH-backed repository
- Cloudflare DNS and TLS for the public domain
- Railway variables inject restricted AWS credentials; they are never present
  in source, build arguments, logs, or client responses

## Repository design

The intended repositories are siblings:

```text
/home/nuggocto/Documents/code/
|
+-- orifude/
|   +-- cmd/
|   |   +-- orifude/
|   |   |   +-- main.go              # TUI entrypoint
|   |   +-- postoffice/
|   |       +-- main.go              # HTTP service entrypoint
|   |   +-- moderate/
|   |       +-- main.go              # internal evidence decrypt tool
|   +-- internal/
|   |   +-- tui/
|   |   |   +-- model.go             # complete UI state
|   |   |   +-- update.go            # messages and transitions
|   |   |   +-- view.go              # screen composition
|   |   |   +-- styles.go            # adaptive Orifude styles
|   |   |   +-- fold.go              # deterministic ANSI fold frames
|   |   +-- api/
|   |   |   +-- types.go             # versioned request/response DTOs
|   |   |   +-- client.go            # bounded TUI HTTP client
|   |   +-- auth/
|   |   |   +-- dpop.go              # challenges, proofs, and sessions
|   |   +-- httpapi/
|   |   |   +-- router.go            # Chi routes and middleware order
|   |   |   +-- letters.go           # letter transport and error mapping
|   |   |   +-- identities.go        # identity transport
|   |   |   +-- moderation.go        # report and block transport
|   |   +-- identity/
|   |   |   +-- local.go             # device keyring and file fallback
|   |   +-- envelope/
|   |   |   +-- envelope.go          # AES-GCM and AWS KMS data keys
|   |   +-- postoffice/
|   |   |   +-- letters.go           # use cases and authorization
|   |   |   +-- claims.go            # atomic claim operation
|   |   |   +-- moderation.go        # report and block operations
|   |   +-- database/
|   |       +-- database.go          # pgx pool ownership and health
|   |       +-- dbgen/               # generated by sqlc; never hand-edit
|   |           +-- db.go
|   |           +-- models.go
|   |           +-- identities.sql.go
|   |           +-- sessions.sql.go
|   |           +-- letters.sql.go
|   |           +-- claims.sql.go
|   |           +-- moderation.sql.go
|   +-- sql/
|   |   +-- migrations/
|   |   |   +-- 00001_initial.sql    # Goose Up and Down sections
|   |   +-- queries/
|   |       +-- identities.sql
|   |       +-- sessions.sql
|   |       +-- letters.sql
|   |       +-- claims.sql
|   |       +-- moderation.sql
|   +-- PROJECT.md
|   +-- README.md
|   +-- go.mod
|   +-- go.sum
|   +-- sqlc.yaml
|   +-- mise.toml                    # pinned toolchain and development tasks
|
+-- orifude-front/
    +-- public/
    |   +-- assets/
    |   |   +-- orifude-logo.webp
    |   |   +-- orifude-watermark.webp
    |   |   +-- orifude-icon.png
    |   |   +-- orifude-mark-mono.webp
    |   |   +-- terminal-demo.webm
    |   +-- favicon.ico
    |   +-- robots.txt
    +-- src/
    |   +-- components/
    |   |   +-- Hero.astro
    |   |   +-- TerminalDemo.astro
    |   |   +-- Download.astro
    |   +-- data/
    |   |   +-- releases.ts          # Zod-validated static metadata
    |   +-- layouts/
    |   |   +-- BaseLayout.astro
    |   +-- pages/
    |   |   +-- index.astro
    |   +-- styles/
    |       +-- global.css           # Tailwind 4 and design tokens
    +-- astro.config.ts
    +-- tsconfig.json
    +-- public/_headers              # Cloudflare Pages security headers
    +-- public/_redirects            # canonical-domain redirects
    +-- .oxlintrc.json
    +-- .oxfmtrc.json
    +-- package.json
    +-- pnpm-lock.yaml
    +-- README.md
```

This is a target tree, not a requirement to create every file immediately.
Closely related code should stay together until a file becomes difficult to
navigate. There will be no handler/service/repository interface for every
operation. Concrete types come first.

### sqlc and migration boundaries

`sqlc.yaml` reads the PostgreSQL schema from `sql/migrations`, reads named
queries from `sql/queries`, uses the `pgx/v5` SQL package, and writes package
`dbgen` to `internal/database/dbgen`.

Generated files are committed so a release build does not need sqlc installed.
CI runs generation and fails on a diff. Schema changes start in a Goose
migration, query changes stay in the relevant `.sql` file, and application code
never embeds an alternate copy of those queries.

Goose Down is supported for local and disposable databases before a migration
is released. Production never rolls back a released schema with Down; it uses a
new reviewed forward-repair migration. Released migrations are immutable.

The package split is deliberate:

- `internal/database` owns the pool, readiness, shutdown, and transaction entry
  points.
- `internal/database/dbgen` contains only sqlc output.
- `internal/auth` owns device challenges, DPoP proof validation, access-token
  issuance, and replay checks.
- `internal/envelope` owns AES-GCM and KMS calls. Plaintext and unwrapped data
  keys do not cross that package boundary except as the immediate call result.
  It defines the two-method KMS caller needed for tests; production passes the
  AWS client directly, and no local-KMS mode exists in the shipped binary.
- `internal/postoffice` owns the transaction boundary for a complete use case
  and uses `dbgen.Queries.WithTx` inside that transaction.
- `internal/httpapi` maps HTTP to post-office operations. It does not contain SQL.

Go interfaces are added only where a consuming package needs a narrow seam.
A broad generated `Querier` interface and another repository wrapper are not
enabled by default.

## TUI navigation tree

```text
Splash
|
+-- First-run onboarding
|   +-- Service explanation
|   +-- Create identity
|   |   +-- Device key generation
|   |   +-- Invite code
|   |   +-- Alias and identity creation
|   |   +-- Key-loss and delete-credential warning
|   +-- Delete a lost identity
|
+-- Branch
    +-- Fold a letter
    |   +-- Compose
    |   +-- Fold preview
    |   +-- Release confirmation
    |   +-- Delivery receipt
    |
    +-- Wait by the branch
    |   +-- Searching
    |   +-- Folded delivery
    |   +-- Unfold animation
    |   +-- Read
    |       +-- Fold a reply
    |       +-- Keep
    |       +-- Report and burn
    |
    +-- Keepsakes
    |   +-- Sent
    |   +-- Received
    |   +-- Exchange detail
    |       +-- Report received message
    |
    +-- Settings
        +-- Motion
        +-- Theme and contrast
        +-- Connection status
        +-- Identity and local data
        |   +-- Permanently delete identity
        +-- About
```

Global keys:

| Key | Action |
| --- | --- |
| `?` | Toggle contextual help |
| `esc` | Leave text-input mode; never navigate back |
| `tab` / `shift+tab` | Move focus |
| `b` | Close the current detail or go back |
| `j` / down arrow | Move down one item or line |
| `k` / up arrow | Move up one item or line |
| `l` / right arrow / `enter` | Open, select, or move right |
| `g g` / `home` | Move to the first item or top |
| `G` / `end` | Move to the last item or bottom |
| `ctrl+u` / `ctrl+d` | Scroll half a page up or down |
| `ctrl+b` / `ctrl+f` | Scroll a full page up or down |
| `i` | Focus the composer and enter text-input mode |
| `q` | Quit from navigation mode, with confirmation for a draft |
| `ctrl+c` | Quit, with confirmation if a draft exists |

Navigation keeps familiar Neovim movement keys but uses `b` consistently for
back. Orifude does not implement a complete modal text editor. While a Bubbles
textarea or Huh text field has focus, printable keys edit text and `q` is
ordinary letter content. In a Bubbles textarea, `esc` returns to navigation
mode; in a Huh text field, it finishes typing and moves focus to the next
control. It never closes a form or changes screens; use `b` when it is
available, or select the form's explicit negative action. This keeps the
bindings predictable without owning a second editor implementation.

## TUI model and in-memory data

The root Bubble Tea model is one cohesive struct. Screen-specific values remain
in the root until independent components have a demonstrated lifecycle.

```go
type Model struct {
    screen     Screen
    mode       InputMode
    width      int
    height     int
    online     bool
    busy       bool
    identity   Identity
    draft      textarea.Model
    form       *huh.Form
    keepsakes  []LetterSummary
    current    *Letter
    cursor     int
    foldFrame  int
    err        error
    styles     Styles
}
```

This is a shape guide, not a frozen API.

### Selected structures

| Need | Representation | Reason |
| --- | --- | --- |
| Current screen | Small `Screen` integer enum | Valid states are finite and type checked |
| Navigation or text mode | Small `InputMode` enum | Prevents `q` and motions from stealing typed text |
| Ordered keepsakes | `[]LetterSummary` | Display and pagination are ordered scans |
| Current letter | `*Letter` | At most one detail is active |
| Fixed fold animation | `[]string` | Frames are bounded, ordered, and immutable |
| Key bindings | One concrete key-map struct | Bubbles help can consume bindings directly |
| Short forms | Embedded `*huh.Form` | Reuses validation, selection, confirmation, and accessible mode |
| Async results | Concrete Bubble Tea message structs | Type switches match the framework model |
| Local identity | P-256 private key and immutable alias | The key stays in the OS credential store or owner-only fallback file |
| Access session | Opaque token in memory | Sessions last 15 minutes and restart requires a fresh signed challenge |

A `map[LetterID]Letter` is not planned. The TUI displays ordered pages and one
active letter, so keyed random access would duplicate state without serving a
common operation. The server remains the source of truth.

### TUI state rules

- `Update` is the only place that mutates the model.
- `View` performs no I/O and does not mutate state.
- Every network command has a context deadline.
- A busy mutation disables duplicate submissions but never blocks navigation
  away from a non-destructive request.
- Late responses include an operation ID and are ignored if their screen or
  operation is no longer current.
- Draft text stays local until release is confirmed.
- Text-input mode receives key presses before global navigation bindings.
- Letter plaintext, device keys, access tokens, revocation credentials,
  DPoP proofs, and encryption material are never written to logs.

## Persistent data model

The expected initial workload is modest: one write per sent letter, one atomic
claim, one open, and at most one reply. Each protected request also commits one
small replay row that expires within 15 minutes. Reads are identity-scoped
keepsake pages and claim lookups. The participant data path uses one Go service,
one PostgreSQL database, and AWS KMS. Moderation also uses Cloudflare Access and
AWS STS, but adds no hosted application. Another component is allowed only when
load tests or production metrics prove this design cannot meet a published
service objective.

### `identities`

| Column | Type | Rules |
| --- | --- | --- |
| `id` | `bigint generated always as identity` | Primary key, internal only |
| `public_key` | `bytea` | Required, unique immutable 65-byte P-256 public key |
| `key_thumbprint` | `bytea` | Required, unique 32-byte RFC 7638 SHA-256 thumbprint |
| `revocation_hash` | `bytea` | Nullable; unique and 32 bytes while active |
| `alias` | `text` | Nullable; unique canonical alias while active |
| `alias_key` | `text` | Nullable; unique comparison key while active |
| `created_at` | `timestamptz` | Required, server time |
| `last_seen_at` | `timestamptz` | Required, updated by authenticated use |
| `deleted_at` | `timestamptz` | Nullable; set by explicit or inactivity deletion |

The client generates one P-256 key with the operating system CSPRNG and sends
only its public JWK. The server accepts exactly `EC`/`P-256`/`ES256`, validates
the point, stores the uncompressed public key and RFC 7638 thumbprint, and never
receives the private key.

The TUI also generates one 32-byte random revocation credential encoded with
unpadded base64url and sends only `SHA-256(credential)` in the key-proven
registration request. It is delete-only, is shown once, and is not recoverable
or replaceable. Explicit deletion clears its hash. The public key and thumbprint
remain reserved so a deleted or compromised device key cannot create another
identity.

Alias input is normalized and validated before both display and comparison
values are stored. Matching recipients may see `alias`; no endpoint searches or
lists aliases. An alias is immutable. Deletion moves `alias_key` to
`alias_reservations` before clearing the identity's revocation hash and alias
fields. Identity creation checks active and reserved keys in one transaction.

### `auth_challenges`

| Column | Type | Rules |
| --- | --- | --- |
| `id` | `varchar(22)` | Primary key, 128-bit random base64url ID |
| `identity_id` | `bigint` | Nullable identity foreign key; set for session challenges |
| `public_key` | `bytea` | Required 65-byte P-256 public key |
| `key_thumbprint` | `bytea` | Required 32-byte thumbprint |
| `purpose` | `smallint` | Registration or session enum |
| `nonce_hash` | `bytea` | Required 32-byte SHA-256 hash |
| `created_at` | `timestamptz` | Required |
| `expires_at` | `timestamptz` | Five minutes after creation |
| `used_at` | `timestamptz` | Nullable; set once before issuing credentials |

Challenges are random 32-byte nonces returned once. Registration and session
proofs are ES256 DPoP JWTs bound to the stored key, challenge nonce, HTTP method,
and canonical public URI. Challenge consumption and identity or session creation
commit in one transaction. Expired and used rows are deleted asynchronously.

### `access_sessions`

| Column | Type | Rules |
| --- | --- | --- |
| `token_hash` | `bytea` | Primary key, 32-byte SHA-256 hash |
| `identity_id` | `bigint` | Required identity foreign key |
| `key_thumbprint` | `bytea` | Required; must match the identity |
| `created_at` | `timestamptz` | Required |
| `expires_at` | `timestamptz` | 15 minutes after creation |
| `revoked_at` | `timestamptz` | Nullable |

The opaque access token is 32 random bytes encoded with unpadded base64url and
is never persisted by the TUI. Every authenticated request supplies the token
with scheme `DPoP` and an ES256 proof containing `typ=dpop+jwt`, the public JWK,
`htm`, `htu`, `iat`, a random `jti`, and `ath`. The server derives `htu` from a
configured public origin rather than forwarded host headers, permits at most 30
seconds of clock skew, and rejects a repeated `jti`. Resource requests do not
require an additional nonce; the short lifetime, signed session challenge, and
server-side replay table provide freshness without serializing requests.

### `dpop_replays`

| Column | Type | Rules |
| --- | --- | --- |
| `session_token_hash` | `bytea` | Access-session foreign key |
| `jti_hash` | `bytea` | SHA-256 of the proof ID |
| `expires_at` | `timestamptz` | Same as the access session |

The composite primary key is `(session_token_hash, jti_hash)`. Proof validation
inserts the pair before the protected operation; a conflict is a replay. Rows
expire with the session. Session validation also checks `identities.deleted_at`
on every request, so deletion invalidates outstanding sessions immediately.

### `alias_reservations`

| Column | Type | Rules |
| --- | --- | --- |
| `alias_key` | `text` | Primary key; never reused |
| `reserved_at` | `timestamptz` | Required |

This table has one job: prevent impersonation through reuse of a deleted alias.
It contains no profile, activity, or contact data.

### `invites`

| Column | Type | Rules |
| --- | --- | --- |
| `token_hash` | `bytea` | Primary key; hash of a random high-entropy code |
| `created_at` | `timestamptz` | Required |
| `expires_at` | `timestamptz` | Seven days after creation |
| `redeemed_at` | `timestamptz` | Nullable; may be set once |
| `redeemed_by` | `bigint` | Nullable identity foreign key; agrees with redemption |
| `revoked_at` | `timestamptz` | Nullable; only before redemption |

Private-alpha invites are single-use, expire after seven days, and can be
revoked before redemption. Only restricted operator tooling may issue or revoke
them. Public registration does not require an invite once moderation, rate
limits, privacy policy, and operations are ready.

This phase implements invite persistence, redemption, revocation queries, and
synthetic test seeding. The restricted issuance command and its production
credential are deployment work and are not exposed by the participant API.

### `letters`

| Column | Type | Rules |
| --- | --- | --- |
| `id` | `varchar(22)` | Primary key, 128-bit client-generated base64url ID |
| `sender_id` | `bigint` | Required identity foreign key |
| `recipient_id` | `bigint` | Nullable identity foreign key |
| `sender_alias` | `text` | Immutable keepsake snapshot |
| `recipient_alias` | `text` | Follows an active claim; frozen when opened |
| `body_ciphertext` | `bytea` | Required AES-256-GCM ciphertext and tag |
| `body_nonce` | `bytea` | Required fresh random 12-byte nonce |
| `body_wrapped_key` | `bytea` | Required KMS-wrapped 256-bit data key |
| `body_kms_key_id` | `text` | Required KMS key ARN returned by KMS |
| `body_encryption_version` | `smallint` | Required; initially `1` |
| `fold_seed` | `bigint` | Non-negative random 63-bit value |
| `created_at` | `timestamptz` | Required |
| `claimed_at` | `timestamptz` | Nullable, agrees with recipient |
| `claim_expires_at` | `timestamptz` | Nullable, unopened claims only |
| `opened_at` | `timestamptz` | Nullable, requires recipient |
| `reply_id` | `varchar(22)` | Nullable, unique 128-bit client-generated retry ID |
| `reply_ciphertext` | `bytea` | Nullable AES-256-GCM ciphertext and tag |
| `reply_nonce` | `bytea` | Nullable fresh random 12-byte nonce |
| `reply_wrapped_key` | `bytea` | Nullable KMS-wrapped 256-bit data key |
| `reply_kms_key_id` | `text` | Nullable KMS key ARN returned by KMS |
| `reply_encryption_version` | `smallint` | Nullable; initially `1` |
| `replied_at` | `timestamptz` | Nullable; all reply fields appear together |
| `withdrawn_at` | `timestamptz` | Nullable, only before claim |
| `expires_at` | `timestamptz` | Required; seven days after creation |
| `sender_removed_at` | `timestamptz` | Nullable; hides the sender's keepsake |
| `recipient_removed_at` | `timestamptz` | Nullable; hides the recipient's keepsake |

The original and reply share one row because the product permits exactly two
messages. A generic `conversations` plus `messages` design was rejected. It
would add a relationship, joins, ordering rules, and impossible multi-message
states for flexibility Orifude does not offer. Multi-turn conversation is a
permanent non-goal.

The 2,000-code-point limit is long enough for an actual letter, including
Japanese text, while still fitting a terminal viewport and a tightly bounded
request. The API limits the complete JSON request to 16 KiB before decoding.
The TUI shows the remaining code-point count and byte validation errors before
release. Go validates code points and UTF-8 bytes before encryption. Database
`CHECK` constraints enforce 12-byte nonce sizes, ciphertext from 17 to 12,304
bytes, wrapped keys from 1 to 6,144 bytes, KMS key ARNs from 1 to 2,048 bytes,
positive encryption versions, and all-or-none reply fields.

Each original and reply gets a fresh KMS-generated data key and nonce. AES-GCM
additional authenticated data and KMS encryption context both bind the schema
version, letter ID, `original` or `reply` part, and reply ID when present.
Context values are deterministic and non-secret. Decryption fails closed if
bound envelope metadata, ciphertext, wrapped key, nonce, or context is altered.
Existing KMS ciphertext remains
decryptable after automatic KMS key rotation; `*_kms_key_id` records provenance
and the service always uses the key ARN returned by KMS rather than trusting a
client value.

### `blocks`

| Column | Type | Rules |
| --- | --- | --- |
| `blocker_id` | `bigint` | Identity foreign key |
| `blocked_id` | `bigint` | Identity foreign key, differs from blocker |
| `created_at` | `timestamptz` | Required |

The composite primary key is `(blocker_id, blocked_id)`. Matching excludes a
pair if either direction is blocked. A block cannot be reversed while both
identities exist. Identity deletion removes the now-irrelevant row; a deleted
alias and device key cannot return.

### `reports`

| Column | Type | Rules |
| --- | --- | --- |
| `id` | `varchar(22)` | Primary key, 128-bit client-generated report ID |
| `letter_id` | `varchar(22)` | Required immutable source ID; no foreign key |
| `reporter_id` | `bigint` | Required immutable identity ID; no foreign key |
| `reported_identity_id` | `bigint` | Required immutable identity ID; no foreign key |
| `target` | `smallint` | Original or reply enum |
| `reason` | `smallint` | Fixed application enum |
| `created_at` | `timestamptz` | Required |
| `reviewed_at` | `timestamptz` | Nullable |
| `reviewed_by` | `text` | Nullable verified Access `sub`; agrees with `reviewed_at` |
| `disposition` | `smallint` | Nullable fixed closure enum |
| `closed_at` | `timestamptz` | Nullable |
| `evidence_purge_at` | `timestamptz` | Nullable; 90 days after closure |
| `record_purge_at` | `timestamptz` | Nullable; one year after closure |
| `evidence_ciphertext` | `bytea` | Nullable only after purge; exact encrypted message |
| `evidence_nonce` | `bytea` | Nullable only after purge; otherwise 12 bytes |
| `evidence_wrapped_key` | `bytea` | Nullable only after purge; wrapped by moderation key |
| `evidence_kms_key_id` | `text` | Nullable only after purge; moderation key ARN |
| `evidence_encryption_version` | `smallint` | Nullable only after purge; initially `1` |
| `evidence_purged_at` | `timestamptz` | Nullable; agrees with cleared evidence fields |

`(letter_id, reporter_id)` is unique. The source and identity columns are
deliberately independent snapshots, so deleting a participant's last keepsake
copy or identity does not delete an active report. Free-form moderation notes
are excluded from the user-facing report to avoid collecting another body of
unsafe text.
The post office can decrypt the reported message under the message-key policy.
It keeps an evidence copy under the separate moderation KMS key, deletes the
live envelope 90 days after closure, and deletes report metadata one year after
closure. Backup copies expire within 30 days of their corresponding live-data
deletion. The Railway runtime can create evidence data keys but cannot decrypt
them. A participant's keepsake copy follows the normal keepsake retention rule.

Report reasons are fixed values for harassment, hateful content, sexual content,
threats, spam or scams, exposed personal information, and other unsafe content.
Closure dispositions are `no_action`, `duplicate`, and `identity_disabled`.
`identity_disabled` applies only to `reported_identity_id` from the report.

### `rate_limit_events`

| Column | Type | Rules |
| --- | --- | --- |
| `id` | `bigint generated always as identity` | Primary key |
| `identity_id` | `bigint` | Required identity foreign key |
| `kind` | `smallint` | Required fixed successful-operation enum |
| `created_at` | `timestamptz` | Required, server time |

The table is the durable source for per-identity cooldown, hourly, and daily
windows. Events are inserted in the same transaction as the successful limited
operation and deleted after the longest configured window. Deployment-edge IP
limits remain an edge concern and never store forwarded network data here.

### `moderation_audit`

| Column | Type | Rules |
| --- | --- | --- |
| `id` | `bigint generated always as identity` | Primary key |
| `request_id` | `varchar(22)` | Required, unique 128-bit random base64url ID |
| `report_id` | `varchar(22)` | Required requested report ID; intentionally no foreign key |
| `moderator_subject` | `text` | Verified Cloudflare Access `sub`, never email |
| `action` | `smallint` | Review claim or case close enum |
| `purpose` | `text` | Required constant `reported-content-review` |
| `outcome` | `smallint` | Authorized or denied enum |
| `created_at` | `timestamptz` | Required |
| `purge_at` | `timestamptz` | Required; one year after creation |

The lack of a foreign key lets denied requests for missing evidence remain in
the audit. Moderation endpoints accept a client request ID and the fixed purpose;
direct review and close also accept one report ID. For evidence review, the
service reads and buffers the envelope and writes an
`authorized` audit row in one transaction, commits, and only then returns the
ciphertext. It does not claim that a disconnected client received it. A denied
new operation is audited when the database is available; a retry keeps its
original operation row. Audit failure denies release. The operator tool calls
`GetCallerIdentity`, then uses its SSO base role
to assume the evidence-decrypt role for 15 minutes with a unique session name
containing the report ID and a random request suffix. CloudTrail records the base
principal on `AssumeRole` and the resulting session on `Decrypt`; the shared
session identifier provides human attribution. KMS context contains the report
ID, target, and fixed purpose. A previously retrieved envelope can be
decrypted again, but each attempt remains in CloudTrail. Plaintext never returns
to Railway. Database audit rows contain no content or email and expire after one
year.

Both review routes require a client request ID. A retry by the same moderator
returns the same authorized report; another owner gets a non-enumerating
conflict. The `next` claim uses `FOR UPDATE SKIP LOCKED` to select the oldest
unreviewed report. Direct review locks its requested report. Both set the first
`reviewed_at` and insert the audit row in that transaction. They authorize
evidence only when `evidence_purged_at IS NULL` and `evidence_purge_at` is null or
later than server time. Cleanup locks the same report row, so release and purge
cannot race. The report ID in the review and audit record remains the recovery
path if review stops before close. Close requires `reviewed_at`, is
first-write-wins on disposition, and sets all retention timestamps in its audit
transaction.

Review idempotency never extends retention. A retry returns the same envelope
only while evidence remains eligible. At or after `evidence_purge_at`, it returns
`evidence_expired` without ciphertext even if the first attempt was authorized;
the original audit row remains unchanged and edge and access logs record the
retry.

A `next` review request that finds no eligible report returns the empty-queue
response without an audit row because no report or evidence was selected for an
authorization decision.

### Required indexes

```sql
CREATE INDEX letters_waiting_idx
    ON letters (created_at, id)
    WHERE recipient_id IS NULL
      AND withdrawn_at IS NULL;

CREATE INDEX letters_expiry_idx
    ON letters (expires_at, id)
    WHERE recipient_id IS NULL
      AND withdrawn_at IS NULL;

CREATE INDEX letters_sender_idx
    ON letters (sender_id, created_at DESC, id DESC);

CREATE INDEX letters_recipient_idx
    ON letters (recipient_id, created_at DESC, id DESC)
    WHERE recipient_id IS NOT NULL;

CREATE INDEX reports_unreviewed_idx
    ON reports (created_at, id)
    WHERE reviewed_at IS NULL;

CREATE INDEX identities_inactive_idx
    ON identities (last_seen_at, id)
    WHERE deleted_at IS NULL;

CREATE INDEX auth_challenges_expiry_idx
    ON auth_challenges (expires_at, id);

CREATE INDEX access_sessions_identity_idx
    ON access_sessions (identity_id, expires_at);

CREATE INDEX access_sessions_expiry_idx
    ON access_sessions (expires_at, token_hash);

CREATE INDEX dpop_replays_expiry_idx
    ON dpop_replays (expires_at, session_token_hash);

CREATE INDEX reports_evidence_purge_idx
    ON reports (evidence_purge_at, id)
    WHERE evidence_purge_at IS NOT NULL
      AND evidence_purged_at IS NULL;

CREATE INDEX reports_record_purge_idx
    ON reports (record_purge_at, id)
    WHERE record_purge_at IS NOT NULL;

CREATE INDEX moderation_audit_retention_idx
    ON moderation_audit (purge_at, id);
```

No index is added without a named query. Index usage must be checked after real
traffic before adding more.

### Data invariants

- Letter IDs are valid 22-character unpadded base64url values.
- Reply and report IDs are valid 22-character unpadded base64url values and make
  first-write-wins retries independent of randomized ciphertext.
- Active aliases are unique, immutable, normalized, non-searchable, and absent
  from unauthenticated and unrelated API responses.
- Deleted alias comparison keys remain permanently reserved.
- Device public keys and thumbprints are immutable and never reused after
  deletion. Revocation hashes exist only for active identities.
- Auth challenges are single-purpose, single-use, and valid for five minutes.
- Access sessions are DPoP-bound to the identity key and valid for 15 minutes.
- A DPoP `jti` may appear only once per access session.
- The sender alias snapshot is immutable. The recipient alias follows claim
  expiry and reassignment until opening makes the assignment permanent.
- Original and reply bodies contain 1 to 2,000 Unicode code points and no more
  than 12 KiB of valid UTF-8.
- The sender and recipient differ.
- Before claim, `recipient_id`, `claimed_at`, and `claim_expires_at` are null.
  An unopened claim sets all three; opening clears only `claim_expires_at`.
- Claim expiry clears the recipient ID, recipient alias, claim timestamps, and
  recipient removal timestamp before the letter returns to waiting.
- `opened_at` requires a recipient.
- Reply ID, ciphertext, nonce, wrapped key, KMS key ID, encryption version, and
  `replied_at` are either all null or all present.
- A reply requires `opened_at`.
- A withdrawal requires no recipient and no open.
- An unclaimed letter is deleted after seven days.
- A participant may remove only their own keepsake access. The letter is purged
  after both participants remove it.
- Explicit deletion or one year without authenticated activity deletes identity
  access, waiting letters, blocks, and that participant's keepsake access.
- The sender may always reread their original while retaining access and may read
  the received reply. After opening, the recipient may reread the original and
  their sent reply while retaining access.
- Only the active recipient may open, reply to, report, or discard the received
  original. Only the sender may report the received reply.
- Everyone else receives `404`, not an ownership-revealing `403`.
- The claim operation excludes both directions of a blocked pair.
- Letter and reply plaintext is validated before encryption. SQL enforces the
  encrypted envelope shape without storing or inspecting plaintext.
- Evidence fields are all present until purge and all absent afterward.
- Report source and identity snapshots have no foreign keys and survive deletion
  of the letter or either identity until report retention expires.
- Disposition, closure, evidence purge, and report purge fields are set together
  on the first close, which requires `reviewed_at`.
- An `identity_disabled` close can delete only the report's snapshotted
  `reported_identity_id` and uses the normal deletion transaction.
- Moderation returns evidence only after an `authorized` audit row commits.
- Every participant mutation locks the caller's identity row first and rechecks
  `deleted_at` before changing letter state. Identity deletion takes the same
  lock, so no participant mutation can commit after deletion commits.
- Identity-scoped reads recheck the active caller in their query. A body read
  performs its KMS call outside a transaction, then uses a short transaction to
  lock the caller identity and letter in that order and recheck authorization
  and state before returning plaintext.

## Atomic letter claiming

Claiming is the highest-risk concurrency operation. It uses one short database
transaction and row locking.

```sql
SELECT id
FROM letters
WHERE recipient_id IS NULL
  AND withdrawn_at IS NULL
  AND expires_at > now()
  AND sender_id <> $1
  AND NOT EXISTS (SELECT 1 FROM reports WHERE letter_id = letters.id)
  AND NOT EXISTS (/* block in either direction */)
ORDER BY created_at, id
FOR UPDATE SKIP LOCKED
LIMIT 1;

UPDATE letters
SET recipient_id = $1,
    claimed_at = now(),
    claim_expires_at = now() + interval '24 hours'
WHERE id = $2;
```

Before selecting a new letter, the operation locks the identity row and returns
that identity's existing unexpired, unopened claim if one exists. This prevents
two concurrent requests from hoarding multiple letters without requiring a
second mutable queue.

Expired unopened claims are released in the same operation or by a small
scheduled cleanup query. Claims expire after 24 hours. Unclaimed letters expire
after seven days. There is no background in-memory queue. PostgreSQL is the only
source of truth.

Cleanup operations are bounded, idempotent database methods in this phase.
Production scheduling is configured with deployment operations rather than an
unmanaged goroutine in every server replica.

FIFO ordering is permanent product policy. Matching uses eligibility and queue
order only; it never ranks content, aliases, or participant behavior.

## Envelope encryption operations

Original and reply creation validate submitted plaintext. Report-evidence
creation decrypts the authorized source message. The service then asks KMS for
an `AES_256` data key under the allowed key ARN and exact encryption context,
encrypts with AES-256-GCM and a fresh 12-byte CSPRNG nonce, and persists only the
ciphertext, nonce, wrapped key, key ARN, and encryption version. Data keys are
never cached. Go cannot guarantee memory zeroization, so plaintext and unwrapped
keys stay in the smallest request scope and are never copied into logs, errors,
metrics, queues, or database parameters.

The fixed contexts are:

```text
original: service=orifude, schema=1, key_purpose=message,
          letter_id=<id>, part=original
reply:    service=orifude, schema=1, key_purpose=message,
          letter_id=<id>, part=reply, reply_id=<id>
evidence: service=orifude, schema=1, key_purpose=evidence,
          report_id=<id>, letter_id=<id>, target=original|reply,
          purpose=reported-content-review
message canary:  service=orifude, schema=1, operation=startup,
                 key_purpose=message
evidence canary: service=orifude, schema=1, operation=startup,
                 key_purpose=evidence, purpose=reported-content-review
```

AES-GCM additional authenticated data binds the record fields as
`orifude:v1:letter:<letter-id>:original`,
`orifude:v1:letter:<letter-id>:reply:<reply-id>`, or
`orifude:v1:evidence:<report-id>:<letter-id>:<target>:reported-content-review`.
IDs are fixed base64url and enum values are closed, so these encodings are
unambiguous. Decrypt compares the stored key ARN to the configured ARN, supplies
that ARN and the exact context to KMS, and then opens the ciphertext with the
exact additional data. The server derives all cryptographic metadata from
validated record fields; clients cannot provide key ARNs, versions, contexts,
nonces, or additional data.

`GenerateDataKey` must return the configured key ARN, exactly 32 plaintext bytes,
and a non-empty bounded ciphertext blob. `Decrypt` must return exactly 32 bytes.
Any other response fails before AES use or persistence.

External KMS calls never occur while holding a database transaction or row
lock. Send, reply, and report first look up their client ID and return an
existing result without KMS work. These lookups are authenticated and
identity-scoped; an ID owned by someone else returns the normal non-enumerating
error. On a miss, they prepare an encrypted envelope before opening a short
transaction that rechecks the ID, authorization, and state. Report creation
decrypts the source only on this miss path. Open first reads and
decrypts the claimed envelope, then locks and rechecks the claim before marking
it opened; plaintext is returned only after commit. If authorization changes,
KMS fails, or the transaction fails, plaintext is discarded and no transition
is committed. Unused generated envelopes are harmless because no wrapped key or
ciphertext was persisted.

Deleting the last participant copy removes the message envelopes. Evidence
purge removes its complete envelope from the live database. Backups and WAL may
retain deleted envelopes until their 30-day retention expires. A restored
database stays isolated, replays through the latest acknowledged commit with
continuous WAL, runs time-based cleanup, and passes a manual retention review
before serving traffic. A recovery point that predates a known deletion is never
promoted. If commit continuity cannot be proved, Orifude remains offline rather
than expose resurrected data or lose permanent alias reservations. This accepts
an availability failure after catastrophic database loss in preference to
reversing deletion or allowing impersonation.

## HTTP API

Participant API endpoints live under `/v1`; health and moderation endpoints use
the separate paths shown below. Requests and responses use JSON. Letter and reply
requests are limited to 16 KiB before decoding. Smaller endpoints use tighter
limits. Decoders reject unknown fields and trailing JSON values. All strings
receive explicit semantic, code-point, byte, and control-character validation
where applicable.

| Method and path | Purpose |
| --- | --- |
| `POST /v1/auth/challenges` | Create a five-minute registration or session challenge |
| `POST /v1/identities` | Prove the device key, create an alias, and exchange an alpha invite |
| `POST /v1/sessions` | Prove the device key and receive a 15-minute DPoP access token |
| `POST /v1/identities/revoke` | Delete an identity with its offline revocation credential |
| `GET /v1/me` | Validate identity and return limits plus the latest supported version |
| `DELETE /v1/me` | Permanently delete identity access and apply retention rules |
| `POST /v1/letters` | Release an idempotent client-ID letter |
| `POST /v1/letters/claim` | Return or atomically create one active claim |
| `GET /v1/letters/{id}` | Return role-safe metadata and authorized content |
| `POST /v1/letters/{id}/open` | Permanently assign and reveal a claimed body |
| `POST /v1/letters/{id}/reply` | Add the only allowed reply using a client reply ID |
| `POST /v1/letters/{id}/withdraw` | Withdraw an unclaimed sent letter |
| `POST /v1/letters/{id}/report` | Report the received original or reply using a client report ID |
| `POST /v1/letters/{id}/block` | Block future matching with its other party |
| `GET /v1/keepsakes` | Cursor-paginated sent and received exchanges |
| `DELETE /v1/keepsakes/{id}` | Remove the caller's access and purge after both remove it |
| `GET /healthz` | Process liveness only |
| `GET /readyz` | Database-backed readiness |
| `POST /moderation/v1/reports/next/claim` | Claim and return the oldest unreviewed encrypted report using a request ID |
| `POST /moderation/v1/reports/{id}/review` | Mark and return one encrypted report using a request ID |
| `POST /moderation/v1/reports/{id}/close` | Close one case with a fixed disposition and start retention |

Protected participant routes require `Authorization: DPoP <token>` and a `DPoP`
proof header. The server never accepts an identity ID, alias, access token
without its proof, or proof without its access token as identity evidence.

Challenge requests name a purpose and carry the public JWK. Session challenges
return the same shape whether or not the key belongs to an active identity;
session creation returns a generic authentication error after proof validation.
Revocation accepts only the high-entropy credential, always returns `204`, is
rate-limited, and performs the same deletion transaction as authenticated
`DELETE /v1/me`. Neither route can recover or replace a device key.

Registration and session proofs are carried in the `DPoP` header. A challenge
response contains a 22-character challenge ID, 32 random bytes as unpadded
base64url `nonce`, `expires_in: 300`, and server time. Identity creation carries
the challenge ID, alias, invite when required, and 32-byte revocation hash in
its JSON body. Its proof includes the challenge nonce and no `ath`. The server
commits challenge consumption, invite redemption, alias reservation, identity
creation, and the initial access session in one transaction. Session creation
carries only its challenge ID; its proof follows the same nonce rule. Both
successful responses return `token_type: "DPoP"`, an opaque access token, and
`expires_in: 900`.

A key-proven registration retry is idempotent. If the active identity has the
same public key, alias, revocation hash, and invite redeemed by that identity,
the server consumes the new challenge and returns a fresh session. For public
registration, the other three fields must match. Any mismatch returns a generic
conflict and never changes the existing identity.

Every proof is at most 8 KiB. Its JOSE header contains only `typ=dpop+jwt`,
`alg=ES256`, and a public JWK with `kty=EC`, `crv=P-256`, `x`, and `y`; private
`d` and remote-key fields such as `jku`, `x5u`, and `x5c` are rejected. Duplicate
`Authorization` or `DPoP` headers are rejected. `htm` is the uppercase request
method. Per RFC 9449, `htu` is the configured `PUBLIC_ORIGIN` scheme and
authority plus the exact escaped request path after trusted proxy routing;
query and fragment are excluded. `jti` is 16 to 128 printable ASCII characters,
and `iat` is within 30 seconds of server time. Challenge proofs require the
issued `nonce`. Resource proofs require `ath` as the unpadded base64url SHA-256
of the ASCII access token, omit nonce, and match the session thumbprint. The
replay insert commits in its own short transaction before the handler runs;
conflict or database failure denies the request even if the later business
operation fails.

### Chi router and middleware

Chi remains a thin router around standard `net/http`. Middleware order is part
of the HTTP contract and receives an integration test.

```text
request ID
trusted proxy IP handling
panic recovery
structured access log
security response headers
route-specific timeout
route-specific body limit
DPoP session authentication for protected /v1 routes
handler
```

Authentication and body limits are mounted at the narrowest route group that
needs them. Forwarded client IP headers are ignored by default. They are trusted
only when the remote peer belongs to an explicitly configured trusted-proxy
CIDR and the deployment proxy strips user-supplied values. The service does not
enable broad CORS because the TUI is the only participant API client and the
landing page never calls it.
Moderation routes have a separate chain that validates the Cloudflare Access
JWT, requires `X-Orifude-Moderation: reported-content-review`, and accepts no
CORS preflight. Requests whose scheme and authority do not match
`MODERATION_ORIGIN` are rejected. The non-simple header prevents a cross-site
form from using an ambient Access browser session. Evidence handlers read and
buffer one envelope and commit the authorization decision before returning
ciphertext. Case close sets a fixed disposition, `closed_at`, 90-day evidence
purge time, and one-year report purge time atomically. It never uses participant
authentication.
Cloudflare Access JWTs come only from `Cf-Access-Jwt-Assertion`. Validation
allows only `RS256`, checks signature, `type=app`, `iss`, application `aud`,
`iat`, `exp`, `nbf` when present, and non-empty `sub`, and obtains rotating keys
from the team-domain Access certs endpoint.
Keys are cached for at most one hour and refreshed once for an unknown `kid`;
fetch or validation failure denies access. Cloudflare Access records requests it
rejects before the origin. After a JWT has a verified operator subject, allowed
and denied decisions are written to the database when it is available; audit
failure always denies evidence release.

### sqlc query rules

- Every query has a sqlc name and one declared cardinality.
- Query files are grouped by domain operation, not by generated filename.
- The claim transaction uses generated lock, select, release, and update queries.
- `internal/postoffice` starts the transaction and passes
  `dbgen.New(tx)` or `Queries.WithTx(tx)` to the operation.
- Generated models do not double as public JSON response types.
- Nullable database values are translated at the application boundary rather
  than leaking pgx types into the TUI protocol.
- CI runs sqlc generation and rejects uncommitted output drift.
- Released Goose migrations are immutable. Every later schema change gets a new
  migration with reviewed locking and rollback behavior.

### API error shape

```json
{
  "error": {
    "code": "letter_already_replied",
    "message": "This letter already has a reply."
  }
}
```

Codes are stable and machine-readable. Messages may improve without becoming a
client contract. Internal database and network details are never returned.

### Network behavior

- The TUI uses one reused `http.Client` and transport.
- TLS is mandatory outside local development.
- The API client refuses redirects so authorization and DPoP headers never move
  to another URL. The server does not redirect API routes.
- Requests have end-to-end deadlines.
- Response bodies are always closed and size-limited.
- Challenge, identity-creation, session-creation, protected participant, and
  moderation responses set `Cache-Control: no-store`.
- Automatic retries apply only to safe reads or explicitly idempotent writes.
- Identity creation, letter creation, claim, open, reply, report, block,
  withdraw, review claim, and case close are idempotent at the server.
- Letter, reply, and report retries reuse their client ID but create a fresh DPoP
  proof ID. The first committed payload wins; a successful retry returns that
  result without decrypting or comparing randomized ciphertext. A different
  reply or report ID after the one allowed write returns a conflict.
- Exponential retry machinery is not planned. The person chooses to retry after
  a bounded failure.

## Authentication, alias, and local identity

The durable identity credential is one P-256 device key that Orifude never
exports. A challenge proof creates an opaque 15-minute access session bound to
that same key. Possessing an access token without the key is insufficient. The
TUI silently creates a fresh session while the key remains available.

The TUI first stores a PKCS#8-encoded private key through the operating system
credential store. If no credential store is available, it asks before using
`os.UserConfigDir()/orifude/identity.json`; the directory is mode `0700`, the
file is mode `0600`, writes use create-or-replace plus atomic rename, and loads
reject symlinks, non-regular files, wrong ownership, and broader permissions on
Unix. The fallback is unavailable where Orifude cannot verify owner-only access.
The limitation is explained before identity creation. The alias and public
thumbprint may remain in ordinary local configuration.

Private keys, access tokens, and revocation credentials are never accepted
through CLI flags, included in URLs, sent to the landing page, or logged. The
access token stays in memory. The revocation credential is displayed once and
never written by Orifude. There is no recovery, account linking, key export, or
multi-device identity. Losing the device key is permanent; from another device,
the separate revocation flow can accept the offline credential only to delete
the inaccessible identity.

The alias is chosen once during identity creation. It is globally unique but
visible only to matched participants. Validation uses a pinned Unicode version,
NFC normalization, and the Unicode TR39 confusable skeleton. Aliases contain 2
to 24 code points, use one script except for Japanese Han, Hiragana, and Katakana
combinations, and allow ASCII digits, single spaces, hyphens, and underscores.
Controls, invisible formatting characters, emoji, and other script mixing are
rejected. Alias availability is checked only during identity creation; there is
no lookup or search endpoint.

Explicit or revocation-credential deletion immediately marks the identity
deleted, revokes every access session, deletes waiting letters and blocks, and
removes that participant's keepsake access in one transaction. Shared keepsakes
remain for the other participant until they also remove them. The same cleanup
runs after one year without an authenticated request. Minimal alias and device
key reservations remain permanently so neither can impersonate a deleted
identity.

## Security boundaries

The design protects correspondence bodies from a PostgreSQL dump, database
backup leak, read-only database operator, and accidental plaintext query. DPoP
protects a stolen access token that is not accompanied by the device key, and
the replay table limits captured proofs to one use. Separate evidence-key
permissions prevent the Railway runtime and ordinary database access from
reading retained moderation evidence.

It does not protect plaintext already displayed to a participant, a stolen
device key, traffic and relationship metadata, a compromised post-office
process while it has message-decrypt permission, or an authorized moderator who
copies evidence after review. An AWS account or KMS administrator who changes
key policy also remains trusted and externally audited. Envelope authentication
covers the bound fields, not every authorization column. An active database
writer could alter unbound sender, recipient, report reason, or state metadata.
Database write access therefore remains tightly restricted and privileged. The
privacy notice must state these limits. Reports, authorization, short sessions,
least-privilege KMS roles, and audit trails reduce these risks; they do not make
the system end-to-end encrypted.

KMS administration is separate from Railway runtime and moderator roles. Key
policies deny those roles policy changes, grants, disabling, scheduling
deletion, and alias changes. Scheduling key deletion uses AWS's 30-day waiting
period and raises an immediate alert. Annual automatic rotation is enabled;
on-demand rotation and credential revocation follow a suspected access incident.

## Safety, privacy, and abuse controls

Pseudonymous messages are a hostile-input boundary. The initial service must be an
invite-only alpha until moderation and operational response exist.

### Required before alpha

- Server-side authorization on every letter operation
- Device-key challenge authentication and DPoP replay rejection
- Fixed body and request-size limits
- Per-identity send and claim limits
- Deployment-edge IP rate limiting
- Report and block actions in the TUI
- Report-only encrypted evidence and an audited moderation review path
- No rendering of terminal escape sequences from letter bodies
- No logging of plaintext, invite codes, authentication material, or key material
- No production pprof or debug endpoint, and process core dumps disabled
- Application envelope encryption with externally held message and evidence KMS
  keys
- Database backups and a tested restore procedure
- A privacy statement that explains service-side decryption and its limits

All user text must be treated as text. Control characters other than newline
are rejected or visibly escaped before terminal rendering. This prevents a
letter from injecting terminal commands, hyperlinks, cursor controls, or fake
Orifude UI.

### Privacy position

- Transport is encrypted with TLS.
- The managed database and its backups provide platform encryption at rest in
  addition to application envelope encryption.
- PostgreSQL stores letter, reply, and report evidence ciphertext. It never
  stores their plaintext or unwrapped data keys.
- The post office decrypts letter and reply bodies only for an authorized
  participant request. It decrypts reported content only while creating retained
  evidence and cannot decrypt that evidence afterward.
- Orifude is not end-to-end encrypted and must not imply otherwise.
- Aliases are visible only to matched participants and service operators. They
  are never searchable or attached to a public history.
- Database access still reveals operational metadata, pseudonymous aliases,
  timing, and participant relationships. Application encryption protects
  correspondence content, not all metadata.
- A compromise of the running post office or its AWS runtime credential can
  decrypt ordinary messages while authorized KMS access remains available. KMS
  separation protects database and backup leaks and casual database access; it
  is not a claim that a compromised application is harmless.
- The supported evidence retrieval path requires Cloudflare Access. Every
  decrypt, including one using an envelope obtained from a database copy,
  requires a federated human AWS role. Correlated CloudTrail role and decrypt
  events record the human principal and fixed context.
- Live report evidence is deleted 90 days after case closure. Recovery copies
  expire no more than 30 days later. Restores remain isolated for retention
  review before serving traffic.
- Live report metadata is deleted one year after closure, and each live
  moderation audit event is deleted after one year. Recovery copies expire
  within 30 more days.
- Waiting letters expire after seven days; unopened claims expire after 24
  hours; completed keepsakes remain until both participants remove them.
- Public analytics are absent by default on the landing page.
- Logs use internal IDs only and never contain message content or credentials.

### Initial limits

Letter and reply bodies allow at most 2,000 Unicode code points and 12 KiB of
valid UTF-8. Their complete JSON request bodies allow at most 16 KiB. Other
operational numbers remain configurable. During private alpha, an identity may
hold only one unopened claim. Each successful new claim starts a 15-minute
cooldown and an identity may receive at most three new claims per hour and eight
per day. Returning an existing active claim does not consume another successful
claim allowance. Successful per-identity limited operations write durable
`rate_limit_events` in their state transaction. Claim requests, reports,
identity creation attempts, letters per hour, deployment-edge IP traffic, and
database pool size start with conservative limits and change from observed
traffic and abuse.

## Visual system

The supplied artwork in `/home/nuggocto/Pictures/Orifude` is the source visual
reference:

- `Orifude-logo.png`: squirrel courier, folded boat, branch, berries, brush enso
- `Orifude-icon.png`: brush enso and folded leaf
- `Orifude-watermark.png`: wordmark and folded leaf
- Monochrome variants: high-contrast and ANSI references

### Visual principles

- Quiet space is part of the design. Do not fill every terminal cell.
- Ink strokes should feel irregular, but controls and text remain crisp.
- Folding is represented by geometry and motion, not generic loading spinners.
- The squirrel appears for meaningful delivery moments, not as decoration on
  every screen.
- Product language uses branch, fold, release, carry, unfold, reply, keepsake,
  and burn consistently.
- Japanese visual references stay restrained. Avoid random kanji, torii gates,
  cherry-blossom wallpaper, or claims of cultural authenticity.

### Palette

| Token | Truecolor starting point | Use |
| --- | --- | --- |
| Ink | `#292823` | Primary text and strokes |
| Washi | `#F1ECE1` | Light background and paper |
| Moss | `#858A72` | Active controls and living leaves |
| Clay | `#A48B68` | Secondary accents and berries |
| Branch | `#62594D` | Borders and structure |
| Ash | `#A59E91` | Muted text |
| Ember | `#A45B52` | Destructive and report actions |

Lip Gloss v2 downsampling provides ANSI 256, ANSI 16, and monochrome fallbacks.
Every status also has a word or symbol; color is never the only distinction.
Styles adapt after Bubble Tea reports whether the terminal background is dark.

### Responsive terminal layout

| Terminal | Layout |
| --- | --- |
| At least 100x30 | Branch artwork beside the active panel |
| 72x24 to 99x29 | Compact artwork above a single panel |
| 56x18 to 71x23 | Text-first layout with reduced decoration |
| Below 56x18 | Clear resize message without corrupted controls |

The TUI must support Unicode input and display, but its structural borders have
an ASCII fallback. It must not depend on Kitty, Sixel, iTerm images, a Nerd Font,
or a particular terminal emulator.

### Motion

- Fold and unfold animations use a small fixed set of frames.
- Frames do not exceed roughly one second for a complete transition.
- Reduced motion skips directly to the final frame.
- Network latency never stretches a decorative animation indefinitely.
- A spinner may indicate waiting after the fixed animation completes.

## Landing-page design

`orifude-front` is a separate Astro project that builds to static files and has
no authenticated state. Its canonical production URL is `https://orifude.com`.

```text
Landing page
|
+-- Hero
|   +-- Full Orifude watermark
|   +-- Tagline
|   +-- Download button
|
+-- Terminal recording
|   +-- Compose
|   +-- Fold
|   +-- Delivery
|   +-- Unfold
|
+-- How it works
+-- Product principles
+-- Privacy and safety
+-- Platform downloads and checksums
+-- FAQ
+-- Source and project links
```

### Frontend implementation

- Astro components own document structure and keep browser JavaScript near zero.
- TypeScript uses Astro's strict configuration.
- Tailwind CSS 4 is installed through `@tailwindcss/vite`, not the legacy
  `@astrojs/tailwind` integration.
- Design tokens for the Orifude palette live in `src/styles/global.css` and are
  reused through Tailwind utilities.
- Zod validates checked-in release metadata and any build-time environment
  values. Invalid download URLs or missing checksums fail the build.
- Ky is added only if a real remote request is introduced. If release metadata
  stays checked in, native build inputs are simpler and Ky remains absent.
- Oxlint checks JavaScript, TypeScript, and script blocks in `.astro` files.
- `astro check` remains responsible for Astro templates and TypeScript because
  Oxlint does not replace Astro's compiler checks.
- Oxfmt formats Astro, TypeScript, CSS, JSON, YAML, and Markdown. Prettier and
  ESLint are not installed alongside Oxc without a proven unsupported rule.
- pnpm is the sole package manager. `packageManager` pins its version and
  `pnpm-lock.yaml` is committed.
- Astro's Vite build emits `dist`; no server adapter is needed for a static site.

Expected package scripts:

```json
{
  "scripts": {
    "dev": "astro dev",
    "build": "astro check && astro build",
    "preview": "astro preview",
    "lint": "oxlint",
    "lint:fix": "oxlint --fix",
    "fmt": "oxfmt",
    "fmt:check": "oxfmt --check",
    "check": "pnpm fmt:check && pnpm lint && astro check && astro build"
  }
}
```

### Cloudflare deployment

Cloudflare Pages hosts the generated `dist` directory. The Pages project connects
to the `orifude-front` GitHub repository, runs `pnpm build`, publishes `dist`, and
creates preview deployments for pull requests. No Worker script or Wrangler
configuration is needed for the static site.

The Pages project attaches `orifude.com` as its custom domain and redirects
`www.orifude.com` to the apex. Static `_headers` and `_redirects` files keep the
security policy and redirect behavior in version control.

The landing deployment has no post-office secret, database binding, or API
token. Browser security headers should include a restrictive content security
policy, `X-Content-Type-Options: nosniff`, a strict referrer policy, and framing
protection. External scripts and analytics remain absent by default.

The page uses the actual supplied logo, icon, watermark, and monochrome assets.
Its visual language is warm washi, muted moss and clay, broad negative space,
and one ink branch that guides the page vertically. It should not look like a
SaaS dashboard or contain fake application panels.

Download links point to immutable release artifacts and checksums. Installation
instructions may describe launching the binary, but the product does not grow
CLI subcommands for setup or use.

## Release distribution

The 1.0 release matrix is Linux, macOS, and Windows on amd64 and arm64.
GoReleaser builds archives, a Windows zip, and one checksum file from tags after
`cmd/orifude` exists. Until that entrypoint builds, the repository does not carry
a speculative GoReleaser configuration.

GitHub Releases is the source of immutable binaries and checksums. GoReleaser
publishes package metadata to `nuggocto/homebrew-tap` and
`nuggocto/scoop-bucket`, both on their `shrek` branches. AUR publication uses its
separate SSH-backed package repository once its package name and repository are
created. Simple POSIX shell and PowerShell installers select the current OS and
architecture, download a pinned release from GitHub, and verify its checksum
before installation.

Version 1.0 is not published until GitHub archives, both installers, Homebrew,
Scoop, and AUR all install working artifacts. SHA-256 checksums are mandatory.
Artifact signatures and attestations are not part of the release contract.

## Configuration

### TUI configuration

| Setting | Source | Default |
| --- | --- | --- |
| API base URL | Built-in release value, environment override for development | Production service URL |
| Device private key | OS credential store, approved owner-only file fallback | Created during onboarding |
| Alias and public thumbprint | Local config file | Created during onboarding |
| Access token | Process memory only | Fresh 15-minute session |
| Reduced motion | TUI settings | Auto-detect where possible, otherwise off |
| Theme | TUI settings | Terminal background adaptive |
| Debug logging | Development environment only | Disabled |

### Server configuration

| Setting | Purpose |
| --- | --- |
| `DATABASE_URL` | PostgreSQL connection string |
| `LISTEN_ADDR` | HTTP listen address |
| `PUBLIC_ORIGIN` | Exact origin used to validate DPoP `htu` |
| `MODERATION_ORIGIN` | Exact Access-protected origin accepted for moderation routes |
| `TRUSTED_PROXY_CIDRS` | Optional comma-separated deployment-proxy CIDRs allowed to supply forwarded scheme, host, and client IP; required when TLS terminates before the service |
| `AWS_REGION` | Region containing both KMS keys |
| `MESSAGE_KMS_KEY_ARN` | Allowed message-key ARN |
| `EVIDENCE_KMS_KEY_ARN` | Allowed moderation-key ARN; must differ from message key |
| `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` | Restricted runtime IAM access key supplied by Railway secrets |
| `CF_ACCESS_ISSUER` | Exact Cloudflare Access team-domain issuer |
| `CF_ACCESS_AUDIENCE` | Audience for the moderation application |
| Invite administration credential | Issue and revoke private-alpha invites |
| `LATEST_TUI_VERSION` | Passive update notice returned by the post office |
| `SEND_PER_HOUR` | Per-identity hourly send limit; zero disables it |
| `CLAIM_COOLDOWN_SECONDS` | Per-identity delay between claims; zero disables it |
| `CLAIM_PER_HOUR`, `CLAIM_PER_DAY` | Per-identity claim limits; zero disables either limit |
| `REPORT_PER_DAY` | Per-identity daily report limit; zero disables it |
| `RATE_EVENT_RETENTION_SECONDS` | Positive retention window at least as long as every enabled cooldown, hourly limit, and daily limit |
| `LOG_LEVEL` | Structured log threshold |

Secrets are injected by the deployment platform. They are never committed,
placed in build arguments, printed at startup, or exposed through readiness
endpoints. The runtime AWS access key is dedicated to Orifude, rotated at least
every 90 days, and denied all IAM and KMS administration. Before accepting
traffic, startup uses a fixed canary context to run message `GenerateDataKey`
and `Decrypt` plus evidence `GenerateDataKey`, verifies returned key ARNs and
that the decrypted canary key equals the generated message key, and discards
every canary value without persistence. This tests effective key policy, not
only `DescribeKey` metadata. Key policies allow only their documented runtime
and startup-canary contexts.

### Moderator configuration

The internal tool has no static AWS credential. Operators authenticate their
named IAM Identity Center profile, then the tool assumes `MODERATOR_ROLE_ARN`
with `DurationSeconds=900` and a unique report-bound role session name. It calls
`GetCallerIdentity` first so the CloudTrail `AssumeRole` event records the base
human session. The tool requires `AWS_REGION`, the exact
`EVIDENCE_KMS_KEY_ARN`, and the fixed purpose `reported-content-review`.
Operators retrieve one envelope from the Access-protected moderation origin with
`cloudflared access curl`, including the required moderation header, and pipe it
to the tool. The tool rejects a key ARN, context, report ID, or target that does
not match the envelope metadata.

## Failure behavior

- Offline startup opens the TUI in a clear disconnected state with local help
  and settings available.
- A draft survives screen navigation within the running process.
- If the credential store is unavailable on first run, the TUI explains the
  owner-only file fallback and requires confirmation. It never silently weakens
  storage.
- A missing, malformed, or inaccessible device key stops authentication with a
  recovery explanation. The TUI never silently creates a replacement identity.
- An expired access token triggers one new signed challenge and session attempt.
  A rejected proof or deleted identity returns to the unrecoverable-identity
  screen rather than looping.
- A device clock outside the 30-second DPoP window produces a clock-correction
  message and does not weaken proof validation.
- The TUI shows and confirms the revocation credential before registration. If
  the registration response is lost, it keeps that credential in memory and
  tries a session challenge with the same key. A successful session proves the
  identity exists; otherwise it retries registration with the same key,
  credential hash, alias, and invite.
- An exit after an ambiguous registration keeps the pending device key. On
  restart, the TUI does not enter the branch until a session challenge confirms
  that identity creation completed. An unavailable post office leaves the TUI
  on a recovery screen with retry and delete-only credential actions.
- A release timeout asks the server about the same client-generated letter ID
  before offering another submission.
- A lost claim response returns the identity's existing active claim on retry.
- An expired unopened claim disappears with an explanation rather than showing
  stale content.
- The TUI shows a non-blocking notice when the post office reports a newer
  supported version. It never downloads or installs an update.
- Server errors preserve the current screen and user text.
- Unauthorized identity errors explain that the identity cannot be recovered;
  the offline revocation credential remains delete-only.
- KMS failure returns `503` for send, open, body read, reply, and report while
  preserving local drafts and server state. Open is not committed before its
  body decrypt succeeds, and no path falls back to plaintext storage.
- Unknown encryption versions, unexpected key ARNs, authentication-tag failure,
  and encryption-context mismatch fail closed and emit a content-free security
  event.
- Evidence is not released if Cloudflare Access validation or the database audit
  write fails. Failure of local AWS SSO or KMS decrypt leaves only ciphertext on
  the moderator machine.
- A startup KMS canary failure prevents the instance from becoming ready. A KMS
  outage after startup leaves readiness database-backed but returns `503` only
  from operations that need encryption or decryption.
- Database unavailability fails readiness so new traffic stops reaching the
  instance.
- Graceful shutdown stops new requests, drains admitted requests for a bounded
  period, and closes the database pool.

## Testing strategy

Tests protect behavior and state transitions, not rendered implementation
details.

### Unit tests

- TUI screen transitions from typed Bubble Tea messages
- Keyboard navigation while in navigation mode
- Printable keyboard behavior while a textarea is focused
- Fold output is deterministic for a given seed and terminal size
- Input validation accepts 2,000 code points and rejects 2,001, oversized UTF-8,
  invalid UTF-8, and unsafe control text
- Alias validation covers normalization, uniqueness, supported scripts,
  confusables, invisible characters, immutability, and permanent reservation
- P-256 JWK validation rejects every other curve and signing algorithm
- DPoP validation covers method, canonical URI, access-token hash, issue time,
  proof ID, key thumbprint, duplicate headers, private or remote JWK fields, and
  malformed claims
- AES-GCM rejects altered ciphertext, nonce, additional data, and wrong keys
- Encryption context generation is stable for original, reply, and evidence
- Local identity storage covers keyring success, explicit fallback confirmation,
  owner and mode checks, symlink rejection, atomic replacement, and proof that
  access and revocation credentials are never serialized
- API error codes map to the correct visible state

### Integration tests

- Migrations apply to an empty disposable PostgreSQL database
- Goose follows the documented migration and rollback policy
- sqlc regeneration produces no uncommitted generated-code drift
- Two simultaneous claim requests cannot receive the same letter
- A sender cannot claim their own letter
- A blocked pair is never matched
- Unauthorized identities cannot infer or read a letter
- Identity and letter creation, claim, open, reply, report, block, withdraw,
  moderation review claim, and moderation close are idempotent
- An expired unopened claim can return to the queue
- Waiting letters expire after seven days
- Single-use invites expire after seven days and cannot be redeemed twice
- Registration and session challenges expire, have one purpose, and cannot be
  consumed twice
- A lost registration response can retry the same key, alias, revocation hash,
  and invite; any changed field conflicts without changing the identity
- An access token without its device proof and a device proof without its access
  token are both rejected
- DPoP proof IDs cannot be replayed, including across concurrent requests
- Access sessions expire after 15 minutes and deletion invalidates them
- A revocation credential can delete but cannot create a session or read data
- Concurrent authenticated or moderator deletion is linearized on the identity
  lock. When deletion wins, in-flight send, open, body read, reply, and report
  cannot commit or return plaintext
- Identity inactivity and explicit deletion apply the same cleanup
- One participant deleting a keepsake does not remove the other's copy; both
  deletions purge it
- An active report survives participant and letter deletion
- Concurrent next-review claims select different reports, while direct review
  sets `reviewed_at` and retries its request ID against the same report
- Review and purge lock the same row; evidence at or past its purge time is never
  returned even when cleanup has not run
- Case close sets disposition and retention once; live evidence is purged after
  90 days, report metadata after one year, and audit rows one year after creation
- `identity_disabled` can delete only the reported identity and cannot accept a
  client-selected target
- PostgreSQL never receives known letter, reply, or evidence plaintext in an
  end-to-end API journey
- Reply retries use the client reply ID and do not compare randomized ciphertext
- Only the receiver of the selected original or reply can report it
- Access JWT tests cover algorithm, app type, issuer, audience, issued-at,
  expiry, optional not-before, subject, unknown-key refresh, duplicate headers,
  cert-fetch failure, the moderation header, denied CORS preflight, and
  `MODERATION_ORIGIN` mismatch
- Moderation returns only one report's encrypted evidence after an authorization
  audit commits; a denied decision is recorded when possible and audit failure
  denies release
- KMS tests reject wrong configured or returned key ARNs, prove the runtime
  cannot decrypt evidence, and cover both startup-canary contexts and failure
- KMS outage and envelope tampering cannot expose body plaintext or commit a
  send, open, reply, or report
- Restore tests remove time-expired data, and the recovery runbook refuses
  promotion for a point before a known deletion or without commit continuity
- The full HTTP router enforces body limits, content types, redirect refusal,
  and `Cache-Control: no-store` on every credential-bearing response

### End-to-end tests

Keep this suite small:

1. Start the production HTTP handler against a disposable database and a
   synthetic in-memory KMS caller.
2. Create two device keys and synthetic identities with unique aliases through
   the real challenge and DPoP API.
3. Send, claim, unfold, reply, and read the completed keepsake.
4. Start the shipped TUI in a controlled pseudo-terminal for one critical
   onboarding and compose journey.

No test contacts production or another person's data. Tests use readiness
signals, not sleeps.

### Required verification

```text
sqlc generate
git diff --exit-code -- internal/database/dbgen
gofmt on touched Go files
go test ./...
go test -race ./...
go vet ./...
govulncheck ./... before releases

pnpm fmt:check
pnpm lint
pnpm astro check
pnpm build
```

Landing-page verification includes keyboard navigation, mobile and desktop
layouts, asset sizes, broken links, reduced motion, download checksum accuracy,
and a Lighthouse accessibility check. A deployment smoke test verifies
`https://orifude.com`, the `www` redirect, TLS, security headers, and one release
download without touching the post-office API.

## Observability

The post office emits structured JSON logs with stable keys such as
`request_id`, `route`, `status`, `duration_ms`, and internal `identity_id` when
authenticated.

It never logs authorization or DPoP headers, invite codes, revocation
credentials, request bodies, plaintext, ciphertext, wrapped keys, KMS responses,
Cloudflare Access JWTs, or raw database errors returned to clients.

Operation relies on Railway HTTP metrics, PostgreSQL metrics, readiness, and
structured logs. Prometheus, tracing, or another metrics dependency is added
only if those sources cannot measure a published service objective or diagnose
an incident.

Events worth counting without content:

- Identities created
- Identities deleted by request or inactivity
- Letters released
- Claims created and expired
- Letters opened
- Replies created
- Reports created
- Authorization failures
- DPoP replay and clock-skew rejections
- KMS operation failures by operation and key purpose, never key material
- Moderation evidence releases and denials by report and opaque operator subject
- Rate-limit rejections
- Database pool acquire waits

## Delivery phases

Check a task only after its behavior and verification are complete. A created
file, passing compile, or mocked happy path does not finish a task whose stated
behavior is broader. Each phase closes only when its "Done when" condition is
met.

### Phase 0: foundations and decisions

- [x] Approve the TUI as the only participant client and keep the website
  presentation-only.
- [x] Record `orifude.com` as the Cloudflare-managed public domain.
- [x] Write the initial product, architecture, data, security, design, and
  delivery specification in `PROJECT.md`.
- [x] Remove the abandoned browser-application scaffold from `orifude` while
  preserving this project document and approved artwork.
- [x] Initialize or confirm the `orifude` Go module and repository.
- [x] Reserve the sibling `orifude-front` repository for later landing-page
  work.
- [x] License source code, documentation, and project-owned artwork under
  Apache-2.0.
- [x] Support Linux, macOS, and Windows on amd64 and arm64 for the first release.
- [x] Fix identity, alias, retention, claim, invite, hosting, update, and release
  policy in this document.
- [x] Fix device-key authentication, DPoP sessions, envelope encryption, KMS
  separation, and report-only moderation policy in this document.
- [x] Pin the current supported Go toolchain and direct Go dependencies.
- [x] Pin sqlc, Goose, and govulncheck as Go project tools.
- [x] Create the root `sql/queries` and `sql/migrations` directories.
- [x] Create `sqlc.yaml` targeting `internal/database/dbgen` with pgx v5.
- [x] Add baseline CI jobs for Go checks and generated-code drift without
  requiring production secrets.
- [x] Document Go and post-office development prerequisites and environment
  variables.

Done when `orifude` installs from a clean checkout, its baseline Go and generated
checks pass, sqlc and Goose paths are fixed, the reserved frontend repository
exists, and no blocking product decision remains for the prototype or schema.

### Phase 1: offline TUI interaction prototype

- [x] Add Bubble Tea v2, Bubbles v2, Lip Gloss v2, and Huh v2.
- [x] Implement one root model with explicit `Screen` and `InputMode` enums.
- [x] Build the splash, first-run, branch, compose, fold preview, delivery,
  unfold, read, reply, keepsake, report, and settings screens using synthetic
  fixtures.
- [x] Add unique multilingual alias creation and the permanent no-recovery
  warning to onboarding.
- [x] Implement wide, compact, text-first, and too-small terminal layouts.
- [x] Translate the monochrome Orifude mark into ANSI and ASCII-safe artwork.
- [x] Implement the truecolor palette with ANSI 256, ANSI 16, monochrome, and
  light/dark background behavior.
- [x] Implement deterministic fold shapes and fixed-frame fold/unfold animation.
- [x] Add reduced-motion behavior that skips decorative frames.
- [x] Accept letters and replies up to 2,000 Unicode code points and 12 KiB.
- [x] Reject or visibly neutralize unsafe terminal control characters.
- [x] Add consistent `b` back navigation plus `j`, `k`, `l`, `g g`, `G`,
  half-page, and full-page navigation outside text-input mode.
- [x] Make `i` focus the composer and `esc` return to navigation mode.
- [x] Ensure printable keys, including `q`, remain text while an editor is
  focused.
- [x] Integrate Huh for onboarding, confirmations, settings, and report reasons.
- [x] Expose Huh accessible mode and ensure important state is never color-only.
- [x] Add contextual Bubbles help for active keyboard actions.
- [x] Add focused tests for navigation modes, size changes, input boundaries,
  alias rules, control text, and deterministic folds.
- [x] Create a deterministic VHS tape for the core compose-to-unfold journey.

The prototype may use immutable synthetic fixtures, but fixture behavior must
not become a local database or a second post office.

Done when a person can complete the entire simulated journey with the keyboard
at every supported terminal size, all TUI tests pass, and the VHS recording can
be regenerated from a clean checkout.

### Phase 2: PostgreSQL and post-office API

- [x] Write `sql/migrations/00001_initial.sql` with Goose Up and Down sections
  for identities, auth challenges, access sessions, DPoP replays, alias
  reservations, invites, letters, blocks, reports, moderation audit, constraints,
  and named indexes.
- [x] Verify the migration applies to an empty PostgreSQL database.
- [x] Document whether production rollback uses Goose Down or a forward repair
  migration for each released schema change.
- [x] Write named sqlc queries under `sql/queries` for identities, letters,
  sessions, DPoP replays, claims, keepsakes, blocks, reports, and moderation
  audit.
- [x] Generate and commit package `dbgen` under `internal/database/dbgen`.
- [x] Add CI drift detection for generated sqlc files.
- [x] Implement pgx pool startup, ping, readiness, sizing, and shutdown in
  `internal/database`.
- [x] Implement transaction ownership in `internal/postoffice` using generated
  sqlc queries bound to pgx transactions.
- [x] Implement P-256 registration and session challenges, DPoP-bound 15-minute
  access sessions, replay rejection, delete-only revocation credentials, alias
  reservation, invite redemption, and last-seen updates.
- [x] Implement AES-256-GCM envelope encryption with fresh KMS data keys,
  deterministic context, key-ARN allowlists, and no plaintext persistence.
- [x] Implement letter creation with client-generated idempotent IDs.
- [x] Implement atomic claim reuse and assignment with identity locking and
  `FOR UPDATE SKIP LOCKED`.
- [x] Implement claim expiry and safe requeue of unopened letters.
- [x] Implement seven-day waiting expiry, one-year identity inactivity cleanup,
  participant keepsake deletion, 90-day evidence cleanup, one-year report
  cleanup, and one-year moderation-audit cleanup.
- [x] Implement authorized open, reply, withdraw, report, block, and keepsake
  operations.
- [x] Make reports self-contained across letter and identity deletion and add
  audited review-claim and idempotent close moderation operations.
- [x] Implement the Chi route tree, middleware order, route-specific body
  limits, DPoP authentication, Cloudflare Access validation, and stable error
  responses.
- [x] Configure explicit HTTP server timeouts and bounded graceful shutdown.
- [x] Enforce 2,000 code points, 12 KiB text, 16 KiB JSON, valid UTF-8, and
  control-character rules before encryption, plus encrypted-envelope constraints
  in PostgreSQL.
- [x] Return `404` for unauthorized letter access without leaking ownership.
- [x] Add per-identity limits with configuration suitable for private alpha.
- [x] Add structured slog output with credential and content redaction.
- [x] Add unit tests for validation, authorization, and state transitions.
- [x] Add PostgreSQL integration tests for every sqlc query and migration.
- [x] Add concurrent claim tests that prove one letter reaches one recipient.
- [x] Add router tests for content types, limits, DPoP and Access authentication,
  timeouts, replay rejection, and idempotent retries.
- [x] Add one API end-to-end test covering identity, send, claim, open, reply,
  keepsakes, report, block, review, close, and both deletion paths with synthetic
  data.

Done when the production HTTP handler and a real disposable PostgreSQL database
pass the complete API journey, the post-office binary passes startup and
shutdown smoke tests, concurrent claim tests cannot duplicate
delivery, DPoP replays fail, known plaintext never reaches PostgreSQL,
authorization audit gates evidence release, migration and sqlc drift checks
pass, and logs contain neither test plaintext nor authentication or key material.

### Phase 3: online TUI integration

- [x] Replace synthetic fixture commands with the bounded API client while
  retaining only simple display fixtures for tests and demos.
- [x] Reuse one configured `http.Client` and transport for the process lifetime.
- [x] Add deadlines, response-size limits, body closure, and typed API errors.
- [x] Implement first-run invite exchange and pseudonymous identity creation.
- [x] Store the P-256 private key in the OS credential store, with an explicit
  owner-only file fallback, while keeping access tokens in memory.
- [x] Show the delete-only revocation credential once without persisting it.
- [x] Create sessions with a fresh challenge, renew them on expiry, and create a
  fresh DPoP proof for every request, including ambiguous-registration recovery
  with the same key.
- [x] Connect authenticated deletion in settings and lost-identity revocation on
  the first-launch screen to the shared server deletion behavior.
- [x] Never accept private keys or access tokens through a command flag, URL,
  clipboard prompt, or landing-page handoff.
- [x] Connect send, claim, open, reply, withdraw, report, block, and keepsake
  screens to their real API operations.
- [x] Preserve drafts and current context across recoverable network failures.
- [x] Handle offline startup, reconnect, expired claims, invalid identities,
  conflicts, rate limits, and server failures with actionable messages.
- [x] Show passive update notices from the post office without downloading or
  installing updates.
- [x] Prevent duplicate mutation submissions while a request is active.
- [x] Ignore stale asynchronous messages after screen or operation changes.
- [x] Confirm every online flow remains usable through the keyboard.
- [x] Confirm accessible and reduced-motion modes work with real responses.
- [x] Add contract tests between shared DTOs, handlers, and client decoding.
- [x] Add a controlled pseudo-terminal test for first run, lost-identity
  revocation, and letter release.
- [x] Regenerate the VHS journey against a local real post office and synthetic
  test identities.
- [x] Add an interactive disposable post-office task that isolates local
  identity material and cleans up its PostgreSQL and synthetic KMS state.

Done when two clean TUI installations with different local identities can use a
real test server to send, claim, unfold, reply, keep, report, and block without
direct API calls or database access, while access tokens remain memory-only and
restart authentication succeeds from each stored device key.

### Phase 4: landing page and release distribution

- [x] Scaffold `orifude-front` with Astro, strict TypeScript, and pnpm.
- [x] Pin pnpm through `packageManager` and commit the frontend lockfile.
- [x] Add Tailwind CSS 4 through `@tailwindcss/vite`.
- [x] Add and configure Oxlint, Oxfmt, Astro Check, and project scripts.
- [x] Add frontend CI for formatting, linting, Astro checks, and production
  builds without production secrets.
- [x] Document frontend prerequisites, local commands, and environment variables.
- [x] Build the base layout with metadata, canonical URL, favicon, social card,
  design tokens, skip link, and reduced-motion handling.
- [x] Build the hero with the real Orifude wordmark, tagline, and primary
  download action.
- [x] Add the VHS-generated terminal recording with a poster and text fallback.
- [x] Add how-it-works, principles, privacy, safety, downloads, FAQ, source, and
  contact sections.
- [x] Validate checked-in release metadata and checksums with Zod at build time.
- [x] Keep Ky uninstalled unless a real HTTP fetch becomes necessary; document
  the request and failure behavior before adding it.
- [x] Build responsive mobile and desktop layouts without generic dashboard
  cards or fake application controls.
- [x] Optimize copies of the supplied logo, icon, watermark, and monochrome
  assets without modifying the originals; optimize video, reserve dimensions,
  and lazy-load below the fold.
- [x] Make the page usable by keyboard, screen reader, mouse, and touch.
- [x] Add a restrictive content security policy and other static security
  headers.
- [x] Connect the `orifude-front` repository to Cloudflare Pages with
  `pnpm build` and `dist` as its build settings.
- [x] Confirm Cloudflare Pages creates preview deployments for pull requests.
- [ ] Attach `orifude.com` as the canonical custom domain.
- [ ] Redirect `www.orifude.com` to `https://orifude.com`.
- [x] Confirm the landing deployment has no post-office or database secret.
- [x] Build Linux, macOS, and Windows artifacts for the approved architecture
  matrix.
- [ ] Add GoReleaser after `cmd/orifude` exists and publish archives and
  checksums through GitHub Releases.
- [ ] Publish Homebrew and Scoop metadata to their `shrek` branches.
- [ ] Publish an AUR package through its separate SSH-backed repository.
- [ ] Publish checksum-verifying POSIX shell and PowerShell installers.
- [ ] Publish immutable release checksums and connect download links.
- [ ] Run Oxc, Astro, build, responsive, accessibility, SEO, link, header, and
  download smoke checks.

Done when `orifude-front` installs from a clean checkout,
`https://orifude.com` loads its production Astro build from Cloudflare Pages,
all checks pass, `www` redirects correctly, and a visitor can download and
verify a working TUI release without the site calling the post-office API.

### Phase 5: private online alpha

- [x] Select Railway for the Go service and managed PostgreSQL.
- [ ] Create separate production and disposable test databases.
- [ ] Provision separate AWS KMS message and evidence keys with annual rotation,
  least-privilege runtime and human roles, and CloudTrail alerts.
- [ ] Configure and test rotation of the restricted Railway AWS credential.
- [ ] Run Goose migrations as a controlled deployment step, not server startup.
- [ ] Configure TLS, service secrets, pool limits, timeouts, readiness, and
  graceful deployment behavior.
- [ ] Disable production core dumps and confirm no debug or pprof route exists.
- [ ] Configure Cloudflare or deployment-edge request limits for the API host.
- [ ] Implement random hashed single-use invites with seven-day expiry and
  pre-redemption revocation.
- [ ] Establish report review, escalation, evidence retention, and response
  procedures before inviting testers.
- [ ] Protect all moderation routes with Cloudflare Access and verify its JWT at
  the origin.
- [ ] Build the internal moderation tool around AWS SSO, the fixed review purpose,
  local evidence decryption, and content-free output handling.
- [ ] Publish an accurate alpha privacy notice covering application encryption,
  service-side decryption, visible metadata, and application-compromise limits.
- [ ] Configure structured log retention and alerts without content logging.
- [ ] Configure database backups and complete one documented restore test.
- [ ] Limit database backup and WAL retention to 30 days and run retention
  reconciliation before a restored service becomes ready.
- [ ] Keep restore deployments unrouted until the recovery owner proves commit
  continuity and approves promotion; keep the service offline if proof fails.
- [ ] Restore encrypted data into a disposable database and prove authorized KMS
  decryption still works without copying KMS administration into Railway.
- [ ] Measure API latency, claim contention, connection-pool waits, and TUI
  startup with representative synthetic traffic.
- [ ] Recruit a bounded tester group and provide one support/contact channel.
- [ ] Track crashes, authorization failures, delivery failures, reports, and
  rate-limit rejections during the alpha.
- [ ] Fix all critical and high-impact defects found in the complete journey.
- [ ] Re-run keyboard-only, reduced-motion, monochrome, and accessible-mode QA
  on release artifacts.

Done when invited users on separate networks complete the full journey against
production, backup restoration and retention reconciliation have succeeded, one
synthetic report has been reviewed and closed through the audited path, Railway
has been denied evidence decrypt in a policy test, and no unresolved defect can
expose another user's letter or duplicate a claim.

### Phase 6: public readiness and launch

- [x] Fix waiting, claim, keepsake, report, identity, and deleted-content
  retention in this document.
- [ ] Implement and verify scheduled retention cleanup without an in-memory
  second source of truth.
- [ ] Publish privacy policy, terms, acceptable-use rules, contact, and deletion
  instructions.
- [ ] Verify both identity deletion flows and their data effects.
- [ ] Complete a threat-model review covering pseudonymous abuse, alias
  impersonation, device-key and session theft, DPoP replay, authorization,
  terminal injection, races, KMS misuse, and operational access.
- [ ] Retest every authorization boundary with controlled identities.
- [ ] Validate edge and per-identity rate limits against measured abuse cases.
- [ ] Confirm moderation capacity and an incident escalation path.
- [ ] Measure supported load, database pool behavior, and claim latency before
  setting public traffic limits.
- [ ] Verify restore objectives and deployment rollback with production-shaped
  data.
- [ ] Verify SHA-256 checksums for every immutable release artifact and install
  path; signing and attestations remain out of scope.
- [ ] Publish final supported platforms and installation instructions.
- [ ] Complete landing-page accessibility, performance, SEO, and security-header
  audits on the production domain.
- [ ] Complete TUI QA on every supported native platform, not only cross-builds.
- [ ] Remove alpha-only claims from copy and open identity creation according to
  the approved abuse-control plan.

Done when pseudonymous registration can be opened without removing the controls
that protected alpha users, the measured system fits published limits, release
artifacts pass native smoke tests, and moderation, backup, deletion, and incident
procedures each have a named owner.

## Settled policy

These are product boundaries, not a backlog:

- One immutable global alias per identity, visible only to matched strangers
- No alias search, public profile, follower graph, reputation, or public history
- No identity recovery, account linking, private-key export, or second device;
  the separate offline credential can only delete
- No push notifications or background notification service
- No native desktop wrapper or browser letter client
- One reply per letter, permanently
- No content or behavior ranking; matching remains eligible FIFO
- No end-to-end encryption; TLS, application envelope encryption, strict
  authorization, and honest disclosure of service-side decryption are required
- One Go service, PostgreSQL, and AWS KMS until measured service objectives prove
  another component necessary

Operational policy is fixed:

- Unopened claims expire after 24 hours.
- Unclaimed letters are deleted after seven days.
- Completed keepsakes remain until both participants remove them.
- Live report evidence is deleted 90 days after moderation closes the case;
  disaster-recovery copies expire within 30 more days.
- Live report metadata is deleted one year after closure, and each live
  moderation audit event is deleted after one year; recovery copies expire
  within 30 more days.
- Access sessions last 15 minutes and require a DPoP proof from the identity key.
- The offline revocation credential deletes an identity and grants no other
  access.
- Senders can always reread their original letter while retaining the keepsake.
- Explicit deletion and one year without authenticated activity both delete
  identity access. Deleted aliases and device keys remain reserved forever.
- Moderation is report-only. Railway cannot decrypt retained evidence; human
  evidence decrypt attempts are recorded in CloudTrail.
- Blocks are hidden and irreversible while both identities exist.
- Private-alpha invites are random, hashed, single-use, valid for seven days,
  and revocable before redemption.
- Railway hosts the Go service and PostgreSQL in one project.
- The TUI may show a passive version notice from the post office but never
  downloads or installs an update.
- Native Japanese review is not required. Orifude remains explicitly described
  as a coined name, not a Japanese dictionary word.
- Release artifacts use SHA-256 checksums. Signing and attestations are out of
  scope.

## Definition of the 1.0 release

Every build remains `0.x` until the full public release contract is met. Version
1.0 requires two real people on separate machines to complete this journey in
production:

1. Install the TUI on every supported OS and architecture through GitHub
   archives and each applicable shell, PowerShell, Homebrew, Scoop, and AUR
   channel.
2. Verify each downloaded artifact with its published SHA-256 checksum.
3. Create unique pseudonymous identities inside the TUI without an alpha invite.
4. Send one letter through the online post office and reread the sender copy.
5. Claim that letter exactly once from the other identity.
6. Unfold and read it without terminal-control injection.
7. Send the only allowed reply and see the exchange in both keepsake views.
8. Report and irreversibly block the other identity while learning only its
   alias, never its internal identifier or history.
9. Complete the entire operational journey with keyboard navigation.
10. Delete each participant's keepsake access and verify that the shared record
    is purged only after both delete it.
11. Download the checksummed TUI from `https://orifude.com` while the Cloudflare
    Pages site remains unable to read, send, or display letters.

The landing page, public registration, moderation process, backups, restore
test, deletion jobs, all promised install channels, and native-platform smoke
tests must be live before 1.0. A feature-complete private build remains `0.x`.
