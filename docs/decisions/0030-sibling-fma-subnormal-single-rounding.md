# ADR-0030: decimal64 / decimal32 FMA subnormal single-rounding

- **Status**: accepted
- **Date**: 2026-05-18

## Context

The fd-dpg sibling exact correctly-rounded FMA oracle sweep
(`property_fma_oracle.rs`), the standing regression guard ADR-0022
left active, surfaced a fourth defect in the sibling FMA path, on a
shape ADR-0022 did not reach.

`round_and_pack_finite` rounded the exact coefficient to PRECISION
digits, and then `finalise_finite`'s deeply-subnormal `biased < 0`
arm rounded the already-rounded value a second time into the
subnormal quantum. When a residue landed strictly above the subnormal
rounding boundary but below the PRECISION boundary, the first rounding
truncated it to a value that the second rounding then saw as an exact
tie, and round-half-even carried it the wrong way.

The decimal64 minimal failing input was
`fma(2.064141013983096e-361, 8.386823222860694e-24, +0e+113)` under
NearestEven. The exact product is
`1.7311585791332650034144478828624e-384`; at the subnormal quantum
`1e-398` the true residue is `…326.50034…`, strictly above the tie,
so the single correctly-rounded result is `…327e-398`. The old path
rounded the 19-digit value to 16 digits first (`…265e-399`), then
re-rounded into the subnormal quantum where `…326.5` is an exact tie
and ties-to-even down to `…326e-398`. The decimal32 analogue,
`fma(3.142290e-17, -2.033196e-78, 5.38890e-95)` NearestEven, gave
`-9.99992e-96` for the correctly-rounded `-9.99991e-96`.

Both siblings carried the defect; it is the FMA-defect-family pattern
of ADR-0018/0019/0020/0022 (the siblings inherit a parent
`Decimal128` rounding defect). The parent had already been corrected:
its `round_and_pack_finite` carries the fd-42l single-rounding
restructure. The siblings were never given the fd-42l analogue;
ADR-0022 ported only the alignment-tail restructure and the decimal32
UNDERFLOW rule, not the subnormal single-rounding.

This is double rounding. The IEEE 754-2019 contract for `fma` is a
single rounding of the exact `a·b + c`. Any architecture that rounds
to the format's normal precision and then re-rounds into the
subnormal range violates that contract whenever the discarded residue
straddles the two rounding boundaries.

## Decision

Port the parent `Decimal128` fd-42l single-rounding restructure to
both siblings' `round_and_pack_finite` (`src/ops/round.rs`), adapting
the parent's `U256` arithmetic to the siblings' `u64` working
coefficient:

- Compute the drop in one step. `qmin = -BIAS`,
  `precision_excess = digits.saturating_sub(PRECISION)`,
  `subnormal_excess = max(0, qmin - unbiased_exp)`,
  `excess = max(precision_excess, subnormal_excess)`. Drop `excess`
  digits once, so the rounding decision sees the full residue. This
  leaves `finalise_finite`'s `biased < 0` arm unreachable for
  non-zero results (it survives as a provably-dead backstop and the
  zero path, matching the parent).
- Detect tininess on the pre-rounding value
  (`tiny_pre = digits + unbiased_exp - 1 < E_MIN`) and raise UNDERFLOW
  on an inexact tiny result there. The post-rounding tininess check
  keys on the rounded digit count, which misses a subnormal value
  that rounding lifts to the Emin boundary; the parent moved the
  decision pre-rounding for the same reason (dqfma2908 / dqmul908).
- Floor the preferred-quantum down-shift target at `qmin`
  (`down_target = max(q_preferred, qmin)`): padding toward a quantum
  below the minimum is not representable.

The siblings deliberately keep the simple `u64` `drop_excess_digits`
loop rather than porting the parent's `round_digit_for_full_drop`
short-circuit. That helper exists in the parent only to bound `U256`
`div_rem10` cost when `shift` can reach the thousands; the sibling
loop's worst case is `excess ≈ BIAS` (≤ 398) cheap `u64` divisions,
and the loop already computes the correct `(round_digit, sticky)` for
`excess ≥ digits`. Adding the helper would be unmotivated
divergence-for-divergence.

These ship within the existing sibling crates as a correctness
remediation; no API surface changes, no version bump beyond the
normal release engineering.

## Consequences

The siblings now round `fma` once, the IEEE 754-2019 single-rounding
contract by construction, across the full finite domain and every
rounding direction. The fd-dpg exact-oracle sweeps are green for both
crates at 300000-case release runs (raised local-reject budget; the
`finite` strategy filter, not any disagreement, is what caps the
default-budget run). The per-file decTest conformance counts are
unchanged (ADR-0010 exact-match guard green for both crates), the
full sibling test suites pass, and the fmt / clippy `-D warnings` /
rustdoc `-D warnings` gates are clean.

Provenance is clean: the single-rounding restructure is derived from
the corrected parent `Decimal128` `round_and_pack_finite`, not from
recall, the same direction ADR-0022 took for the decimal32 UNDERFLOW
rule. The durable artifacts are `tests/regression_fd_dc6.rs` (both
crates, the discovered shapes pinned bit-for-bit and
status-for-status against the exact oracle for every mode), the
persisted `property_fma_oracle.proptest-regressions` seeds, and the
standing exact-oracle sweeps.

This ADR does not supersede ADR-0018/0019/0020/0022; it completes the
sibling port of the parent fd-42l single-rounding fix, the one member
of the parent FMA-rounding-defect family ADR-0022 left unported,
where the exact oracle, not a tolerance envelope, is the arbiter.

## Related

- Issues: fd-dc6 (discovered-from fd-dpg, the standing exact-oracle
  sweep ADR-0022 left active)
- Other ADRs: parent fd-42l single-rounding (referenced in ADR-0022);
  sibling FMA remediation ADR-0022; static-window family
  ADR-0018/0019/0020; exact-oracle contract ADR-0021; testing
  strategy ADR-0010
