---
name: write-product-spec
description: Write a Hikari product spec (specs/<id>/product.md) defining user-visible behavior as numbered, testable invariants. Use when a feature issue is ready-to-spec or the user asks for a product spec.
---

# Write a product spec

1. Read `specs/000-foundation/product.md`, `specs/_template/product.md`, and the linked issue.
   Read any existing spec the feature builds on.
2. Create `specs/GH<issue-number>/product.md` (use the next `NNN-<slug>` only when there is no
   issue yet) from the template.
3. Describe behavior from the consumer's view (CLI user, TUI user, app-server client). Stay out of
   implementation detail: no crate, type, or function names.
4. Write the core as a numbered list of invariants. Each one must be observable and testable, with
   concrete values (limits, defaults, exit codes, messages) instead of adjectives like "fast".
   Cover: happy path, visible states, inputs and outputs, empty/error states, cancellation,
   offline, permission denied, concurrency/races, and Windows/macOS/Linux differences.
5. State the resource impact (binary size, RSS, disk) whenever the feature can affect it; disk
   writes must go through the budgeted data directory.
6. List non-goals explicitly and put unresolved decisions under "Open questions" instead of
   guessing.
7. Keep it short. Do not restate the foundation spec; reference its invariant numbers.
