# ADR-0020: Decimal128 FMA sub-ULP trigger keys on the buffer bound, not a raw shift

- **Status**: accepted
- **Date**: 2026-05-16

## Context

A `cargo test --workspace` run during the decimal32 correctness
slice persisted a shrunk `property_fma_oracle` counterexample for the
parent `Decimal128`. It was scoped out of that slice (the parent
crate was untouched) and filed as fd-oaa for its own triage. The
operands reduce to `fma(1e33, 1, 3.0)` where the `3.0` sits in the
cohort `coefficient 3000000000000000, exponent -15`.

The exact value `10^33 + 3` is `1000000000000000000000000000000003`,
exactly 34 significant digits. Decimal128 carries 34. The correctly
rounded result is therefore that value with no rounding and status
`OK`. Released `ferrodec` 1.15.0 instead returned
`1000000000000000000000000000000000` and raised `INEXACT`: a 3 ULP
error on an exactly representable result plus a false flag. No
property tolerance argument rescues that, so the question "kernel bug
or envelope too tight" resolves to kernel bug.

The FMA kernel aligns the product and the addend into a `U384`
buffer, accumulates exactly, then rounds once. A sub-ULP path
collapses the smaller summand to a sticky bit when alignment would
overflow the buffer. The trigger was a disjunction of a dynamic
clause (`digit_count + shift > 110`, the real buffer bound) and a
static clause (`shift_ab > 47`, `shift_c > 82`). The product
coefficient of `1e33 × 1` is the single digit `1`. The addend's deep
cohort drags `target = min(qab, qc)` down, so `shift_ab` reaches 48
and the static clause fires even though the exact aligned sum is only
49 digits, far inside the buffer. Control diverts to the sub-ULP path
and the addend's value is discarded.

This is the parent-crate instance of the static-alignment-window
anti-pattern already corrected in decimal64 (ADR-0018) and decimal32
(ADR-0019). The corrected decimal64 kernel admits the analogous case
through its normal path on the dynamic rule
`digit_count(ab_coef) + shift_ab ≤ 38`, with the regression test
`fma_far_exponent_with_small_product_does_not_drop_c`.

An empirical scope gate (`tests/regression_fd_oaa.rs`) established
that the parent `Decimal128` `add` and `mul` paths are sound; the
defect is FMA-only and cohort-triggered. The slice scope is
therefore the FMA kernel alone, decided focused rather than a full
six-agent re-review (the defect family is already mapped by ADR-0018
and ADR-0019).

## Decision

Remove the static raw-shift disjuncts from the FMA sub-ULP trigger.
The sub-ULP path is needed precisely when aligning into the `U384`
buffer would overflow it, which the dynamic grown digit count already
expresses. The trigger becomes `cab_grown_digits > BUFFER_DIGIT_LIMIT`
and `cc_grown_digits > BUFFER_DIGIT_LIMIT`, where `BUFFER_DIGIT_LIMIT`
is 110 (the `U384` capacity of about 115 digits less carry headroom).
The renamed constant replaces the old `SHIFT_LIMIT`. The helper
precondition comments are re-derived from the capacity bound rather
than the deleted constant.

Ship as `ferrodec` 1.15.1, a patch release: the change corrects
output for inputs that previously produced a wrong value, and adds no
API surface.

## Consequences

The sub-ULP path now fires strictly less often, and only when the
smaller operand is genuinely far below one ULP: `ab_too_wide` implies
`qab − qc > 110 − digit_count(cab) ≥ 42` and `c_too_wide` implies
`qc − qab > 110 − digit_count(cc) ≥ 76`, both well beyond
`PRECISION = 34`. The helper preconditions hold with a wider margin
than the deleted constant claimed. Cases that no longer divert flow
through the exact `U384` accumulate plus single round, which is the
IEEE 754 single-rounding contract by construction, so they are at
least as correct as before. The dqFMA near-tie conformance cases that
previously routed through the opposite-sign helper now take the exact
common path; the per-file conformance counts are unchanged, verified
by the exact-match guard in `tests/conformance.rs`.

The fix carries clean provenance: it is derived from the corrected
sibling crates' admission rule, not from the buggy ancestor or from
recall. The full gate is green, including 67 Kani harnesses and the
property oracle. The cost is one widened code path to reason about;
the durable artifact is the regression file
`tests/regression_fd_oaa.rs` (the reproducer, the empirical scope
gate, the boundary sweep, and both genuine sub-ULP non-regression
guards) plus the findings doc.

This ADR does not supersede ADR-0018 or ADR-0019; it is the
Decimal128 analogue, completing the static-alignment-window
correction across all three precisions.

## Related

- Plan: `plans/2026-05-16-fd-oaa-decimal128-fma.md`
- Findings: `plans/2026-05-16-fd-oaa-decimal128-fma-findings.md`
- Commits: Phase 1 (reproducer plus scope gate), Phase 2 (the fix),
  Phase 3 (regression breadth), Phase 4 (this ADR plus the 1.15.1
  release)
- Issues: fd-oaa
- Other ADRs: Decimal128 analogue of ADR-0018 (decimal64) and
  ADR-0019 (decimal32); testing strategy ADR-0010
