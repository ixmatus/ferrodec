# ADR-0054: Context precision becomes NonZeroU32 (2.0.0)

- **Status**: accepted
- **Date**: 2026-06-11

## Context

`Context::new` documented "must be at least one" for the working precision
but never enforced it, and `precision` is a public field, so the documented
invariant was unenforceable even if the constructor had checked
(2026-06-09 review, fd-aqs.4). A precision of zero makes `round_finite`
drop every digit and return silent nonsense; no decTest vector exercises
precision zero, so nothing pinned the behavior.

Three shapes were on the table: a `NonZeroU32` field (illegal state
unrepresentable, breaking), a documented saturating `max(1)` inside
`round_finite` (non-breaking, invariant lives in a runtime clamp), and a
panicking constructor with a defensive clamp (stays 1.x, partial
function). House style orders these: types over runtime checks, total
functions over partial ones.

## Decision

`Context.precision` is `core::num::NonZeroU32` and `Context::new` takes
`NonZeroU32` (Parnell's call, 2026-06-11). The crate re-exports
`NonZeroU32` from the root for callers' convenience. Internal consumers
read `ctx.precision.get()`; no operation needs to handle zero because zero
cannot arrive.

This breaks the 1.0.1 public API (field type and constructor signature),
so ferrodec-decimal's next release is 2.0.0. The cost is low: 1.0.1 was
never published to crates.io, so no external consumer exists; every
in-workspace call site was migrated in the same change.

## Consequences

- A zero precision is now a compile-time impossibility instead of a
  documented promise; `round_finite` and every operation keep their
  contracts total without a defensive clamp.
- Call sites that previously wrote `Context::new(34, ...)` now write
  `Context::new(NonZeroU32::new(34).unwrap(), ...)` or hold a
  `const NonZeroU32`. The `unwrap` on a literal is const-evaluable and
  cannot fail at runtime.
- The decTest conformance harness converts the file-supplied `precision:`
  directive with `NonZeroU32::new(...).unwrap()`; a hypothetical vector
  with `precision: 0` would panic the harness rather than silently clamp,
  which is the desired loud failure for a malformed corpus.
- Inverted, the plausible failure modes: a missed `.get()` coercion site
  fails to compile (the point of the change); a third-party 1.x user is
  broken on upgrade (none exist; the major bump signals it anyway); and
  ergonomic friction pushes future test code toward a shared helper, which
  is acceptable test-local noise.

## Related

- Beads: fd-aqs.4 (2026-06-09 review finding 5c)
- Report: `docs/archive/REPORT-rigorous-review-2026-06-09.md`
- Other ADRs: ADR-0045 (the 1.0 API settle this supersedes in part),
  ADR-0053 (same remediation arc)
