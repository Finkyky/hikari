# Hikari

Hikari is a resource-frugal coding agent written in Rust, aiming for feature and quality parity
with Claude Code and Codex CLI while treating your CPU, RAM, and disk as scarce resources. One
core drives three front-ends: an interactive TUI, a headless `exec` mode, and an embeddable
JSON-RPC app-server.

Hard resource budgets are a product feature:

- Release binary at most 30 MB per platform.
- Idle interactive session RSS at most 40 MB.
- All persistent state under a single data directory with a 1 GiB default budget and 30-day
  default retention.

See `specs/000-foundation/` for the full product and tech specs.

## Status

Pre-alpha. Milestone M0: workspace, tooling, CI, protocol skeleton, and feature flags. No agent
logic is implemented yet; every command exits with "not implemented yet".

## Build and run

```console
cargo run -p hikari_cli              # interactive TUI (default)
cargo run -p hikari_cli -- exec "<prompt>"
cargo run -p hikari_cli -- app-server
cargo build --release -p hikari_cli
```

Developer setup and checks:

```console
cargo xtask bootstrap   # install cargo-nextest and cargo-deny
cargo xtask presubmit   # fmt check, clippy, tests, deny
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the issue/spec/PR flow and
[AGENTS.md](AGENTS.md) for coding style. Feature work is specified under `specs/`.

## License

MIT. See [LICENSE](LICENSE).
