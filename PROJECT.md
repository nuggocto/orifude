# Orifude

> Fold a thought. Leave it for one stranger.

Orifude is an online, anonymous, one-to-one letter exchange experienced only
through a terminal user interface. A person writes a short letter, folds it,
and releases it. The post office assigns it to one unrelated recipient. That
recipient unfolds it and may send one reply. The exchange then becomes a
keepsake for both people.

The public website is a separate, static landing page. It explains the project,
shows the TUI, and distributes builds. It never reads, writes, or displays
letters.

This document is the product and technical baseline. Decisions marked "open"
must be settled before a public release. Everything else is the intended first
implementation.

## Product identity

- Product name: `orifude`
- TUI repository and Go module: `orifude`
- Landing-page repository: `orifude-front`
- Public domain: `https://orifude.com`
- Working tagline: `Fold a thought. Leave it for one stranger.`
- Operational client: the Orifude TUI only
- Public presentation: a static website only
- Primary motif: folded paper carried through an ink-painted garden

The name is treated as a coined brand inspired by folding and brushwork. The
project must not market it as a dictionary Japanese word without a native
language review.

## Product promise

Orifude creates a small exchange between two people without turning that
exchange into content for an audience.

The product has no public feed, follower graph, profiles, likes, search,
trending page, recipient picker, attachments, or unrestricted direct messages.
Those omissions are product rules, not missing features.

### Core rules

1. Every letter has exactly one sender.
2. A letter can be claimed by at most one active recipient at a time.
3. The sender can never claim their own letter.
4. The recipient is selected by the server, not by the sender.
5. The body remains hidden until the assigned recipient explicitly unfolds it.
6. The recipient may send zero or one reply.
7. A reply cannot receive another reply.
8. Neither participant sees the other's identity or activity history.
9. A claimed letter is one-reader, not one-view. Its recipient may reopen it.
10. The website cannot participate in the exchange.

### Goals

- Make receiving a small anonymous letter feel deliberate rather than noisy.
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
- End-to-end encryption in the first release
- A browser version of the letter application
- A command-based interface or a family of CLI subcommands
- Offline delivery between users

The program is launched as a binary, but all user interaction after launch is
inside the TUI. Development and server administration may still use normal
commands and environment variables.

## User journeys

### First launch

1. The TUI displays the Orifude mark and checks terminal capabilities.
2. The person enters an alpha invite code if the service is invite-only.
3. The TUI requests an anonymous identity from the post office.
4. The post office returns a secret bearer token once.
5. The TUI stores the token in the operating system's user config directory
   with owner-only permissions.
6. The person enters the branch screen.

There is no username, password, profile, biography, avatar, or email in the
first release. Losing the local token loses access to that identity and its
keepsakes. This must be stated during onboarding.

### Send a letter

1. Select `Fold a letter` from the branch screen.
2. Write between 1 and 2,000 Unicode code points.
3. Preview the folded form generated from the letter's `fold_seed`.
4. Confirm release.
5. The TUI sends the letter with a client-generated opaque ID.
6. The post office stores it in the waiting queue.
7. The sender receives a release receipt and can see its delivery state.

The client-generated ID makes a retried submission idempotent. A timeout after
a successful write must not create a duplicate letter.

### Receive a letter

1. Select `Wait by the branch`.
2. The TUI asks the post office for one available letter.
3. The post office returns an existing unexpired claim for this identity or
   atomically claims the oldest eligible letter.
4. The TUI displays only its folded form and age.
5. The person explicitly unfolds it.
6. The post office records the open and returns the body.
7. The person may reply, keep it without replying, report it, or discard it.

An unopened claim expires after a configurable period. The first proposed
period is 24 hours. After expiry, the letter returns to the queue and can be
claimed by someone else. Once opened, assignment is permanent.

### Reply

1. The recipient selects `Fold a reply` on an opened letter.
2. The reply accepts between 1 and 2,000 Unicode code points.
3. The recipient previews and confirms it.
4. The server writes it only if no reply already exists.
5. The sender sees the reply in the same keepsake.

Retries with the same body are safe. A second, different reply is rejected.

### Report and block

1. The recipient selects `Report and burn`.
2. The TUI asks for one reason from a fixed list.
3. The server records the report, hides the letter from the reporter, and blocks
   future matching between the two anonymous identities.
4. The server retains the reported content for the moderation retention period.

The recipient never receives an identifier for the sender. Blocking is applied
server-side using the letter relationship.

## Letter lifecycle

State is derived from timestamps and ownership columns rather than duplicated
in a mutable status string.

```text
                  claim lease expires
             +-----------------------------+
             |                             |
             v                             |
[waiting] --claim--> [folded/claimed] --unfold--> [opened] --reply--> [replied]
    |                       |                    |                    |
 withdraw                  report               report               report
    |                       |                    |                    |
    v                       v                    v                    v
[withdrawn]             [reported]           [reported]           [reported]
```

