# Headless Exec MVP — Tech Spec

## Context

M0 (`specs/000-foundation/`) left a compiling workspace: `hikari_protocol` defines `Op`/`EventMsg`
(`crates/hikari_protocol/src/{op,event}.rs`), `hikari_features` has `FeatureFlag`, `hikari_cli`
(`crates/hikari_cli/src/main.rs`) parses subcommands and stubs them, and every other crate is an
empty placeholder. Invariant numbers refer to `product.md`.

## Proposed changes

### New workspace dependencies

`tokio` (`rt-multi-thread`, `macros`, `sync`, `time`, `process`, `io-util`, `fs`, `signal`),
`tokio-util` (`CancellationToken`), `futures`, `reqwest` (`default-features = false`, rustls TLS,
`json`, `stream`), `bytes`, `toml`, `ignore`, `globset`, `grep-regex`, `grep-searcher`,
`tempfile` (dev only). Each must be at least 7 days old; list them in the PR's dependency section.

### `hikari_protocol`

- `EventMsg::ToolCallRequested` gains `arguments: String` (raw JSON as produced by the model).
- `EventMsg::ToolCallCompleted` gains `output: String` (the capped result sent to the model).
- `EventMsg::ToolCallDenied { call_id, reason: String }`.
- `EventMsg::TurnCompleted` gains `usage: Option<TokenUsage>`;
  `TokenUsage { input_tokens: u64, output_tokens: u64 }`.
- `EventMsg::TurnAborted { turn_id }` for interruption (25).
- `ApprovalRequested` stays; `exec` answers it automatically (below).
- `serde_json` becomes a normal dependency only if needed; `arguments` stays a `String`.

### `hikari_config`

```rust
pub struct Config { pub home: PathBuf, pub provider: String, pub model: Option<String>,
                    pub providers: BTreeMap<String, ProviderConfig> }
pub struct ProviderConfig { pub kind: ProviderKind, pub base_url: String,
                            pub api_key_env: Option<String> }
pub enum ProviderKind { OpenAiChat }
pub struct Overrides { pub provider: Option<String>, pub model: Option<String> }
pub fn load(overrides: Overrides) -> Result<Config, ConfigError>;
fn load_from(home: PathBuf, file: Option<&str>, overrides: Overrides) -> Result<Config, ConfigError>;
```

- `home` = `HIKARI_HOME` env var, else `std::env::home_dir()/.hikari` (1). Reading only; nothing is
  created.
- Layering: built-in `openai` provider and `provider = "openai"` → file → overrides (2, 3). A file
  `[providers.openai]` table replaces the built-in.
- `load_from` is the testable seam (no env/file access). TOML uses `snake_case` keys; unknown keys
  are an error naming the key.

### `hikari_llm`

```rust
pub struct ModelRequest { pub model: String, pub system: String, pub messages: Vec<Message>,
                          pub tools: Vec<ToolSpec> }
pub enum Message {
    User { text: String },
    Assistant { text: Option<String>, tool_calls: Vec<ToolCall> },
    ToolResult { call_id: String, content: String },
}
pub struct ToolCall { pub id: String, pub name: String, pub arguments: String }
pub struct ToolSpec { pub name: String, pub description: String, pub parameters: serde_json::Value }
pub enum StreamEvent { TextDelta(String), ToolCall(ToolCall), Completed { usage: Option<Usage> } }
pub type ResponseStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>;

pub trait ModelClient: Send + Sync {
    fn stream<'a>(&'a self, request: &'a ModelRequest) -> BoxFuture<'a, Result<ResponseStream, LlmError>>;
}
pub struct OpenAiChatClient { /* reqwest::Client, base_url, api_key: Option<String> */ }
```

- `sse.rs`: a minimal incremental SSE parser (`data:` lines, multi-line data, comments, `\r\n`,
  events split across byte chunks). Hand-written; no extra crate.
- `openai_chat.rs`: POST `{base_url}/chat/completions` with `stream: true`,
  `stream_options: {include_usage: true}`, `tools` as `{"type":"function","function":{...}}`.
  Accumulate `delta.tool_calls[i]` by `index` (id/name arrive once, `arguments` in fragments) and
  emit each `ToolCall` once its choice reports a `finish_reason` (23). `[DONE]` ends the stream.
  Unknown delta fields (e.g. `reasoning_content`) are ignored.
- Retries (24) happen inside `stream` before the stream is returned: 429/5xx/connect errors, 3
  retries, backoff 1 s/2 s/4 s or `Retry-After` (seconds form), capped at 30 s.
- `LlmError` variants: `Http { status, message }`, `Transport(String)`, `Decode(String)`,
  `Cancelled`. Error `Display` never includes request headers (5).

### `hikari_sandbox`

M1 ships no sandbox, only the sanctioned spawner:

