# ferrodec-decimal64 known issues

Tracks bugs and coverage gaps Slice D of the 1.15 cycle surfaced
through the conformance dispatcher (see
`docs/decisions/0017-decimal64-conformance-coverage-gap.md` in the
workspace root). Each entry names a reproducible decTest case so the
fix slice has a one-line repro to start from.

## Open: finite-finite addition magnitude loss

* **Status**: open. Reproducible on Decimal64 1.2.0 and current main.
* **Reproducer**: `ddAdd.decTest:358` (case `ddadd360`).
* **Symptom**: ferrodec's `Decimal64::add(...).0` returns
  `0E+50` (bits `0x2300000000000000`); spec expects `1.0000E+5`
  (the value 100,000, bits `0x223C000000002710`).
* **Likely cause**: the alignment / sticky-bit logic in the
  finite-finite path drops the result coefficient on a specific
  exponent-difference edge. Same bug class as decimal128's
  `parse_str` long-mantissa magnitude-loss H-tier finding from the
  2026-05-09 six-agent correctness review (which was fixed in 1.13.x
  for decimal128).
* **Fix landing**: dedicated decimal64 correctness slice, post 1.15
  cycle.

## Open: rounding direction at the precision boundary

* **Status**: open. Reproducible on Decimal64 1.2.0 and current main.
* **Reproducer**: `ddAdd.decTest:802..821` (20 sequential cases
  `ddadd71100`, `ddadd71101`, ..., `ddadd71119`). The negated mirror
  exists around case `71200..71219`. Total: at least 40 cases in
  `ddAdd` alone hit this; similar patterns probably exist in `ddSubtract`
  and `ddMultiply` (1 confirmed failure in `ddMultiply.decTest`).
* **Symptom**: under the `half_even` rounding directive (which the
  whole `dd*` suite mostly runs under), an exact result `99.999…9`
  (16 nines) at the 16-digit precision boundary rounds to
  `100.0…0`. The spec expects the 16-digit `99.999…9` representation.
  Sign-mirrored cases produce `-100.0…0` where `-99.999…9` is
  expected.
* **Likely cause**: a round-half-up rule firing in place of
  round-half-to-even at the exact-tie ULP. Same bug class as
  decimal128's FMA sub-ULP directional H5 finding from the
  2026-05-09 review (fixed in 1.13.x for decimal128).
* **Fix landing**: dedicated decimal64 correctness slice.

## Open: pack_finite biased_exp precondition panic in FMA

* **Status**: open. Reproducible on Decimal64 1.2.0, 1.3.0, and
  current main. Pinned 2026-05-11 in Phase 0 of the decimal64
  correctness slice.
* **Reproducer**: `ddFMA.decTest:281` (case `ddfma2504`). Operands:
  `fma 0E-260 1000E-260 0E+384`. Spec answer: `0E-398` with the IEEE
  754 informational `Clamped` condition raised.
* **Symptom**: debug builds panic at
  `ferrodec-decimal64/src/bid.rs:216` with
  `assertion failed: biased_exp <= BIASED_EXP_MAX`. Release builds
  compile the `debug_assert!` out; `pack_finite` continues with the
  out of range biased exp, packs garbage bits, and the conformance
  run reports
  `got "9.007199254740992E-249" want "0E-398"`.
* **Mechanism**: the FMA path computes the ideal exponent for a zero
  product result by summing the product operand exponents
  `(-260) + (-260) = -520` and taking the minimum with the addend
  exponent `+384`, leaving `-520`. The conversion to biased form
  then computes `biased_exp = -520 + 398 = -122` and casts to `u32`,
  which wraps to a value far above `BIASED_EXP_MAX = 767`. The spec
  requires clamping the ideal exponent to the representable range
  `[-398, +369]` for decimal64 *before* the biased conversion.
* **Fix shape**: the slice plan's "H3 fix shape (constraint)"
  subsection in
  `docs/decisions/plans/2026-05-11-decimal64-correctness.md`
  requires lifting the invariant out of debug runtime. Preferred is
  a typed `BiasedExp` newtype whose constructor proves the range;
  fallback is a release safe check (saturating clamp, or hard
  `assert!`). No `debug_assert!` on input derived arithmetic
  survives the slice.
* **Fix landing**: dedicated decimal64 correctness slice, Phase 2 H3.

## Coverage gap: conformance dispatcher is `Apply` / `tosci` only

* **Status**: by design until the three correctness bugs above are
  fixed. See ADR-0017.
* **Symptom**: 38 of the 43 `dd*.decTest` files in
  `tests/vectors/` report 0 passes — every case routes through
  `run_case` as `Outcome::Skip` because `dispatch_op` only knows
  `tosci` / `apply`. This understates Decimal64's actual surface
  coverage (the methods exist and run from unit tests and the
  property suite), but it accurately reflects the *spec-conforming*
  surface.
* **Closing this gap**: enable each `OpKind` arm as its underlying
  ops' bugs close. The expansion lives in the decimal64 correctness
  slice's commit cadence.