Derived states:

| State | Required facts |
| --- | --- |
| Waiting | No recipient, not withdrawn, not reported |
| Claimed | Recipient and claim timestamps exist, not opened |
| Opened | Recipient and opened timestamp exist, no reply |
| Replied | Reply body and replied timestamp exist |
| Withdrawn | Sender withdrew before a successful claim |
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
                                                     |
                                                     | pgx
                                                     v
                                          +----------------------+
                                          | PostgreSQL           |
                                          | source of truth      |
                                          +----------------------+

 +----------------+       Cloudflare Pages
 | orifude-front  | ---------------------> browser
 | Astro + Vite   |       https://orifude.com
 +----------------+

 The landing page has no path to the letter API or database.
```

### Runtime responsibilities

The TUI owns presentation, keyboard input, local identity storage, request
timeouts, retry prompts, and rendering a fold from a server-provided seed.

The post office owns authentication, authorization, validation, letter state
transitions, recipient assignment, claim leases, rate limits, reports, blocks,
and persistence.

PostgreSQL owns durable truth, uniqueness, referential integrity, and the row
locks that prevent two people claiming one letter.

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
- `crypto/rand` creates identity secrets, public IDs, and fold seeds.
- `crypto/sha256` hashes high-entropy identity tokens before database storage.

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
- Bubble Tea mouse click and wheel messages make every visible control usable
  with a mouse. Hover and pointer motion are not required.
- Charm's VHS records deterministic terminal demos for the landing page and
  release notes. VHS is development tooling, not a runtime dependency.

Bubble Tea commands perform HTTP I/O outside `Update`. Results return as typed
messages. The program does not start unmanaged goroutines.

### Post office

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
- One managed PostgreSQL database
- TLS terminated by the deployment platform or reverse proxy
- Cloudflare Pages for `orifude-front`, connected to its GitHub repository
- `orifude.com` as the canonical custom domain and `www.orifude.com` redirected
  to it
- GitHub Releases for checksummed TUI binaries, with signing added when the
  release process can operate it reliably
- GoReleaser for release archives and checksums once `cmd/orifude` exists
- Homebrew through `nuggocto/homebrew-tap`, Scoop through
  `nuggocto/scoop-bucket`, and AUR through its separate SSH-backed repository
- Cloudflare DNS and TLS for the public domain
- The Go service and managed PostgreSQL provider remain open; application code
  must not depend on one host

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
|   |   +-- httpapi/
|   |   |   +-- router.go            # Chi routes and middleware order
|   |   |   +-- letters.go           # letter transport and error mapping
|   |   |   +-- identities.go        # identity transport
|   |   |   +-- moderation.go        # report and block transport
|   |   +-- identity/
|   |   |   +-- local.go             # owner-only local token storage
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
|   |           +-- letters.sql.go
|   |           +-- claims.sql.go
|   |           +-- moderation.sql.go
|   +-- sql/
|   |   +-- migrations/
|   |   |   +-- 00001_initial.sql    # Goose Up and Down sections
|   |   +-- queries/
|   |       +-- identities.sql
|   |       +-- letters.sql
|   |       +-- claims.sql
|   |       +-- moderation.sql
|   +-- PROJECT.md
|   +-- README.md
|   +-- go.mod
|   +-- go.sum
|   +-- sqlc.yaml
|   +-- Makefile                     # only repeated development tasks
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

The package split is deliberate:

- `internal/database` owns the pool, readiness, shutdown, and transaction entry
  points.
- `internal/database/dbgen` contains only sqlc output.
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
|   +-- Invite code
|   +-- Anonymous identity creation
|   +-- Recovery warning
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
    |
    +-- Settings
        +-- Motion
        +-- Theme and contrast
        +-- Connection status
        +-- Identity and local data
        +-- About
```

Global keys:

| Key | Action |
| --- | --- |
| `?` | Toggle contextual help |
| `esc` | Leave text input, then go back or close a modal |
| `tab` / `shift+tab` | Move focus |
| `h` / left arrow | Move left, close detail, or go back |
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

Navigation follows familiar Neovim motions, but Orifude does not implement a
complete modal text editor. While a Bubbles textarea or Huh text field has
focus, printable keys edit text and `q` is ordinary letter content. `esc`
returns to navigation mode. This keeps the bindings predictable without owning
a second editor implementation.

### Mouse behavior

Mouse support is required for the first release and has keyboard parity:

- Left click selects visible buttons, tabs, list rows, and confirmation choices.
- Clicking a folded letter opens the same action as `enter`.
- The wheel scrolls letters, keepsakes, help, and long settings forms.
- Clicking the composer focuses it; clicking outside returns to navigation.
- Destructive actions still require confirmation after the initial click.
- No action exists only on hover, right click, drag, or double click.
- Focus remains visible after a mouse action so keyboard use can resume.