```rust
pub struct Command { /* wraps tokio::process::Command */ }
impl Command { pub fn new(program: impl AsRef<OsStr>) -> Self; /* arg, args, current_dir, env,
    stdin/stdout/stderr, spawn */ }
```

`new` sets `kill_on_drop(true)` and, on Windows, `CREATE_NO_WINDOW` (17). The
`clippy::disallowed_types` allow lives only in this module.

### `hikari_tools`

```rust
pub struct ToolContext { pub workspace: PathBuf, pub cancel: CancellationToken }
pub enum ToolOutput { Ok(String), Err(String) }
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn risk(&self, args: &serde_json::Value, ctx: &ToolContext) -> ToolRisk;
    fn run<'a>(&'a self, args: serde_json::Value, ctx: &'a ToolContext) -> BoxFuture<'a, ToolOutput>;
}
pub struct ToolRegistry { /* name -> Box<dyn Tool> */ }
impl ToolRegistry { pub fn builtin() -> Self; pub fn specs(&self) -> Vec<ToolSpec>;
                    pub fn get(&self, name: &str) -> Option<&dyn Tool>; }
pub fn cap_output(s: &str, max_chars: usize) -> String; // head + "\n… [N characters truncated] …\n" + tail
```

- One file per tool plus `<tool>_tests.rs`. File and search work runs in `spawn_blocking`.
- `edit_file` CRLF rule (14): if the file contains `\r\n` and `old_string` contains no `\r`,
  replace `\n` with `\r\n` in both strings before matching.
- `read_file` binary check: a NUL byte in the first 8 KiB (13).
- `shell` (17) reads stdout and stderr concurrently into a bounded buffer (keep the first and last
  15,000 characters while streaming, never the whole output), returns `exit code: N` plus output,
  and on cancel/timeout kills the child.
- `risk` for write tools canonicalizes the target's nearest existing ancestor and compares it to the
  canonical workspace (20).
- Invalid arguments (bad JSON or schema mismatch) return `ToolOutput::Err` describing the problem.

Tool specs (names, descriptions, and JSON Schemas) are fixed text:

| Tool | Description | Parameters |
|---|---|---|
| `read_file` | Read a text file. Returns lines prefixed with line numbers. Use offset and limit for large files. | `path` (string, required), `offset` (integer ≥ 1, 1-based line), `limit` (integer ≥ 1, default 2000) |
| `write_file` | Create or overwrite a file with the given content. Prefer edit_file for changes to existing files. | `path`, `content` (strings, required) |
| `edit_file` | Replace an exact string in a file. old_string must match exactly once unless replace_all is true. Read the file first. | `path`, `old_string`, `new_string` (strings, required), `replace_all` (boolean) |
| `list_dir` | List entries of a directory. Directories end with a slash. | `path` (string, default ".") |
| `glob` | Find files whose paths match a glob pattern such as "src/**/*.rs". Respects .gitignore. | `pattern` (string, required), `path` (string, default ".") |
| `grep` | Search file contents with a regular expression. Returns path:line:text matches. Respects .gitignore. | `pattern` (string, required), `path` (string), `glob` (string filter), `case_insensitive` (boolean) |
| `shell` | Run a shell command in the workspace root and return its exit code and output. | `command` (string, required), `timeout_secs` (integer, default 120, max 600) |

### `hikari_core`

```rust
pub enum ApprovalPolicy { Untrusted, Auto }
pub struct SessionConfig { pub model: String, pub workspace: PathBuf, pub policy: ApprovalPolicy,
                           pub max_round_trips: usize /* 100 */ }
pub struct SessionHandle { pub ops: mpsc::Sender<Op>, pub events: mpsc::Receiver<Event> }
pub fn spawn_session(config: SessionConfig, client: Arc<dyn ModelClient>, tools: ToolRegistry) -> SessionHandle;
```

Turn loop per `Op::UserInput`:

1. Emit `TurnStarted`; append `Message::User`.
2. Stream the model; forward `TextDelta` as `AgentMessageDelta`; collect tool calls.
3. No tool calls → emit `AgentMessage` (full text) and `TurnCompleted { usage }`.
4. Otherwise append the assistant message, then for each call in order: emit
   `ToolCallRequested`; if `risk` is not `ReadOnly` and the policy is `Untrusted`, emit
   `ApprovalRequested` and await `Op::Approval` for that `call_id`; on `Deny` emit
   `ToolCallDenied` and send the model
   `"Denied: this action requires approval. Re-run with --full-auto to allow writes and shell commands."`;
   otherwise run it and emit `ToolCallCompleted`. Unknown tool names produce an error result. Then
   go to step 2.
5. After `max_round_trips` model calls, emit `Error` and end the turn (10).

`Op::Interrupt` cancels the turn's `CancellationToken`; the model stream and running tool both
`select!` on it, and the turn ends with `TurnAborted` (25). Usage is summed across round trips.

