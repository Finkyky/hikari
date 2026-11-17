# Headless Exec MVP — Product Spec

## Problem

Hikari has no working agent yet. The first useful, testable slice is a non-interactive
`hikari exec` that can answer questions about a codebase and, when explicitly allowed, change it.
It proves the core loop, one provider protocol, and the essential tools before the TUI exists.

## Goals

- `hikari exec "<prompt>"` completes real coding tasks end to end against any OpenAI-compatible
  Chat Completions endpoint (OpenAI, DeepSeek, Qwen, OpenRouter, Ollama, vLLM).
- Read-only by default; writes and shell only with an explicit flag.
- Zero bytes written to disk by Hikari itself (persistence arrives in M2).

## Non-goals

- TUI, app-server, sessions/resume, checkpoints, compaction, MCP, AGENTS.md loading, sandboxing,
  Anthropic and OpenAI Responses adapters (later milestones; see `000-foundation/tech.md`).

## Behavior invariants

### Configuration

1. Hikari reads `$HIKARI_HOME/config.toml`, where `$HIKARI_HOME` defaults to `~/.hikari`. A missing
   file is not an error.
2. The config selects a default `provider` and `model`; `[providers.<name>]` tables define `kind`
   (`openai_chat`), `base_url`, and `api_key_env` (the name of an environment variable). A built-in
   `openai` provider (`https://api.openai.com/v1`, `OPENAI_API_KEY`) exists without configuration.
3. `--provider` and `--model` override the config. If no model is resolved, exec exits with code 1
   and a message naming both the config key and the flag.
4. If the provider's key variable is unset, exec exits with code 1 naming the variable. Providers
   with no `api_key_env` (e.g. local Ollama) send no `Authorization` header.
5. API keys never appear in stdout, stderr, or `--json` output, including error messages.

### Running a task

6. `hikari exec "<prompt>"` runs one task in the current directory; `-C <dir>` sets the workspace
   root instead. The prompt may also be read from stdin when given as `-`.
7. Human mode: the final agent message is written to stdout; progress (one line per tool call
   with a short argument summary and its outcome) goes to stderr. Streaming text is not echoed to
   stdout so stdout contains only the final answer.
8. `--json` writes every event as one JSON object per line to stdout and nothing else to stdout.
9. Exit codes: `0` task completed, `1` configuration, provider, or internal error, `2` usage error,
   `130` interrupted.
10. A task stops after 100 model round trips with an error event and exit code 1.

### Tools

11. Available tools: `read_file`, `write_file`, `edit_file`, `list_dir`, `glob`, `grep`, `shell`.
12. Relative paths resolve against the workspace root.
13. `read_file` returns numbered lines, defaults to the first 2000 lines, supports `offset`/`limit`,
    truncates lines over 2000 characters, and refuses binary files with an explanatory error.
14. `edit_file` replaces `old_string` with `new_string`. It fails without modifying the file if
    `old_string` is absent, if it matches more than once and `replace_all` is not set, or if the two
    strings are equal. Files using CRLF line endings keep CRLF after an edit made with LF strings.
15. `write_file` creates parent directories and overwrites existing files.
16. `glob` and `grep` respect `.gitignore`, `.ignore`, and hidden-file rules the same way ripgrep
    does by default, and cap results (default 200) with a note when truncated.
17. `shell` runs a command with the workspace root as cwd, using `$SHELL -c` (fallback `/bin/sh`)
    on Unix and PowerShell (`pwsh` if on `PATH`, else `powershell.exe`) on Windows. Default timeout
    is 120 s, maximum 600 s; on timeout the process is killed and the result says so. No console
    window is shown on Windows.
18. Any single tool result sent to the model is capped at 30,000 characters, keeping the head and
    tail with an explicit truncation marker.
19. Tool failures are returned to the model as error results; they never abort the task.

### Permissions

20. Each tool call is classified `read_only` (`read_file`, `list_dir`, `glob`, `grep`),
    `workspace_write` (`write_file`/`edit_file` inside the workspace), or `dangerous` (`shell`, or
    writes outside the workspace).
21. By default exec allows only `read_only` calls. Other calls are denied without executing, and
    the model receives an error result telling it the action requires `--full-auto`.
22. `--full-auto` allows all calls.

### Streaming, errors, and cancellation

23. Model output streams; tool calls whose arguments arrive across many chunks are reassembled
    correctly, including several tool calls in one response.
24. HTTP 429 and 5xx responses received before any streamed data are retried up to 3 times with
    exponential backoff (honoring `Retry-After` when present, capped at 30 s); other HTTP errors fail
    immediately with the status and the provider's error message.
25. The first Ctrl+C cancels the in-flight model request or tool (killing its process) within
    500 ms and exits with code 130. A second Ctrl+C exits immediately.

### Footprint

26. A full exec run writes no files except those the model edits through tools.
27. Peak RSS of the `hikari` process during a typical exec task stays under 60 MB.

## Open questions

- Whether the default model for the built-in `openai` provider should be preset; for now the user
  must choose a model.