The TUI enables click and wheel events, not all-motion tracking. Avoiding
continuous pointer motion reduces redraws and interferes less with native
terminal text selection. Hit regions are rebuilt from the current layout on
every `View`, so compact and wide layouts share the same action model.

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
    hitRegions []HitRegion
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
| Mouse targets | Small `[]HitRegion` rebuilt by `View` | Target count is tiny and layout order matters |
| Short forms | Embedded `*huh.Form` | Reuses validation, selection, confirmation, and accessible mode |
| Async results | Concrete Bubble Tea message structs | Type switches match the framework model |
| Local token | Fixed-length byte value at creation, encoded on disk | Bounds and entropy remain explicit |

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
- Mouse and keyboard actions dispatch the same internal action values.
- Text-input mode receives key presses before global Neovim-style bindings.
- Letter bodies and bearer tokens are never written to debug logs.

## Persistent data model

The expected initial workload is modest: one write per sent letter, one atomic
claim, one open, and at most one reply. Reads are identity-scoped keepsake pages
and claim lookups. A single PostgreSQL instance is sufficient. The design avoids
Redis, queues, caches, search indexes, and partitioning until measurements show
a need.

### `identities`

| Column | Type | Rules |
| --- | --- | --- |
| `id` | `bigint generated always as identity` | Primary key, internal only |
| `token_hash` | `bytea` | Unique, exactly 32 bytes |
| `created_at` | `timestamptz` | Required, server time |
| `disabled_at` | `timestamptz` | Nullable |

The client receives 32 random bytes encoded with unpadded base64url. The server
stores only `SHA-256(token)`. Because the token is high entropy, a database leak
does not provide a practical offline token-guessing route. Password hashing is
not appropriate because this is not a human-chosen password.

### `letters`

| Column | Type | Rules |
| --- | --- | --- |
| `id` | `varchar(22)` | Primary key, 128-bit client-generated base64url ID |
| `sender_id` | `bigint` | Required identity foreign key |
| `recipient_id` | `bigint` | Nullable identity foreign key |
| `body` | `text` | 1 to 2,000 Unicode code points, at most 12 KiB UTF-8 |
| `fold_seed` | `bigint` | Non-negative random 63-bit value |
| `created_at` | `timestamptz` | Required |
| `claimed_at` | `timestamptz` | Nullable, agrees with recipient |
| `claim_expires_at` | `timestamptz` | Nullable, unopened claims only |
| `opened_at` | `timestamptz` | Nullable, requires recipient |
| `reply_body` | `text` | Nullable, same 2,000-code-point and 12 KiB limits |
| `replied_at` | `timestamptz` | Nullable, agrees with reply body |
| `withdrawn_at` | `timestamptz` | Nullable, only before claim |

The original and reply share one row because the product permits exactly two
messages. A generic `conversations` plus `messages` design was rejected. It
would add a relationship, joins, ordering rules, and impossible multi-message
states for flexibility Orifude does not offer. If the product later permits
multi-turn conversations, that change requires a deliberate migration rather
than speculative schema now.

The 2,000-code-point limit is long enough for an actual letter, including
Japanese text, while still fitting a terminal viewport and a tightly bounded
request. The API limits the complete JSON request to 16 KiB before decoding.
The TUI shows the remaining code-point count and byte validation errors before
release. Database `CHECK` constraints enforce code-point bounds with
`char_length`; Go enforces both code points and UTF-8 bytes.

### `blocks`

| Column | Type | Rules |
| --- | --- | --- |
| `blocker_id` | `bigint` | Identity foreign key |
| `blocked_id` | `bigint` | Identity foreign key, differs from blocker |
| `created_at` | `timestamptz` | Required |

The composite primary key is `(blocker_id, blocked_id)`. Matching excludes a
pair if either direction is blocked.

### `reports`

| Column | Type | Rules |
| --- | --- | --- |
| `id` | `bigint generated always as identity` | Primary key |
| `letter_id` | `varchar(22)` | Letter foreign key |
| `reporter_id` | `bigint` | Identity foreign key |
| `reason` | `smallint` | Fixed application enum |
| `created_at` | `timestamptz` | Required |
| `reviewed_at` | `timestamptz` | Nullable |

`(letter_id, reporter_id)` is unique. Free-form moderation notes are excluded
from the user-facing report to avoid collecting another body of unsafe text.

### Required indexes

```sql
CREATE INDEX letters_waiting_idx
    ON letters (created_at, id)
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
```

No index is added without a named query. Index usage must be checked after real
traffic before adding more.

### Data invariants

