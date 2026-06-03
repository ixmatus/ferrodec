# ADR-0040: Arbitrary-precision transcendentals for ferrodec-decimal

- **Status**: accepted
- **Date**: 2026-06-02

## Context

ADR-0038 names two gates between `ferrodec-decimal` 0.1.0 and a complete
specification surface: the static general decTest suite (delivered in ADR-0039)
and the four numerical transcendentals the General Decimal Arithmetic
specification defines, `exp`, `ln`, `log10`, and `power`. The specification has
no trigonometric or hyperbolic functions, so there is no pi and no Payne-Hanek
argument reduction, which removes the single hardest part of arbitrary-precision
elementary functions. `squareRoot` already shipped in 0.1.0.

The reference, CPython's libmpdec, computes these correctly rounded for `exp`,
`ln`, and `log10`, and "almost always" correctly rounded for `power`. The
fixed-width ferrodec-transcend kernels do not transfer: they are pinned to 50
working digits with a stored constant table and a 6300-digit Payne-Hanek table,
none of which generalize to unbounded precision. The crate already requires a
heap, so the embedded-latency argument that rejected Ziv's strategy for the
fixed formats (ADR-0032) does not bind here.

## Decision

Implement all four functions on a private variable-precision float and the
bounded Ziv strategy, derived fresh from the `atanh` and Taylor identities and
the specification's operation definitions (Muller, *Elementary Functions*, for
the range-reduction and error-budget framing).

**Working type.** `transc::work::Work` is a finite signed decimal float
`(-1)^sign * coeff * 10^exp` with a sticky bit, built only on the `DecBig`
coefficient primitives. Its `normalize_to` is the single point where a digit is
folded into the sticky bit, and only ever below the working width, so a later
`round_finite` sees the true round digit and a faithful sticky.

**Bounded Ziv, correctly rounded (`exp`, `ln`, `log10`).** Each kernel is a pure
function of a working precision and returns a value within a small, stated ulp
bound of the true value (carrying internal guard digits to meet it). The
strategy brackets the true value, rounds both bracket endpoints to the context,
and accepts the result when they agree; otherwise it doubles the guard and
re-runs, with a generous cap and a faithful fallback. This is correct at any
precision and is libmpdec's own technique, so the differential against it stays
cohort exact. A fixed wide guard cannot be *proven* sufficient at unbounded
precision (the table-maker's-dilemma hardest case is unknown), so a single-pass
fixed-guard mode, which would trade the correctly-rounded guarantee for lower
latency, is left as a documented future lever rather than wired here. `exp`,
`ln`, and `log10` round half-even regardless of the context's rounding mode,
matching `squareRoot` and the reference.

**Own oracle, correctly rounded (`power`).** `power` is `exp(y * ln|x|)` with an
exact integer fast path (binary exponentiation in `DecBig`, the reciprocal for a
negative exponent) and the full IEEE 754-2019 section 9.2.1 special-case table.
It is rounded with the context's rounding mode and is correctly rounded by
construction, validated against an independent high-precision oracle
(`tests/pow_oracle.rs`, the ADR-0026 pattern). Because libmpdec is only "almost
always" correctly rounded, the differential and the decTest conformance compare
`power` within a one-ulp band: on the rare hard input this crate is the stronger
of the two.

**Constants on demand.** `ln 2` and `ln 10` are computed at the requested
precision by a Machin-like `atanh` series (`ln 2 = 2*atanh(1/3)`, `ln 10 = 3*ln 2
+ 2*atanh(1/9)`), memoized per call in a `ConstCache` of optional `Work`s, with
no stored table and no global mutable state, so the surface stays `no_std`
clean.

## Consequences

The full specification surface is now implemented and validated. The differential
against libmpdec covers the four functions across four contexts and eight
rounding modes (`exp` / `ln` / `log10` cohort exact, `power` within one ulp), and
the static decTest suite vendors `exp`, `ln`, `log10`, `power`, `powersqrt`, and
the `rounding` / `inexact` flag tests, standing at 22938 pass, 0 fail, 99 skip.

Two skip categories were added to the conformance runner, both restrictions of
the reference that this crate deliberately does not impose: `Invalid_context` (a
precision or exponent bound beyond decNumber's internal limits) and the
`DEC_MAX_MATH` operand range (an operand whose adjusted exponent leaves
`[-1999997, 999999]`). This crate places no such ceiling and computes the
mathematically correct result within its `i32` exponent.

**Named residual exposures.** Two, both bounded and documented:

- The bounded Ziv cap. If an input ever exceeds the iteration cap (an
  astronomically unlikely table-maker's-dilemma case), the result falls back to a
  faithful rounding, wrong by at most one ulp, never silently claimed correct.
- The `power` one-ulp band against the reference. This crate's `power` is
  correctly rounded; the divergence is the reference rounding the other way on a
  hard input, or its far-underflow returning a zero where the correctly rounded
  round-away result is the smallest subnormal.

**Performance follow-up (deferred, not a 1.0 blocker).** `DecBig` uses schoolbook
`mul` and Knuth Algorithm D `div_rem`, with no Newton division, Karatsuba, or FFT.
The `ln` log1p series runs in roughly `O(wp^3)`, and the Ziv re-runs multiply
that; the cost is acceptable at the conformance and differential precisions
(at most 34 digits) but grows at high precision. The named levers are Newton
reciprocal division, Karatsuba multiplication, and a Brent-McMillan `ln`. A
single-pass fixed-guard rounding mode, trading the correctly-rounded guarantee
for lower latency, is a further lever where latency matters more than the last
ulp.

## Related

- Plan: `plans/spicy-dazzling-deer.md`
- Builds on ADR-0038 (the 1.0 gates and the `Work` design), ADR-0039 (the decTest
  runner this phase extends), ADR-0026 (the independent-oracle pattern), and
  ADR-0032 (the fixed-format correctly-rounded contract, whose embedded-latency
  rejection of Ziv does not bind a heap crate).
- Issues: `fd-rd0.5` through `fd-rd0.10` (the transcendental slices), `fd-7la`
  (`to_eng_string`, closed earlier in the arc).
