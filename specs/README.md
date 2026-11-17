# Specs

Hikari is spec-driven: features are designed and reviewed before they are implemented.

## Flow

1. An issue is opened and, once a maintainer agrees on direction, labeled `ready-to-spec`.
2. A spec PR creates a directory here named `GH<issue-number>/` containing `product.md` and
   `tech.md`, based on the templates in `specs/_template/`.
3. Once the spec PR is merged and the issue is labeled `ready-to-implement`, code PRs implement
   the spec, one logical change per PR.

Specs written before the issue tracker is in use are named `NNN-<slug>/` instead.
`specs/000-foundation/` fixes the language, workspace layout, and engineering workflow every later
spec builds on; `specs/001-exec-mvp/` is milestone M1.

The `write-product-spec` and `write-tech-spec` skills in `.agents/skills/` scaffold new specs.

## Contents

- `product.md` — problem, goals, non-goals, and numbered, testable behavior invariants.
- `tech.md` — context, proposed changes, and testing/validation that maps each invariant to a
  test.