- Letter IDs are valid 22-character unpadded base64url values.
- Original and reply bodies contain 1 to 2,000 Unicode code points and no more
  than 12 KiB of valid UTF-8.
- The sender and recipient differ.
- `recipient_id`, `claimed_at`, and `claim_expires_at` are all null or initially
  set together.
- `opened_at` requires a recipient.
- `reply_body` and `replied_at` are both null or both present.
- A reply requires `opened_at`.
- A withdrawal requires no recipient and no open.
- Only the sender may read their sent body and eventual reply.
- Only the active recipient may open, read, reply to, report, or discard a
  received letter.
- Everyone else receives `404`, not an ownership-revealing `403`.
- The claim operation excludes both directions of a blocked pair.
- Letter and reply bodies are validated in Go and constrained again in SQL.

## Atomic letter claiming

Claiming is the highest-risk concurrency operation. It uses one short database
transaction and row locking.

```sql
SELECT id
FROM letters
WHERE recipient_id IS NULL
  AND withdrawn_at IS NULL
  AND sender_id <> $1
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
scheduled cleanup query. There is no background in-memory queue. PostgreSQL is
the only source of truth.

FIFO ordering is intentional. `ORDER BY random()` becomes expensive as the
waiting set grows and gives old letters no delivery guarantee. A different
matching policy should be added only for a concrete product requirement.

## HTTP API

All endpoints live under `/v1`. Requests and responses use JSON. Letter and
reply requests are limited to 16 KiB before decoding. Smaller endpoints use
tighter limits. Decoders reject unknown fields and trailing JSON values. All
strings receive explicit semantic, code-point, byte, and control-character
validation where applicable.

| Method and path | Purpose |
| --- | --- |
| `POST /v1/identities` | Exchange an alpha invite for a new anonymous token |
| `GET /v1/me` | Validate identity and return service limits |
| `POST /v1/letters` | Release an idempotent client-ID letter |
| `POST /v1/letters/claim` | Return or atomically create one active claim |
| `GET /v1/letters/{id}` | Return role-safe metadata and authorized content |
| `POST /v1/letters/{id}/open` | Permanently assign and reveal a claimed body |
| `POST /v1/letters/{id}/reply` | Add the only allowed reply |
| `POST /v1/letters/{id}/withdraw` | Withdraw an unclaimed sent letter |
| `POST /v1/letters/{id}/report` | Report a received letter |
| `POST /v1/letters/{id}/block` | Block future matching with its other party |
| `GET /v1/keepsakes` | Cursor-paginated sent and received exchanges |
| `GET /healthz` | Process liveness only |
| `GET /readyz` | Database-backed readiness |

Authorization uses `Authorization: Bearer <token>`. The server never accepts an
identity ID from the client as proof of identity.

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
bearer authentication for protected /v1 routes
handler
```

Authentication and body limits are mounted at the narrowest route group that
needs them. Forwarded client IP headers are trusted only when the deployment
proxy strips user-supplied values. The service does not enable broad CORS
because the TUI is the only API client and the landing page never calls it.

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
- Requests have end-to-end deadlines.
- Response bodies are always closed and size-limited.
- Automatic retries apply only to safe reads or explicitly idempotent writes.
- Claim, open, reply, report, block, and withdraw are idempotent at the server.
- Exponential retry machinery is not planned. The person chooses to retry after
  a bounded failure.

## Authentication and local identity

The identity token is an opaque bearer credential. Possession grants access to
sent letters, received letters, and keepsakes.

The TUI stores configuration under `os.UserConfigDir()` in an `orifude`
directory. The token file is created with owner-only permissions where the
platform supports Unix permission bits. Atomic write-and-rename prevents a
partial file from destroying the credential.

The token is never accepted through a CLI flag, printed in normal output,
included in URLs, sent to the landing page, or logged. A future system keychain
integration may replace the file if users need stronger local protection.

Token recovery, account linking, and multi-device identities are deferred. They
require a recovery authority such as email, passkeys, or recovery codes and
would weaken the current no-account promise if added casually.

## Safety, privacy, and abuse controls

Anonymous messages are a hostile-input boundary. The initial service must be an
invite-only alpha until moderation and operational response exist.

### Required before alpha

- Server-side authorization on every letter operation
- Fixed body and request-size limits
- Per-identity send and claim limits
- Deployment-edge IP rate limiting
- Report and block actions in the TUI
- A documented moderation review path
- No rendering of terminal escape sequences from letter bodies
- No logging of bodies, replies, invite codes, or bearer tokens
- Database backups and a tested restore procedure
- A privacy statement that explains plaintext server storage

All user text must be treated as text. Control characters other than newline
are rejected or visibly escaped before terminal rendering. This prevents a
letter from injecting terminal commands, hyperlinks, cursor controls, or fake
Orifude UI.

