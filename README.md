# Orifude

> Send a letter into the quiet and let a stranger find it.

Orifude is a pseudonymous, one-to-one letter exchange for the terminal.

See [PROJECT.md](PROJECT.md) for product and technical details.

## Development

Install the pinned Go toolchain with [mise](https://mise.jdx.dev/):

```sh
mise install
```

Run the TUI:

```sh
mise run dev
```

The participant TUI connects to `https://api.orifude.com` by default. For local
development, set `ORIFUDE_API_URL` to an HTTPS origin or a loopback HTTP origin:

```sh
ORIFUDE_API_URL=http://127.0.0.1:8080 mise run dev
```

`mise run vhs` owns a disposable test post office and drives the terminal
journey automatically. Its fixture invite works only during that run, and the
test post office stops when the recording finishes. It is not an interactive
backend for a later `mise run dev` session.

Orifude keeps access sessions in memory. It stores the device key in the
operating-system credential store and offers an explicitly confirmed,
owner-only file fallback if that store is unavailable. The delete-only
revocation credential is displayed once and is never persisted. Set
`ORIFUDE_OFFLINE_DEMO=1` only when running the fixture-backed local demo.

Database migrations remain a separate deployment or development step; neither
the TUI nor the post office runs them at startup.

Available tasks:

| Command | Purpose |
| --- | --- |
| `mise run dev` | Run the TUI. |
| `mise run postoffice` | Run the post office using the server variables in `PROJECT.md`. |
| `mise run build` | Build all Go packages. |
| `mise run fmt` | Format all Go packages. |
| `mise run test` | Run the test suite. |
| `mise run test-race` | Run the test suite with the race detector. |
| `mise run vet` | Run Go static analysis. |
| `mise run vuln` | Scan Go dependencies for known vulnerabilities. |
| `mise run check` | Run tests, race detection, vet, and vulnerability scanning. |
| `mise run vhs` | Record the online TUI journey against a disposable local post office. |
