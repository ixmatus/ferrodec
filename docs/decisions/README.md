# Architecture Decision Records

This directory holds the record of *why* ferrodec is the way it is. Each significant choice — feature scope, encoding, verification posture, performance tradeoffs — gets one Architecture Decision Record (ADR). Together they form the audit log a future reviewer would otherwise have to reconstruct from commit messages and release notes.

## Conventions

- **Filenames**: `NNNN-short-slug.md`, four-digit zero-padded sequence number, lowercase slug. Numbers are never re-used; superseded ADRs keep their slot and link forward.
- **Format**: see `template.md`. Each ADR is short — a single page is the target. The form is more important than the length.
- **Status lifecycle**:
  - `proposed` — drafted, not yet acted on. Avoid this for retroactive ADRs.
  - `accepted` — the decision is in effect.
  - `superseded by ADR-NNNN` — replaced; keep the file as a historical record, link forward.
  - `rejected` — considered and decided against. Document for the next person who wonders the same thing.
- **Plans**: approved planning artifacts (the inputs to /plan output) archive under `plans/` with a date prefix (`YYYY-MM-DD-slug.md`). They're snapshots — the *state at decision time*, not living documents. ADRs reference the plan that produced them when applicable.

## Writing a new ADR

1. Pick the next available number.
2. Copy `template.md` to `NNNN-your-slug.md`.
3. Fill in: status, date, context, decision, consequences, related references.
4. If the decision supersedes a prior one, edit the prior ADR's status line to `superseded by ADR-NNNN`.

Decisions that are reversible or local in scope don't need an ADR — these are for choices that matter to future contributors deciding whether to revisit a path.

## Index

The ADRs in number order:

- [0001 — BID-128 over DPD-128](0001-bid-over-dpd.md)
- [0002 — Per-op `(value, Status)` over global flag word](0002-per-op-status.md)
- [0003 — Method-only API; `core::ops` opt-in via feature flag](0003-method-only-api.md)
- [0004 — Skip Verus pilot graduation](0004-skip-verus-graduation.md)
- [0005 — Will-not-fix `half_down` / `05up` rounding directives](0005-half-down-05up-wontfix.md)
- [0006 — Defer wholesale perf optimization until profile data exists](0006-defer-perf-pass.md) *(superseded by 0007)*
- [0007 — Performance baseline (1.10.1 + bench expansion)](0007-perf-baseline.md)
- [0008 — Performance pass results (1.11.0)](0008-perf-results.md)
- [0009 — DPD interchange behind the `dpd` feature (1.12.0)](0009-dpd-interchange.md)
- [0010 — Testing strategy after the 6-agent correctness review](0010-testing-strategy-after-six-agent-review.md)
- [0011 — Cargo workspace for sibling decimal-precision crates](0011-workspace-for-decimal-siblings.md)
- [0012 — Extract `ferrodec-ieee` after three concrete consumers](0012-extract-ferrodec-ieee.md)
