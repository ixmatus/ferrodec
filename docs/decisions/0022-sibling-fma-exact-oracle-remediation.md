# ADR-0022: decimal64 / decimal32 FMA exact-oracle remediation

- **Status**: accepted
- **Date**: 2026-05-17

## Context

The faithfulness-remediation engagement ported the parent
`Decimal128` exact correctly-rounded FMA oracle sweep to both siblings
(`property_fma_oracle.rs`, fd-dpg) but quarantined the bug-finding
property `#[ignore]`d under fd-9fi: the sweep proved the siblings
carried the parent `fd-7nf` defect family, with the kernel fix
deferred. Un-ignoring the sweep and making it green is the fd-9fi
deliverable.

The exact oracle is the arbiter. Un-ignored, it surfaced not one
defect but three distinct ones in the sibling FMA path, two
pre-existing and independent of the originally filed shape. Each was
reproduced before fixing and pinned against the oracle.

1. **fd-9fi (the filed shape).** A tiny opposite-sign product with a
   dominant same-sign addend under a directed mode produced a gross
   magnitude error, not one ULP. `fma(1e-398, -1e-398, -1e+114)`
   `TowardNegative` returned `-2e114` instead of the correctly-rounded
   `-1.000000000000001e+114`; the decimal32 analogue is
   `fma(-1e-101, 1e-101, -1e+27)` → `-2e27` versus `-1.000001e+27`.
   The genuine-sub-ULP early-return passed the dominant operand to the
   funnel at its own coarse quantum, so the directed round-up landed
   one ULP at `10^114` rather than at the precision LSB `10^99`,
   doubling the magnitude. The effective-subtraction path was immune
   because `h2_borrow_and_extend` already re-cohorts the dominant
   operand to the `u128` digit cap; the effective-addition path had no
   such treatment. This is the effective-addition analogue of the
   parent `fd-7nf` static-window family (ADR-0018/0019/0020).

2. **Overlap misclassification (pre-existing, distinct).** The
   early-return dominance test gated solely on whether aligning a side
   to `target_q = min(ab_exp, c_exp)` overflowed `u128`
   (`shift > safe_shift`), never on genuine precision overlap. A
   product whose magnitude reached into the addend's kept window but
   whose natural quantum dragged `target_q` below the `u128` alignment
   bound was discarded into a single sticky bit.
   `fma(9.007199254740992e+19, 5.629499534213120e-160,
   5.629499534213120e-127)` returned `c` unchanged where the oracle
   adds the product's overlap (`…213120` → `…213627`), roughly 500 ULP
   folded away.

3. **decimal32 subnormal UNDERFLOW (pre-existing, decimal32-only).**
   `round_and_pack_finite`'s deeply-subnormal `biased < 0` arm gated
   UNDERFLOW only on its own digit drop, ignoring an INEXACT the
   caller had already recorded from an earlier precision rounding.
   `fma(-5.738903e-42, 5.487024e-55, -0e-101)` (zero addend, rebased
   subnormal product) signalled Inexact only; IEEE 754-2019 §7.5
   requires Underflow Inexact Subnormal. decimal64 already carried the
   `fd-99f` / M1 pre-rounding-tininess rule; decimal32 lacked it.

A decimal32 unit test, `fma_h4_same_sign_additive_control_no_regression`,
asserted `fma(1E+45, 1E+45, 1E-101)` `TowardPositive` →
`2.000000E+90`. That expectation was the pre-fix doubling bug itself
(the directed round-up applied to the bare 1-digit coefficient at its
own `10^90` quantum). The correctly-rounded value is `1.000001E+90`;
the exact oracle confirms it.

## Decision

Restructure the sibling FMA alignment tail (decimal64 and decimal32,
`src/ops/fma.rs`) into three explicit branches, replacing the two
`shift > safe_shift` early-returns:

- **Both sides fit at `target_q`.** Exact `u128` sum, no residue,
  funnel rounds to PRECISION only. Unchanged behaviour.
- **Magnitude gap ≥ `WORK_DIGITS` (= `2 × PRECISION`).** The
  lower-magnitude side is a genuine sub-ULP residue. Take the dominant
  value plus sticky, classified by magnitude top rather than by which
  alignment overflowed. `h2_borrow_and_extend` still carries the
  effective-subtraction one-ULP borrow (H2 mirror); the new
  `extend_to_u128_cap` re-cohorts the dominant operand on effective
  addition so the directed round-up lands at the precision LSB
  (fd-9fi). H4 preferred-quantum threading is preserved.
- **Overlap (gap < `WORK_DIGITS`, not both-fit).** Raise a working
  quantum `q_work = max(target_q, hi_top − (WORK_DIGITS − 1))` so both
  sides fit, retaining `WORK_DIGITS` digits of the dominant magnitude
  and folding only the genuinely sub-precision tail of each side into
  the sticky bit (`align_to_quantum`), then the real sign-aware
  combine. `WORK_DIGITS = 2 × PRECISION` keeps `PRECISION` guard
  digits beyond the rounded result, far more than the single round
  digit a correct rounding needs, while `WORK_DIGITS + 1` (the combine
  carry) stays inside the `u128` digit cap.

Port the decimal64 `fd-99f` / M1 rule into decimal32
`round_and_pack_finite`: signal UNDERFLOW when a subnormal result is
inexact from either this arm's own drop or an incoming INEXACT, in
both the deeply-subnormal `biased < 0` arm and the representable
subnormal final arm.

Correct the decimal32 unit test to the oracle-true `1.000001E+90` and
record why the prior expectation was the bug.

These ship within the existing 1.4.0 sibling crates as a correctness
remediation; no API surface changes.

## Consequences

The overlap path is the general correct alignment; the two surviving
early-returns are now provably-safe special cases (a side more than
`2 × PRECISION` digits below the other is genuinely below the round
position, so collapsing it to sticky is exact). The exact common path
fires whenever both sides fit, which is the IEEE 754 single-rounding
contract by construction. The per-file conformance counts are
unchanged (ADR-0010 exact-match guard green for both crates), the full
workspace test suite and Kani harnesses are green, and the now-active
exact-oracle sweeps stay green across five 20000-case multi-seed runs
per crate (the fd-42l non-determinism discipline).

Provenance is clean: the decimal32 UNDERFLOW rule is derived from the
corrected decimal64 sibling, not from recall; the overlap strategy is
derived from the rounding contract. The durable artifacts are
`tests/regression_fd_9fi.rs` (both crates, every discovered shape
pinned bit-for-bit and status-for-status against the exact oracle),
the persisted `property_fma_oracle.proptest-regressions` seeds, and
the now-active sweeps as standing regression guards.

This ADR does not supersede ADR-0018/0019/0020; it completes the
static-alignment-window correction in the siblings where the exact
oracle, not a tolerance envelope, is the arbiter, and folds in two
adjacent pre-existing FMA defects the same instrument surfaced.

## Related

- Issues: fd-9fi (filed); the overlap and decimal32-UNDERFLOW defects
  discovered-from fd-9fi during remediation
- Other ADRs: sibling completion of ADR-0018 (decimal64) /
  ADR-0019 (decimal32) / ADR-0020 (Decimal128); testing strategy
  ADR-0010; exact-oracle contract ADR-0021
- Engagement: faithfulness-remediation follow-up
  (`memory/project_faithfulness_remediation.md`)