### Privacy position

- Transport is encrypted with TLS.
- The managed database must provide encryption at rest.
- Letter bodies are readable by the post office because delivery and moderation
  require server access in the first release.
- The system is not end-to-end encrypted and must not imply otherwise.
- Public analytics are absent by default on the landing page.
- Logs use internal IDs only and never contain message content or credentials.

### Initial limits

Letter and reply bodies allow at most 2,000 Unicode code points and 12 KiB of
valid UTF-8. Their complete JSON request bodies allow at most 16 KiB. Other
operational numbers remain configurable: letters per hour, claims per minute,
reports per day, identity creation attempts, and database pool size. Defaults
start conservatively during alpha and change from observed traffic and abuse.

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

The first release matrix is Linux, macOS, and Windows on amd64 and arm64.
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

## Configuration

### TUI configuration

| Setting | Source | Default |
| --- | --- | --- |
| API base URL | Built-in release value, environment override for development | Production service URL |
| Identity token | Owner-only local config file | Created during onboarding |
| Reduced motion | TUI settings | Auto-detect where possible, otherwise off |
| Theme | TUI settings | Terminal background adaptive |
| Debug logging | Development environment only | Disabled |

### Server configuration

| Setting | Purpose |
| --- | --- |
| `DATABASE_URL` | PostgreSQL connection string |
| `LISTEN_ADDR` | HTTP listen address |
| `INVITE_SECRET` or invite-store configuration | Alpha identity creation |
| `CLAIM_TTL` | Unopened claim lease |
| Rate-limit settings | Alpha abuse tuning |
| `LOG_LEVEL` | Structured log threshold |

Secrets are injected by the deployment platform. They are never committed,
printed at startup, or exposed through readiness endpoints.

## Failure behavior

- Offline startup opens the TUI in a clear disconnected state with local help
  and settings available.
- A draft survives screen navigation within the running process.
- A release timeout asks the server about the same client-generated letter ID
  before offering another submission.
- A lost claim response returns the identity's existing active claim on retry.
- An expired unopened claim disappears with an explanation rather than showing
  stale content.
- Server errors preserve the current screen and user text.
- Unauthorized identity errors lead to a recovery warning, never silent token
  replacement.
- Database unavailability fails readiness so new traffic stops reaching the
  instance.
- Graceful shutdown stops new requests, drains admitted requests for a bounded
  period, and closes the database pool.

## Testing strategy

Tests protect behavior and state transitions, not rendered implementation
details.

### Unit tests

- TUI screen transitions from typed Bubble Tea messages
- Neovim-style navigation while in navigation mode
- Printable keyboard behavior while a textarea is focused
- Mouse clicks and keyboard selections dispatch the same action
- Mouse hit regions remain correct across compact and wide layouts
- Fold output is deterministic for a given seed and terminal size
- Input validation accepts 2,000 code points and rejects 2,001, oversized UTF-8,
  invalid UTF-8, and unsafe control text
- API error codes map to the correct visible state

### Integration tests

- Migrations apply to an empty disposable PostgreSQL database
- Goose follows the documented migration and rollback policy
- sqlc regeneration produces no uncommitted generated-code drift
- Two simultaneous claim requests cannot receive the same letter
- A sender cannot claim their own letter
- A blocked pair is never matched
- Unauthorized identities cannot infer or read a letter
- Create, open, reply, report, and withdraw are idempotent
- An expired unopened claim can return to the queue
- The full HTTP router enforces body limits and content types

### End-to-end tests

Keep this suite small:

1. Start the shipped post-office binary against a disposable database.
2. Create two synthetic identities through the real HTTP API.
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

It never logs authorization headers, invite codes, request bodies, letter
bodies, reply bodies, or raw database errors returned to clients.

Initial operation relies on platform HTTP metrics, database metrics, readiness,
and structured logs. Prometheus, tracing, and a custom metrics dependency are
deferred until the deployment or an incident demonstrates a need.

Events worth counting without content:

- Identities created
- Letters released
- Claims created and expired
- Letters opened
- Replies created
- Reports created
- Authorization failures
- Rate-limit rejections
- Database pool acquire waits

## Delivery phases

Check a task only after its behavior and verification are complete. A created
file, passing compile, or mocked happy path does not finish a task whose stated
behavior is broader. Each phase closes only when its "Done when" condition is
met.

### Phase 0: foundations and decisions

- [x] Approve the TUI as the only operational client and keep the website
  presentation-only.
- [x] Record `orifude.com` as the Cloudflare-managed public domain.
- [x] Write the initial product, architecture, data, security, design, and
  delivery specification in `PROJECT.md`.
- [x] Remove the abandoned browser-application scaffold from `orifude` while
  preserving this project document and approved artwork.
