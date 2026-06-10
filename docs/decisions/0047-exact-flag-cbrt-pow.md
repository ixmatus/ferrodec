# ADR-0047: Exact-result detection suppresses spurious INEXACT on cbrt and pow

- **Status**: accepted (amended 2026-06-10, fd-aqs.8: extended to exp2 / log2 / log10; see Amendment)
- **Date**: 2026-06-04

## Context

IEEE 754-2019 §7.5 raises INEXACT exactly when the delivered result differs from
the infinitely precise result. The shared `ferrodec-transcend` kernel evaluates
every §9.2 transcendental at 50-digit `Extended` precision and rounds once at the
format boundary; `exp_from_extended` raises INEXACT unconditionally on that final
step. For `exp`, `ln`, and the trigonometric and hyperbolic families that is
correct: the true result is irrational for every non-special input, and the
special inputs short-circuit before the rounding step. `cbrt` and `pow` are the
two exceptions. A perfect cube root (`cbrt(8) = 2`) and an exact integer or
rational power (`pow(10, 300) = 1E+300`, `pow(4, 0.5) = 2`) are themselves
representable, so §7.5 forbids the flag, yet the kernel raised it anyway. The
extended approximation almost never lands exactly, so the round step's own inexact
bit reflects the 50-to-format-width narrowing rather than whether the true result
is irrational; it cannot be trusted to answer the §7.5 question. The defect was
recorded in `KNOWN_ISSUES.md` and tracked as `fd-92w.8`. The integer fast path
already returned a clear status for small exact integer powers, so only the
general `exp(y · ln|x|)` path carried the spurious flag.

The kernel crate is `no_std` and alloc-free for the Cortex-M0+ floor, so the fix
cannot reach for a growable bignum (`DecBig`); it must stay inside the fixed-width
`U256` / `U384` integer primitives. Correctness of any exact test rests on the
kernel being correctly rounded (ADR-0032): if the infinitely precise result is
representable, correct rounding delivers it exactly.

## Decision

Add a `pub(crate)` `exact` module to `ferrodec-transcend` that proves a delivered
`cbrt` / `pow` result is exact, and have the two kernels clear INEXACT only when
the proof holds. The module is built around one invariant: it defaults to "not
proven" and clears the flag only on a positive proof. Any coefficient that would
exceed the fixed-width envelope, any exponent that would overflow `i32`, and any
rational denominator past `u32` all bail to "not proven". The only outcome the
bounds rule out is a false positive (clearing a real INEXACT); a false negative
merely leaves today's spurious flag in place, which is harmless.

- `cbrt`: the result is exact when its canonical coefficient cubed reproduces the
  input value exactly. A perfect cube root of a value with at most 34 significant
  digits has at most 12, so the cube fits `U256` and a wider coefficient bails.
  This is complete: it clears the flag for every perfect cube on every format.
- `pow`: write `|y| = a / b` in lowest terms from the exponent's coefficient and
  quantum, cancelling common factors of 2 and 5 without forming `10^d`. The result
  is exact when `|result|^b == |x|^a` for `y > 0`, or `|result|^b · |x|^a == 1`
  for `y < 0` (since `result = x^{-a/b}`). Both powers are computed by
  bounds-checked square-and-multiply on the canonical coefficients; a base above
  one outgrows the envelope quickly and bails, while a power-of-ten base (canonical
  coefficient 1) stays small for any exponent. This covers every representable
  exact power, integer or rational, positive or negative exponent, that the bounded
  envelope can witness.

The kernels clear INEXACT (and INEXACT only) when the proof holds and the result
is finite and non-overflowing. `UNDERFLOW` is left untouched (see Consequences).

## Consequences

The transcendental family now matches §7.5 on its two exact-capable operations on
all three formats from a single shared fix. The value is never touched, so every
value-based corpus, exhaustive, and conformance test is unchanged; only the flag
moves. The proof is sound by construction: it can only ever clear a genuinely
spurious flag. It is `no_std` and alloc-free, holding the embedded posture.

The boundaries are deliberate and recorded here rather than left to a future
reader to rediscover:

- An exact result that lands in the subnormal range keeps `UNDERFLOW`. The case is
  astronomically rare (the true value must be both subnormal and an exact
  representable cube or power). `status.rs` documents `UNDERFLOW` with
  tininess-only wording, so clearing it would assume a tininess-gated-by-inexactness
  convention the crate has not committed to. The conservative choice never wrongly
  suppresses the flag; revisit only if a downstream consumer needs the §7.5
  inexact-gated reading.
- Correctness rests on ADR-0032 correct rounding. If a future change regressed an
  operation to faithful rounding, an exact result could be delivered one ULP off,
  the cube or power check would fail, and the flag would stay set: a false negative
  (spurious flag), never a false positive. The dependency is one-directional and
  fails safe.
- The general path now performs a bounded integer computation on every `cbrt` /
  `pow`, negligible against the cost of the Extended transcendental it follows.
- Pathological exponents (a rational with a large denominator, or an integer
  exponent past `u32`) bail to "not proven". No representable exact power has such
  an exponent, so nothing real is lost; the worst case is the pre-existing spurious
  flag on an input that has it anyway.

This is a spec-conformance fix under the existing ADR-0032 contract, not a new
contract. It needs no version bump on its own; the "Fixed" entries sit under each
crate's `[Unreleased]` until the next release.

## Related

- Other ADRs: depends on ADR-0032 (correctly rounded §9.2 transcendentals);
  related to ADR-0026 (independent transcendental oracles).
- Bead: `fd-92w.8`.
- Commits: `a969b8b` (cbrt), `acde146` (pow), `93800f4` (cross-format tests).
- Removes the "cbrt and pow raise INEXACT on exact results" entry from
  `KNOWN_ISSUES.md`.

## Amendment (2026-06-10, fd-aqs.8)

The Context above claims `exp`, `ln`, and the trigonometric and
hyperbolic families irrational "for every non-special input". That is
false as stated for three derived functions this ADR never named:
`exp2(n)` is exact whenever `n` is an integer with `2^n` representable
at the format precision (`2^n` for `n >= 0`, `5^{-n} * 10^n` below;
up to `2^112` and `5^48` at Decimal128), `log10(10^k) = k`, and
`log2(2^k) = k`. The 2026-06-09 review found both consequences: every
such case raised a spurious INEXACT, and the directed modes could
deliver the wrong value outright when the 50-digit approximation
landed on the wrong side of the exact result (`exp2(3)` at
`TowardNegative` returned `7.999999...`; `log2(1024)` returned
`9.999...9`). It also left `exp2(10)` and `pow(2, 10)` disagreeing on
flags, an inconsistency invisible to the metamorphic suite because
`exp2 == pow(2, x)` was dropped as a value tautology (ADR-0025) and
the flag dimension went with it.

Unlike `cbrt` and `pow`, whose exactness needs the rounded result,
these three are decidable from the input alone, so the extension
short-circuits *before* the kernel (`exact::exp2_exact`,
`log2_exact`, `log10_exact`), repairing the value at every rounding
direction and the flag in one move, under the same
default-to-not-proven soundness posture as the rest of the module.
`tests/transcend_exact.rs` and the sibling mirrors pin the exact
cases, the representability boundaries on both sides (`2^112`/`2^113`,
`5^48`/`5^49` at Decimal128), and the `exp2`/`pow` flag parity.
Decimal32's `exp2(-11)` stays on the kernel path by design: `5^11`
has 8 digits, so the result is inexact at 7-digit precision — the
ADR-0033 exhaustive sweep's exact-NE-tie case is unchanged.
