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
| `mise run vhs` | Record and verify the offline TUI journey with Docker. |