- [ ] Initialize or confirm the `orifude` Go module and repository.
- [ ] Create the sibling `orifude-front` Astro repository.
- [x] License source code, documentation, and project-owned artwork under
  Apache-2.0.
- [x] Support Linux, macOS, and Windows on amd64 and arm64 for the first release.
- [ ] Resolve the retention, claim lease, sender reread, and invite decisions in
  the open-decisions section.
- [ ] Pin the current supported Go toolchain and direct Go dependencies.
- [ ] Pin pnpm through `packageManager` and commit the frontend lockfile.
- [ ] Pin sqlc, Goose, govulncheck, Oxlint, and Oxfmt as project tools.
- [ ] Create the root `sql/queries` and `sql/migrations` directories.
- [ ] Create `sqlc.yaml` targeting `internal/database/dbgen` with pgx v5.
- [ ] Add baseline CI jobs for Go checks, generated-code drift, and frontend
  checks without requiring production secrets.
- [ ] Optimize copies of the supplied logo, icon, watermark, and monochrome
  assets without modifying the originals.
- [ ] Document local development prerequisites and environment variables.

Done when both repositories install from clean checkouts, all empty baseline
checks pass, sqlc and Goose paths are fixed, assets are accounted for, and no
blocking product decision remains for the prototype or schema.

### Phase 1: offline TUI interaction prototype

- [ ] Add Bubble Tea v2, Bubbles v2, Lip Gloss v2, and Huh v2.
- [ ] Implement one root model with explicit `Screen` and `InputMode` enums.
- [ ] Build the splash, first-run, branch, compose, fold preview, delivery,
  unfold, read, reply, keepsake, report, and settings screens using synthetic
  fixtures.
- [ ] Implement wide, compact, text-first, and too-small terminal layouts.
- [ ] Translate the monochrome Orifude mark into ANSI and ASCII-safe artwork.
- [ ] Implement the truecolor palette with ANSI 256, ANSI 16, monochrome, and
  light/dark background behavior.
- [ ] Implement deterministic fold shapes and fixed-frame fold/unfold animation.
- [ ] Add reduced-motion behavior that skips decorative frames.
- [ ] Accept letters and replies up to 2,000 Unicode code points and 12 KiB.
- [ ] Reject or visibly neutralize unsafe terminal control characters.
- [ ] Add Neovim-style `h`, `j`, `k`, `l`, `g g`, `G`, half-page, and full-page
  navigation outside text-input mode.
- [ ] Make `i` focus the composer and `esc` return to navigation mode.
- [ ] Ensure printable keys, including `q`, remain text while an editor is
  focused.
- [ ] Add mouse click targets for every visible action and wheel scrolling for
  every scrollable view.
- [ ] Ensure mouse and keyboard paths dispatch the same internal actions.
- [ ] Integrate Huh for onboarding, confirmations, settings, and report reasons.
- [ ] Expose Huh accessible mode and ensure important state is never color-only.
- [ ] Add contextual Bubbles help for active keyboard and mouse actions.
- [ ] Add focused tests for navigation modes, mouse hit regions, size changes,
  input boundaries, control text, and deterministic folds.
- [ ] Create a deterministic VHS tape for the core compose-to-unfold journey.

The prototype may use immutable synthetic fixtures, but fixture behavior must
not become a local database or a second post office.

Done when a person can complete the entire simulated journey with only a
keyboard or only a mouse at every supported terminal size, all TUI tests pass,
and the VHS recording can be regenerated from a clean checkout.

### Phase 2: PostgreSQL and post-office API

- [ ] Write `sql/migrations/00001_initial.sql` with Goose Up and Down sections
  for identities, letters, blocks, reports, constraints, and named indexes.
- [ ] Verify the migration applies to an empty PostgreSQL database.
- [ ] Document whether production rollback uses Goose Down or a forward repair
  migration for each released schema change.
- [ ] Write named sqlc queries under `sql/queries` for identities, letters,
  claims, keepsakes, blocks, and reports.
- [ ] Generate and commit package `dbgen` under `internal/database/dbgen`.
- [ ] Add CI drift detection for generated sqlc files.
- [ ] Implement pgx pool startup, ping, readiness, sizing, and shutdown in
  `internal/database`.
- [ ] Implement transaction ownership in `internal/postoffice` using generated
  sqlc queries bound to pgx transactions.
- [ ] Implement the identity-token exchange and SHA-256 token lookup.
- [ ] Implement letter creation with client-generated idempotent IDs.
- [ ] Implement atomic claim reuse and assignment with identity locking and
  `FOR UPDATE SKIP LOCKED`.
- [ ] Implement claim expiry and safe requeue of unopened letters.
- [ ] Implement authorized open, reply, withdraw, report, block, and keepsake
  operations.
- [ ] Implement the Chi route tree, middleware order, route-specific body
  limits, bearer authentication, and stable error responses.
