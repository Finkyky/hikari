# AGENTS.md

This file provides guidance when working with code in this repository. All code, comments, docs,
commit messages, and PR text are written in English.

## Development Commands

### Build and Run
- `cargo run -p hikari_cli` - Build and run the `hikari` binary (TUI by default)
- `cargo run -p hikari_cli -- exec "<prompt>"` - Run a headless task
- `cargo run -p hikari_cli -- app-server` - Run the JSON-RPC stdio server
- `cargo build --release -p hikari_cli` - Release build

### Setup
- `cargo xtask bootstrap` - Install developer tools (`cargo-nextest`, `cargo-deny`)

### Testing
- `cargo nextest run --no-fail-fast --workspace` - Run all tests
- `cargo nextest run -p <package>` - Run tests for one crate
- `cargo test --doc --workspace` - Run doc tests

### Linting and Formatting
- `cargo xtask presubmit` - Full local presubmit (fmt check, clippy, tests, deny). Run only when
  explicitly requested.
- `cargo xtask format` - Format all code
- `cargo clippy -p <package> --all-targets --tests -- -D warnings` - Targeted Clippy

All automation lives in `xtask/` rather than shell scripts so it behaves identically on Windows,
macOS, and Linux.

### Implementation Validation Order
Optimize for fast delivery and let CI catch uncommon failures outside targeted local coverage.

1. While editing, run only the smallest targeted `cargo check` or `cargo nextest` command that gives
   useful feedback; defer checks and tests until after implementation where possible.
2. Once the code and self-review are complete, run the relevant tests and fix the code until they
   pass.
3. Run the relevant Clippy checks and fix their findings. Return to affected tests only when a fix
   materially changes behavior.
4. Run `cargo xtask format` once, after all other code changes are complete.
5. Open or update the PR without rerunning tests or lint after formatting.

A later source, test, manifest, or configuration change creates a new candidate: rerun the affected
portion of the sequence and finish with the formatter.

## Architecture Overview

Hikari is a resource-frugal coding agent written in Rust. One core drives three front-ends. The
design is specified in `specs/000-foundation/`; read it before structural changes.

- `crates/hikari_protocol` - `Op` submissions and `Event` stream shared by every surface. No I/O.
- `crates/hikari_core` - Session, turn loop, context assembly, compaction, approvals.
- `crates/hikari_llm` - Model client trait and provider adapters.
- `crates/hikari_tools` - Built-in tools and risk classification.
- `crates/hikari_sandbox` - OS sandboxes; the only sanctioned way to spawn processes.
- `crates/hikari_mcp` - MCP client.
- `crates/hikari_store` - Data dir, transcripts, blobs, checkpoints, disk budget and retention GC.
- `crates/hikari_config` - Layered TOML configuration.
- `crates/hikari_features` - Runtime feature flags.
- `crates/hikari_tui`, `crates/hikari_exec`, `crates/hikari_app_server` - Front-ends.
- `crates/hikari_cli` - The `hikari` binary; argument parsing and dispatch only.

Dependency rules:
- Front-ends depend on `hikari_core` and `hikari_protocol`, never on each other.
- `hikari_protocol` depends on no workspace crate.
- Add third-party dependencies to `[workspace.dependencies]` and reference them with
  `workspace = true`.

### Resource Budgets
Footprint is a product feature, not an optimization pass. See `specs/000-foundation/product.md`.
- Never write outside `$HIKARI_HOME` except files the user asked to edit; route all persistent
  writes through `hikari_store` so they are budgeted and garbage-collected.
- Never snapshot or copy a whole workspace. Checkpoints store only touched files, deduplicated.
- Stream large data instead of buffering it; cap tool output kept in memory and in transcripts.
- Justify every new dependency in the PR description (size, compile time, license). Prefer
  `default-features = false`.

## Coding Style Preferences
- Avoid unnecessary type annotations, especially in closure params.
- Avoid long path qualifiers; import at the top of the file. Inside `cfg`-guarded branches, scope
  the import locally or use an absolute path for one-offs.
- If a function takes a context parameter, name it `ctx` and put it last, unless the function also
  takes a closure, which goes last.
- Remove unused parameters completely rather than prefixing them with `_`.
- Use inline format arguments (`format!("{name}")`, not `format!("{}", name)`).
- Libraries return typed errors (`thiserror`); binaries and front-ends may use `anyhow`.
- No `unwrap()`/`expect()` in non-test code unless an invariant makes failure impossible; state the
  invariant in the `expect` message.
- Spawn processes only through `hikari_sandbox`; `std::process::Command` is disallowed by Clippy
  because it flashes a console window on Windows and bypasses the sandbox.

### Comments
Comments have a cost: they must be kept in sync with the code.
- Assume the reader is a senior engineer. Do not explain WHAT or HOW when names already do.
- Inline comments are for "why" only: non-obvious rationale, third-party workarounds, complex
  algorithms, unidiomatic code, or surprising edge cases.
- No line-by-line narration.
- Keep doc comments concise. Document public APIs; do not narrate implementation steps.
- Document a thing once, at its declaration; do not repeat it at call sites.
- A container's doc comment describes the whole; members document themselves.
- Do not list callers in doc comments.
- No "transformation comments" describing your edits; those belong in the PR.
- Do not remove existing comments in unrelated changes.
- Reflow comments to the 100-column `max_width`.

### Exhaustive Matching
Avoid the `_` wildcard in `match` whenever possible so new enum variants force every site to be
revisited.

## Testing
- Use `cargo nextest`.
- Unit tests live in a sibling file named `${filename}_tests.rs` (or `mod_tests.rs` for `mod.rs`)
  and are included at the end of the module:
  ```rust
  #[cfg(test)]
  #[path = "filename_tests.rs"]
  mod tests;
  ```
- Unit tests never touch the network or the real `$HIKARI_HOME`. Use recorded fixtures and
  `tempfile` directories.
- TUI screens use render-to-lines snapshot tests; `exec --json` uses golden JSONL files.

## Feature Flags
Add a variant to `FeatureFlag` in `crates/hikari_features/src/lib.rs`, optionally list it in
`DOGFOOD_FLAGS`, and gate code with `FeatureFlag::YourFlag.is_enabled()`.
- Prefer runtime checks over `#[cfg(...)]`; use `cfg` only when code cannot compile otherwise.
- Keep flags product-level, not per call site.
- Remove the flag and dead branches once the feature is stable.

## Contribution Flow
- Issues come first. Bugs that are actionable go straight to a code PR.
- Features need a spec PR: `specs/GH<issue-number>/product.md` (testable behavior invariants) and
  `tech.md` (context, proposed changes, testing). Templates live in `specs/_template/`.
- PRs use `.github/pull_request_template.md` and include a changelog line when user-visible:
  `CHANGELOG-NEW-FEATURE:`, `CHANGELOG-IMPROVEMENT:`, or `CHANGELOG-BUG-FIX:`.
- Keep each PR to one logical change. Do not disclose security issues publicly; see `SECURITY.md`.
