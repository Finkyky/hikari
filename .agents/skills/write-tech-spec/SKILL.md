---
name: write-tech-spec
description: Write a Hikari tech spec (specs/<id>/tech.md) grounded in the current codebase, mapping every product invariant to a test. Use after the product spec exists, or when the user asks for a tech spec.
---

# Write a tech spec

1. Read the feature's `product.md`, `specs/000-foundation/tech.md`, `AGENTS.md`, and
   `specs/_template/tech.md`.
2. Explore the code the feature touches. Every claim about the current system in "Context" must
   cite real files (and lines where useful); mark anything unverified as an open question.
3. "Proposed changes": per crate, give the exact public types and signatures, data flow, and the
   tradeoffs considered with the reason for the choice. Respect the dependency rules in
   `AGENTS.md` (front-ends never depend on each other; `hikari_protocol` has no workspace deps).
4. Fix any user-facing or model-facing text (prompts, tool descriptions, error messages) verbatim
   in the spec rather than leaving it to implementation.
5. List new third-party dependencies with their purpose; prefer `default-features = false` and
   crates published at least 7 days ago.
6. "Testing and validation": a table mapping every product invariant number to a concrete test
   (unit file, fixture, golden, property, or manual step). No invariant may be left unmapped.
7. Call out hard edge cases explicitly (encoding, CRLF, Windows process behavior, cancellation,
   partial streams) and how each is handled.
8. Add Risks and Follow-ups. Keep scope to one milestone; move the rest to Follow-ups.