- [ ] Configure explicit HTTP server timeouts and bounded graceful shutdown.
- [ ] Enforce 2,000 code points, 12 KiB text, 16 KiB JSON, valid UTF-8, and
  control-character rules at API and database boundaries.
- [ ] Return `404` for unauthorized letter access without leaking ownership.
- [ ] Add per-identity limits with configuration suitable for private alpha.
- [ ] Add structured slog output with credential and content redaction.
- [ ] Add unit tests for validation, authorization, and state transitions.
- [ ] Add PostgreSQL integration tests for every sqlc query and migration.
- [ ] Add concurrent claim tests that prove one letter reaches one recipient.
- [ ] Add router tests for content types, limits, authentication, timeouts, and
  idempotent retries.
- [ ] Add one API end-to-end test covering identity, send, claim, open, reply,
  keepsakes, report, and block with synthetic data.

Done when the shipped post-office binary and a real disposable PostgreSQL
database pass the complete API journey, concurrent claim tests cannot duplicate
delivery, migration and sqlc drift checks pass, and logs contain neither test
letter bodies nor tokens.

### Phase 3: online TUI integration

- [ ] Replace synthetic fixture commands with the bounded API client while
  retaining only simple display fixtures for tests and demos.
- [ ] Reuse one configured `http.Client` and transport for the process lifetime.
- [ ] Add deadlines, response-size limits, body closure, and typed API errors.
- [ ] Implement first-run invite exchange and anonymous identity creation.
- [ ] Store the bearer token atomically with owner-only local permissions.
- [ ] Never accept the token through a command flag, URL, clipboard prompt, or
  landing-page handoff.
- [ ] Connect send, claim, open, reply, withdraw, report, block, and keepsake
  screens to their real API operations.
- [ ] Preserve drafts and current context across recoverable network failures.
- [ ] Handle offline startup, reconnect, expired claims, invalid identities,
  conflicts, rate limits, and server failures with actionable messages.
- [ ] Prevent duplicate mutation submissions while a request is active.
- [ ] Ignore stale asynchronous messages after screen or operation changes.
- [ ] Confirm every online flow remains usable through keyboard and mouse.
- [ ] Confirm accessible and reduced-motion modes work with real responses.
- [ ] Add contract tests between shared DTOs, handlers, and client decoding.
- [ ] Add a controlled pseudo-terminal test for first run and letter release.
- [ ] Regenerate the VHS journey against a local real post office and synthetic
  test identities.

Done when two clean TUI installations with different local identities can use a
real test server to send, claim, unfold, reply, keep, report, and block without
direct API calls or database access.

### Phase 4: landing page and release distribution

- [ ] Scaffold `orifude-front` with Astro, strict TypeScript, and pnpm.
- [ ] Add Tailwind CSS 4 through `@tailwindcss/vite`.
- [ ] Add and configure Oxlint, Oxfmt, Astro Check, and project scripts.
- [ ] Build the base layout with metadata, canonical URL, favicon, social card,
  design tokens, skip link, and reduced-motion handling.
- [ ] Build the hero with the real Orifude wordmark, tagline, and primary
  download action.
- [ ] Add the VHS-generated terminal recording with a poster and text fallback.
- [ ] Add how-it-works, principles, privacy, safety, downloads, FAQ, source, and
  contact sections.
- [ ] Validate checked-in release metadata and checksums with Zod at build time.
- [ ] Keep Ky uninstalled unless a real HTTP fetch becomes necessary; document
  the request and failure behavior before adding it.
- [ ] Build responsive mobile and desktop layouts without generic dashboard
  cards or fake application controls.
- [ ] Optimize all images and video, reserve dimensions, and lazy-load below the
  fold.
- [ ] Make the page usable by keyboard, screen reader, touch, and mouse.
- [ ] Add a restrictive content security policy and other static security
  headers.
- [ ] Connect the `orifude-front` repository to Cloudflare Pages with
  `pnpm build` and `dist` as its build settings.
- [ ] Confirm Cloudflare Pages creates preview deployments for pull requests.
- [ ] Attach `orifude.com` as the canonical custom domain.
- [ ] Redirect `www.orifude.com` to `https://orifude.com`.
- [ ] Confirm the landing deployment has no post-office or database secret.
- [ ] Build Linux, macOS, and Windows artifacts for the approved architecture
  matrix.
- [ ] Add GoReleaser after `cmd/orifude` exists and publish archives and
  checksums through GitHub Releases.
- [ ] Publish Homebrew and Scoop metadata to their `shrek` branches.
- [ ] Publish an AUR package through its separate SSH-backed repository.
- [ ] Publish checksum-verifying POSIX shell and PowerShell installers.
- [ ] Publish immutable release checksums and connect download links.
- [ ] Run Oxc, Astro, build, responsive, accessibility, SEO, link, header, and
  download smoke checks.

