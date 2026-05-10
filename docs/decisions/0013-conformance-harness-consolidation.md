# ADR-0013: Conformance harness consolidation across the ferrodec family

- **Status**: accepted
- **Date**: 2026-05-10

## Context

After Phase D-1 (the `ferrodec-ieee` extraction), three concrete
consumers of the ferrodec testing pattern exist:

- `ferrodec` — `tests/conformance.rs`, 1075 lines. Carries the
  full IEEE 754-2019 §5 dispatch, plus DPD interchange, plus the
  `up` directive's two-pass emulation, plus per-case timeout
  guards. Evolved over ~14 minor releases.
- `ferrodec-decimal32` — `tests/conformance.rs`, 469 lines. Adapted
  from Decimal128's at sibling-creation time; covers `tosci` /
  `apply` only, pending wiring of further dispatch arms.
- `ferrodec-decimal64` — `tests/conformance.rs`, 475 lines. Same
  shape as ferrodec-decimal32's, with type names swapped and the
  per-file expectation table retargeted.

Phase D-3 in the original plan asked whether to consolidate the
shared harness machinery into `ferrodec-test-support` (the
companion to `ferrodec-ieee`, also workspace-internal) or leave
the copies as-is.

Diff analysis between the two siblings shows ~89% similarity (52
lines diverge out of ~470, almost entirely type-name
substitutions, per-file expectation tables, and prose). The IBM
decTest parser, directive accumulator, expectation guard, and
file-walking driver are byte-identical between Decimal32 and
Decimal64.

Diff analysis between the siblings and Decimal128 shows genuine
divergence: Decimal128's Context carries an `Encoding` field
(`Bid` / `Dpd`) the siblings don't yet need, and its
`CaseRounding` enum distinguishes the IEEE set from the decTest
extras (`up`, `half_down`, `05up`) where the siblings handle this
through `map_rounding(...)? else Skip`. These differences are
load-bearing for Decimal128's mature dispatch surface.

## Decision

Two-tier consolidation:

1. **Sibling pair (Decimal32 + Decimal64)**: factor the shared
   ~430 lines (parser, Context with format-default constructors,
   Outcome / TestCase / Failure / Totals types, `run_suite`
   driver, `map_rounding` / `decode_conditions` helpers) into
   `ferrodec-test-support::conformance`. Each sibling's
   `tests/conformance.rs` shrinks to ~120 lines: dispatch
   closure, type-specific `parse` / `format`, per-file
   expectation table, and a one-line `run_suite(...)`
   invocation.

2. **Decimal128**: leave `ferrodec/tests/conformance.rs`
   unchanged. The divergence (extra Context fields, the
   `CaseRounding` enum, ~3× larger dispatch coverage) is
   genuine: migrating Decimal128 onto the shared scaffold would
   require either widening the shared Context type (which would
   make the siblings' harness carry fields they don't use) or
   wrapping every dispatch through a generic-over-context
   adapter (which would obscure the per-precision dispatch the
   harness exists to expose). The deduplication win for a single
   consumer doesn't justify the abstraction cost.

The asymmetry is a deliberate choice: the siblings benefit
because they're near-clones of each other, and Decimal128's
mature copy stays put because it carries detail the siblings
don't yet need. If Decimal64 (or a future Decimal16) grows toward
DPD interchange and the `up` directive, the right move is to
*revisit* this ADR — either by lifting the Decimal128-shape
machinery into `ferrodec-test-support` (Decimal128 then migrates
too) or by leaving each precision tier self-contained.

## Consequences

**Wins.**

- Decimal32 and Decimal64 harnesses lose ~340 lines each of
  duplicated machinery — ~680 LOC total deletion.
- A bug in the parser, expectation guard, or file walker fixes
  itself across both siblings simultaneously.
- New siblings (a hypothetical Decimal16 that lands later) start
  with a ~30-line harness shell.

**Costs.**

- One more workspace dev-dep edge for the siblings.
  `ferrodec-test-support` is `publish = false`; downstream users
  never see it.
- Reading a sibling's harness now requires jumping into
  `ferrodec-test-support` for the parser body. Mitigated by
  keeping the shared crate's surface well-documented.

**Non-consequences.**

- Conformance pass / fail counts are unchanged. The asymmetric
  per-file expectation guard still fires on regressions per
  ADR-0010.
- ferrodec (Decimal128) consumers see no change. Its harness
  binary, conformance counts, and expectation tables are exactly
  as they were in 1.14.4.

## Related

- Plan: `plans/2026-05-09-workspace-and-decimal-siblings.md`
  (Phase D, step D3).
- ADR-0010: testing strategy after the 6-agent correctness
  review — defines the asymmetric per-file expectation guard
  this consolidation preserves.
- ADR-0012: `ferrodec-ieee` extraction — same shape (the "stand
  alone first" principle, applied to test-support rather than
  IEEE metadata).
