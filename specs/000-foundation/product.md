# Hikari Foundation — Product Spec

## Problem

Existing coding agents (Codex CLI, Claude Code, Gemini CLI) deliver strong results but are hostile
to the machines they run on:

- Unbounded on-disk state. Session logs, rollouts, snapshots, and caches grow without limit; users
  report hundreds of GB consumed by a single agent.
- High memory. Node.js-based agents idle at hundreds of MB of RSS and grow with session length.
- Heavy installs. A runtime plus `node_modules` must be installed and kept up to date.

Hikari is a coding agent that matches the capability of the leading agents while treating the
user's CPU, RAM, and disk as scarce resources it must account for.

## Goals

- Feature and quality parity with Claude Code / Codex CLI for day-to-day coding work.
- Single self-contained native binary per platform (Windows, macOS, Linux; x86_64 and aarch64).
- Hard, user-visible resource budgets, enforced by the product rather than documented as advice.
- One core, multiple front-ends: interactive TUI, headless `exec`, and an embeddable app-server.

## Non-goals

- A GUI desktop application or IDE fork (the app-server exists so others can build those).
- Hosting models or running a cloud backend.
- Plugin runtimes that require Node.js or Python to be installed.

## Behavior invariants

### Install and footprint

1. Hikari ships as one executable with no runtime dependencies beyond the OS (and `git` for
   git-aware features). Installing it never requires Node.js, Python, or a package manager.
2. Release binary size is at most 30 MB on every tier-1 target.
3. `hikari --version` completes in under 50 ms on a warm cache.
4. Idle interactive session RSS is at most 40 MB; RSS during an active turn, excluding child
   processes spawned by tools, is at most 150 MB for a 200k-token context.

### Disk hygiene

5. All persistent state lives under a single data directory (`$HIKARI_HOME`, defaulting to
   `~/.hikari`). Hikari never writes elsewhere except files the user asked it to edit and
   OS temp files it deletes before exit.
6. The data directory has a configurable total size budget (default 1 GiB). When a write would
   exceed the budget, Hikari evicts the oldest sessions/checkpoints first and never fails the
   current turn because of the budget.
7. Sessions older than a configurable retention period (default 30 days) are pruned automatically.
8. Session transcripts are stored compressed and append-only; tool outputs larger than a threshold
   are stored once, truncated in the transcript, and referenced by hash.
9. File checkpoints (for undo/rewind) store only files Hikari modified, content-addressed and
   deduplicated. Hikari never copies or snapshots an entire workspace.
10. `hikari storage` shows current usage per category and `hikari storage gc` reclaims space on
    demand.

### Surfaces

11. `hikari` (no args) starts the interactive TUI in the current directory.
12. `hikari exec "<prompt>"` runs a single task non-interactively, streams progress to stderr, writes
    the final answer to stdout, and exits non-zero on failure. `--json` emits the event stream as
    JSON Lines.
13. `hikari app-server` speaks JSON-RPC 2.0 over stdio so IDE extensions and GUIs can drive
    sessions. The TUI and `exec` use the same protocol types internally.

### Models

14. Supported provider protocols: Anthropic Messages, OpenAI Responses, and OpenAI-compatible Chat
    Completions (covering DeepSeek, Qwen, OpenRouter, Ollama, vLLM, and similar).
15. Providers, models, base URLs, and credentials are configured in TOML; API keys may come from
    environment variables and are never written to transcripts or logs.
16. Responses stream token by token; cancellation (Ctrl+C / `interrupt` RPC) stops the in-flight
    request and any running tool within 500 ms.

### Agent capabilities (parity set)

17. Built-in tools: read file, write file, edit/patch file, glob, grep, list directory, shell
    command, web fetch, todo list, and spawning sub-agents.
18. MCP servers (stdio and streamable HTTP) can be configured and their tools are exposed to the
    model alongside built-ins.
19. Project instructions are loaded from `AGENTS.md` (and `CLAUDE.md` for compatibility) found in
    the workspace hierarchy and in the user config dir.
20. Long sessions are kept within the model context window by automatic compaction; users can also
    trigger it with `/compact`.
21. Sessions can be listed, resumed (`hikari resume`, `/resume`), and rewound to a prior checkpoint.
22. Slash commands, custom commands from Markdown files, skills, and lifecycle hooks are supported.

### Safety

23. Every tool call is classified as read-only, workspace-write, or dangerous. The approval policy
    (`untrusted`, `on-request`, `auto`) decides which classes require user confirmation.
24. Shell commands run inside an OS sandbox by default (Seatbelt on macOS, Landlock+seccomp on
    Linux, restricted token/job object on Windows) limited to the workspace and temp dirs, with
    network off unless allowed.
25. Spawned processes never flash a console window on Windows.

## Open questions

- Telemetry: none by default; whether to offer opt-in anonymous metrics is undecided.
- Distribution channels beyond GitHub Releases (Homebrew, winget, Scoop, cargo-binstall).