Done when `https://orifude.com` loads the production Astro build from Cloudflare
Pages, all checks pass, `www` redirects correctly, and a visitor
can download and verify a working TUI release without the site calling the
post-office API.

### Phase 5: private online alpha

- [ ] Select the Go service host and managed PostgreSQL provider.
- [ ] Create separate production and disposable test databases.
- [ ] Run Goose migrations as a controlled deployment step, not server startup.
- [ ] Configure TLS, service secrets, pool limits, timeouts, readiness, and
  graceful deployment behavior.
- [ ] Configure Cloudflare or deployment-edge request limits for the API host.
- [ ] Establish invite issuance, expiry, revocation, and support procedures.
- [ ] Establish report review, escalation, evidence retention, and response
  procedures before inviting testers.
- [ ] Publish an accurate alpha privacy notice and plaintext-storage statement.
- [ ] Configure structured log retention and alerts without content logging.
- [ ] Configure database backups and complete one documented restore test.
- [ ] Measure API latency, claim contention, connection-pool waits, and TUI
  startup with representative synthetic traffic.
- [ ] Recruit a bounded tester group and provide one support/contact channel.
- [ ] Track crashes, authorization failures, delivery failures, reports, and
  rate-limit rejections during the alpha.
- [ ] Fix all critical and high-impact defects found in the complete journey.
- [ ] Re-run keyboard-only, mouse-only, reduced-motion, monochrome, and
  accessible-mode QA on release artifacts.

Done when invited users on separate networks complete the full journey against
production, backup restoration has succeeded, reports have an owner and response
path, and no unresolved defect can expose another user's letter or duplicate a
claim.

### Phase 6: public readiness and launch

- [ ] Finalize waiting, claim, keepsake, report, and deleted-content retention.
- [ ] Implement and verify scheduled retention cleanup without an in-memory
  second source of truth.
- [ ] Publish privacy policy, terms, acceptable-use rules, contact, and deletion
  instructions.
- [ ] Add an identity deletion flow and verify its data effects.
- [ ] Complete a threat-model review covering anonymous abuse, token theft,
  authorization, terminal injection, races, and operational access.
- [ ] Retest every authorization boundary with controlled identities.
- [ ] Validate edge and per-identity rate limits against measured abuse cases.
- [ ] Confirm moderation capacity and an incident escalation path.
- [ ] Measure supported load, database pool behavior, and claim latency before
  setting public traffic limits.
- [ ] Verify restore objectives and deployment rollback with production-shaped
  data.
- [ ] Add signed artifacts or attestations if the chosen release process can
  operate them reliably; checksums remain mandatory.
- [ ] Publish final supported platforms and installation instructions.
- [ ] Complete landing-page accessibility, performance, SEO, and security-header
  audits on the production domain.
- [ ] Complete TUI QA on every supported native platform, not only cross-builds.
- [ ] Remove alpha-only claims from copy and open identity creation according to
  the approved abuse-control plan.

Done when anonymous registration can be opened without removing the controls
that protected alpha users, the measured system fits published limits, release
artifacts pass native smoke tests, and moderation, backup, deletion, and incident
procedures each have a named owner.

### Deferred until proven necessary

- Multi-device identity recovery
- Push notifications
- Native desktop wrappers
- Public profiles or social graphs
- Multi-turn conversations
- Content recommendation
- Redis, message queues, event buses, or microservices
- End-to-end encryption
- Browser letter client

## Open decisions

These decisions require product or operational agreement before implementation
reaches the related phase:

- [ ] Set the unopened claim lease; the current proposal is 24 hours.
- [ ] Set waiting-letter expiry; the current proposal is 7 days.
- [ ] Set completed-keepsake retention and deletion behavior.
- [ ] Decide whether a sender can reread their original body after release.
- [ ] Define alpha invite issuance and revocation.
- [ ] Select managed PostgreSQL and Go service hosting providers.
- [ ] Decide whether the TUI performs automatic update checks.
- [ ] Obtain native Japanese review of the name and Japanese-language marketing.

## Definition of the first complete release

The first release is complete when two real people on separate machines can:

1. Install the TUI from a checksummed release.
2. Create anonymous identities inside the TUI.
3. Send one letter through the online post office.
4. Claim that letter exactly once from the other identity.
5. Unfold and read it without terminal-control injection.
6. Send one reply.
7. See the exchange in both keepsake views.
8. Report and block the other identity without learning its identifier.
9. Complete the operational journey with Neovim-style keys or with a mouse.
10. Download the checksummed TUI from `https://orifude.com` while the website
    remains unable to read or send letters.

The landing page must explain and distribute that release, but it must remain
incapable of performing any of these application actions.
