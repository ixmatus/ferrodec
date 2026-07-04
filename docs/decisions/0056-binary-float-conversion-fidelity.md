# ADR-0056: Binary-float conversion fidelity and family unification

- **Status**: accepted
- **Date**: 2026-07-03
- **Note**: originally numbered ADR-0055; renumbered to 0056 on integration
  to resolve a collision with the concurrently-developed
  [ADR-0055](0055-decimal-ordering-and-float-string-ergonomics.md) (decimal
  ordering and float-string ergonomics), which reached `main` first. The
  fd-aqs.12 commits and their CHANGELOG history predate the renumber and
  still cite "ADR-0055".

## Context

The 2026-06-09 review (fd-aqs.12) found the `Decimal*` to and from binary
float conversions inconsistent across the three fixed formats and wrong on
the `INEXACT` flag in both directions.

`from_f64` diverged silently. The Decimal128 parent rendered the f64 with
the shortest round-trip form (`{value:e}`), so `from_f64(0.1)` gave `0.1`
with `OK`. The Decimal64 and Decimal32 siblings rendered `{:.17e}`, an
eighteen-significant-digit form, then re-rounded it into their sixteen or
seven digit coefficient. That is a double rounding (the 18 digit
intermediate rounds once, the format rounds again), and it made
`from_f64(0.1)` give `0.1000000000000000` with `INEXACT` on the siblings.

`to_f64` and `to_f32` decided `INEXACT` by convention, not by fact, and in
opposite directions. The Decimal128 string path raised `INEXACT`
unconditionally, so `Decimal128::ONE.to_f64` reported `INEXACT` on an
exact conversion. The Decimal64 and Decimal32 `to_f64` numerical path
(`coefficient as f64 × pow10_f64(exp)`) never raised `INEXACT` at all, and
for a Decimal64 coefficient above `2^53` (a sixteen digit coefficient
exceeds it) the `coefficient as f64` cast rounds, so the numerical path
also double-rounded the value. The Decimal32 `to_f64` docstring claimed
the conversion was always exact; `0.1` is a Decimal32 value that is not
exactly a binary64, so the claim was false.

`from_f64` also dropped the NaN sign on every format (always returning
`+NaN`), and the Decimal128 `from_f64` did not raise `INVALID` on a
signaling NaN operand, unlike the siblings.

## Decision

The family converges on one behavior, with the load-bearing numeric
predicate factored into `ferrodec-ieee` so all three siblings share it.

1. **`from_f64` renders shortest round trip on every format** (Parnell's
   call via AskUserQuestion, 2026-07-03): the siblings switch from
   `{:.17e}` to `{:e}`, matching the parent. `from_f64(0.1)` is now `0.1`
   with `OK` on all three, and the double-rounding hazard is gone. A value
   whose shortest form genuinely exceeds the format precision still rounds
   once, with `INEXACT`.

2. **`to_f64` and `to_f32` are correctly rounded and flag `INEXACT`
   exactly.** All six functions take the decimal-string path
   (`Display` then `str::parse`), which rounds once and correctly; the
   Decimal64 and Decimal32 `to_f64` numerical path is retired (and
   `pow10_f64` with it). The flag is decided by
   `ferrodec_ieee::decimal_is_binary_exact(coefficient, exponent,
   mantissa_bits)`, which tests whether `coefficient × 10^exponent` is
   exactly an `m × 2^k` with `m` under the significand: for `exponent ≥ 0`
   the odd part is `oddpart(coefficient) × 5^exponent`, for `exponent < 0`
   the value is dyadic iff `5^-exponent` divides the coefficient. Both
   loops terminate early without overflow. `binary_conversion_status`
   packages the overflow, underflow, and subnormal rules around it.

3. **`from_f64` preserves the NaN sign (§6.3) and raises `INVALID` on a
   signaling NaN (§5.4.2) on every format.** A negative NaN operand now
   yields a negative NaN; the Decimal128 path gains the sNaN `INVALID`
   the siblings already had.

## Consequences

These change observable behavior on the released 3.3.x surface, all as
corrections: sibling `from_f64` output and flags (the double-rounding
case), sibling `to_f64` values (the double-rounded coefficient case) and
their previously-absent `INEXACT`, the parent `to_f64` / `to_f32`
previously-spurious `INEXACT`, and `from_f64` NaN sign and the parent's
sNaN handling. Callers that pinned the old bit patterns or flags see the
corrected ones. The version bump is Parnell's at release.

`decimal_is_binary_exact` is exponent-range agnostic: it answers the
significand question only. The caller treats an infinite result as
overflow, a zero result (from a nonzero operand) as underflow, and a
*subnormal* result as conservatively `INEXACT`, because a subnormal float
carries fewer significand bits than the predicate models. This can only
under-claim exactness, never over-claim it, and no `Decimal128` value is
a representable binary subnormal exactly in any case (the exact decimal of
`2^-1074` needs 751 digits, far past 34).

Invert-first failure paragraph. The most plausible ways this is wrong:
(1) `decimal_is_binary_exact` says exact when the value is not, which
would raise no `INEXACT` on a lossy conversion (a false clean bill) — the
unit tests pin the boundary cases (`0.1`, `2^53 + 1`, `2^24 + 1`) and the
subnormal branch is conservative, so an error here fails toward `INEXACT`,
not toward silence; (2) the string path disagrees with the retired
numerical path on some value, surfacing as a differential or property
regression — this is intended (the string path is correctly rounded, the
numerical path was not); (3) a future `Display` widening overflows the
fixed buffer, caught by the defensive `write!` error path returning
`NaN + INVALID` rather than a wrong value.

`ferrodec-ieee` gains two public functions (`decimal_is_binary_exact`,
`binary_conversion_status`), an additive change; its next crates.io
release is a minor bump. Path dependencies pick them up without a bump in
the workspace build.

## References

- fd-aqs.12; the 2026-06-09 review report,
  `docs/archive/REPORT-rigorous-review-2026-06-09.md`.
- IEEE 754-2019 §5.4.2 (convertFormat), §6.3 (the sign of the result).
- `ferrodec-ieee/src/binary.rs`; `src/convert/binary.rs` and the sibling
  `convert/binary.rs`.
