# Contributing to Hikari

Hikari follows an issue-first development flow. All code, comments, docs, commit messages, and PR
text are written in English.

## Issues come first

- **Bugs**: once a bug report is actionable (reproducible, scope understood), it can go straight
  to a code PR.
- **Features**: a maintainer labels the issue `ready-to-spec` when it needs design, or
  `ready-to-implement` when the direction is already agreed.

## Spec PRs (features)

Before feature code lands, open a spec PR adding a directory `specs/GH<issue-number>/` containing:

- `product.md` — problem, goals, non-goals, and numbered, testable behavior invariants covering
  the happy path, states, errors, cancellation, offline behavior, permissions, and races.
- `tech.md` — context (current system with file/line references), proposed changes (modules,
  types/APIs, data flow, tradeoffs), and testing/validation that maps each invariant to a test.

Templates live in `specs/_template/`; see `specs/README.md` for the directory conventions.
`specs/000-foundation/` is the bootstrap exception to the `GH<issue-number>` naming rule.

## Code PRs

- Branch from `main`. Keep each PR to one logical change.
- Add or update tests with your change.
- Before pushing, run `cargo xtask presubmit` (fmt check, clippy, tests, deny). Follow the
  validation order in AGENTS.md: targeted checks while editing, tests, clippy, then the formatter
  once at the end.
- Fill in `.github/pull_request_template.md`, including proof of manual testing (commands run,
  output, screenshots for TUI changes).
- When the change is user-visible, add a changelog line in the PR body using one of:
  - `CHANGELOG-NEW-FEATURE:`
  - `CHANGELOG-IMPROVEMENT:`
  - `CHANGELOG-BUG-FIX:`

## Style

Coding style, testing conventions, and architecture rules live in [AGENTS.md](AGENTS.md) — it is
authoritative for both humans and agents working in this repo.

## Security

Do not report security issues in public issues or PRs. See [SECURITY.md](SECURITY.md).