System prompt (fixed text, `{workspace}` and `{os}` substituted):

```
You are Hikari, a coding agent running in the user's terminal. You help with software engineering
tasks in the workspace at {workspace} on {os}.

- Use the tools to inspect the codebase before answering questions about it or changing it. Do not
  guess file contents.
- Prefer edit_file for targeted changes and read a file before editing it.
- Follow the conventions of the existing code. Keep changes minimal and focused on the request.
- Run relevant build or test commands with shell when they help verify your work.
- If a tool call is denied, do not retry it; explain what you would have done instead.
- When done, reply with a concise summary of what you found or changed.
```

### `hikari_exec`

```rust
pub struct ExecArgs { pub prompt: String, pub workspace: PathBuf, pub json: bool, pub full_auto: bool,
                      pub provider: Option<String>, pub model: Option<String> }
pub async fn run(args: ExecArgs) -> ExitCode;
async fn run_with(args: ExecArgs, client: Arc<dyn ModelClient>, out: impl Write, err: impl Write) -> ExitCode;
```

- Builds the client from `hikari_config`, spawns a session with `Untrusted` (or `Auto` with
  `--full-auto`), sends the prompt then `Op::Shutdown` after the turn ends, auto-answers every
  `ApprovalRequested` with `Deny` (21).
- Rendering lives in pure functions so it is unit-testable: human mode prints
  `> <tool> <summary>` and `  ok` / `  error: <first line>` / `  denied` to stderr (summary =
  `path`, `pattern`, or `command` argument truncated to 80 chars), final text to stdout (7);
  `--json` prints each `Event` with `serde_json::to_string` per line (8).
- Ctrl+C via `tokio::signal::ctrl_c`: first sends `Op::Interrupt` and exits 130 after
  `TurnAborted`; the second calls `std::process::exit(130)` (25).

### `hikari_cli`

`exec` gains `-C/--cd <DIR>`, `--full-auto`, `--provider`, `--model`; prompt `-` reads stdin (6).
Builds a tokio runtime with `worker_threads = min(4, available_parallelism)` and calls
`hikari_exec::run`. Other subcommands stay stubbed.

## Testing and validation

| Invariant | Test |
|---|---|
| 1–4 | `hikari_config` unit tests on `load_from`: missing file, built-in openai, override precedence, unknown key, missing model message, missing key env var message (in `hikari_exec`). |
| 5 | `hikari_llm` test: 401 response's `LlmError` display does not contain the key. |
| 6–9 | `hikari_exec` tests via `run_with` + scripted client: stdout has only the final text; `--json` lines parse as `Event`; exit codes for success, provider error. |
| 10 | Core test: scripted client that always calls a tool hits the round-trip limit. |
| 11–19 | Per-tool tests with `tempfile`: read offset/limit/long line/binary; edit not-found/ambiguous/replace_all/equal/CRLF; write creates dirs; glob/grep honor `.gitignore` and caps; shell exit code, timeout kill, output cap; `cap_output` head/tail. |
| 20–22 | Risk classification tests (inside vs outside workspace); core test that `Untrusted` + `Deny` yields `ToolCallDenied` and the denial result, and `Auto` runs the tool. |
| 23 | SSE parser tests (split chunks, CRLF, multi-line data); OpenAI fixture with two tool calls whose arguments are fragmented across chunks. |
| 24 | Local `tokio::net::TcpListener` mock server (hand-written, no new crate) returning 429 then 200; asserts a retry and `Retry-After` honored (use a 0–1 s value). |
| 25 | Core test: interrupt during a scripted slow stream ends with `TurnAborted` within 500 ms; shell test: cancel kills a `sleep`-style command. |
| 26–27 | Manual: run exec against a real provider with `HIKARI_HOME` pointed at an empty dir, confirm it stays empty; record peak RSS (Task Manager / `/usr/bin/time -v`). |

`ModelClient` scripted fakes live in a `hikari_llm::testing` module behind a `testing` cargo
feature, enabled only as a dev-dependency feature.

## End-to-end flow

`hikari exec "where is the config loaded?"` → CLI parses → `hikari_config::load` → builds
`OpenAiChatClient` → `spawn_session(Untrusted)` → model calls `grep` (read-only, runs) → model calls
`read_file` → model replies with text → exec prints it to stdout, exit 0.

## Risks

- OpenAI-compatible servers differ (missing `stream_options` support, tool-call chunk shapes). The
  fixture set should include at least OpenAI and one compatible server; unsupported
  `stream_options` can be retried without it in a follow-up.
- No sandbox in M1: `--full-auto` gives the model the user's full privileges. The flag's help text
  must say so.

## Follow-ups

- Run independent read-only tool calls concurrently.
- Persist transcripts (M2) and add the Anthropic and Responses adapters (M4).
