# ADR-0041: GDA miscellaneous-operation surface for ferrodec-decimal

- **Status**: accepted
- **Date**: 2026-06-03

## Context

ADR-0040 completed the numerical surface of `ferrodec-decimal`: the core
arithmetic, `squareRoot`, and the four transcendentals, all conformant against
the general decTest suite and the libmpdec differential. That left a second tier
of the General Decimal Arithmetic specification unimplemented: the logical,
positioning, exponent, next-value, extended-comparison, and classification
operations. The fixed-width siblings already carry most of these under ADR-0031,
libmpdec implements all of them, and the Cowlishaw general decTest suite has a
vector file for each. Leaving them out would force every consumer to route
around `class`, `scaleb`, `logb`, `shift`, `rotate`, `nextPlus`, the logical
ops, and the magnitude comparisons. Completeness against the specification is the
bar this crate is held to, so the gap is closed rather than documented.

The siblings are an oracle for behavior but not a template: the parent BID crate
does not implement `class`, `nextToward`, or `compareSignal` at all, so those
three are derived fresh from the specification.

## Decision

Implement the full miscellaneous, comparison, and predicate surface on
`Decimal`, taking each operation's behavior from the specification and pinning
it against the vendored general decTest file and the libmpdec differential.

**Logical (`and`, `or`, `xor`, `invert`).** Digit-wise on the low `precision`
digits, validating that the whole coefficient is a non-negative exponent-zero
integer of zeros and ones. These diverge from every other operation in NaN
handling: a NaN is not a valid logical operand, so *every* NaN, quiet or
signaling, raises `Invalid_operation` and yields the default NaN with no payload
propagation.

**Positioning (`shift`, `rotate`).** A shared digit kernel moves the coefficient
within the precision, selected by a wrap flag. The count must be an
exponent-zero integer (so `1.0`, numerically integral but carrying a non-zero
quantum, is rejected) of magnitude at most the precision. NaN propagates
normally and before the count is validated; an infinite first operand with a
valid count passes through.

**Exponent (`scaleb`, `logb`).** `scaleb` shifts the exponent by an
exponent-zero integer of magnitude at most `2 * (emax + precision)` (out of
range is invalid, distinct from an in-range result that overflows) and routes
through the rounding core. `logb` returns the adjusted exponent as an integer
rounded to the context, with zero giving `-Infinity` and division by zero, and
an infinity giving `+Infinity`.

**Next-value (`next_plus`, `next_minus`, `next_toward`).** A shared stepper
rounds the operand onto the grid toward the target infinity (handling operands
wider than the precision) and, when it is already on the grid, takes the
explicit one-place step, carrying the decade crossings (spill up at a power of
ten, refine down below one). `next_plus` and `next_minus` signal nothing but
`Invalid_operation` for a signaling NaN. `next_toward` signals like an
arithmetic step: a normal result raises nothing, a nonzero subnormal result
raises `Underflow` and `Inexact`, an infinite result raises `Overflow` and
`Inexact`, and a zero result (the step crossed the subnormal gap to a signed
zero at Etiny) raises `Underflow`, `Inexact`, and `Clamped` exactly when Etiny
is below Emin, independent of the clamp flag. That last rule was distinguished
from the clamp-dependent and the always-on readings by the libmpdec differential
at precisions the decTest vectors do not exercise; at precision one Etiny equals
Emin and the zero carries no flag.

**Comparison and classification.** `compareSignal` is `compare` with invalid
raised for any NaN. `compareTotalMag` is the total ordering of the magnitudes.
`maxMagnitude` and `minMagnitude` pick by magnitude with the value-based max /
min as the tie-break. `sameQuantum` returns the decimal one or zero. `class`
returns the classification string (the normal / subnormal threshold is the
adjusted exponent against Emin). Plain `copy` is the pure identity, leaving a
signaling NaN signaling. `isNormal`, `isSubnormal`, `isSigned`, and `radix`
round out the predicates; `isCanonical` is unconditionally true, since the
arbitrary-precision representation has no redundant encodings.

**Excluded: `trim`.** `trim` is a decNumber library convenience, not a General
Decimal Arithmetic specification operation, and has no general decTest file. It
is left out so the surface is exactly spec-scoped.

**Convenience constructors.** `from_i64` / `from_i128` / `from_u64` / `from_u128`
build the exact integer at exponent zero. `TryFrom<f64>` / `TryFrom<f32>`, behind
a new `binary-float` feature, convert *losslessly*: a finite binary float is a
dyadic rational, hence an exact finite decimal, so the result is the float's
precise value with no rounding and no context. This deliberately diverges from
the fixed-width siblings, which must round an f64 to their width; an
arbitrary-precision value need not. The conversion is pure coefficient
arithmetic (`2^-k = 5^k * 10^-k` with the shared powers of two cancelled), so the
feature pulls no dependency and builds `no_std` without the `fmt` surface. NaN
and the infinities are rejected with `DecimalFromFloatError`.

## Consequences

`ferrodec-decimal` now implements the whole General Decimal Arithmetic numerical
and miscellaneous surface, at parity with the ADR-0031 sibling surface and
beyond it (`nextToward`, `class`, and `compareSignal`, which the fixed formats
declined). Eighteen further general decTest files are vendored and pinned under
the ADR-0010 record-then-pin discipline (Cowlishaw suite 2.62, the ADR-0039
provenance), standing at 27492 pass, 0 fail, 99 skip across 50 files. The
libmpdec differential is extended with all the value-returning operations and a
logical / shift operand generator so their valid paths are exercised, not only
the invalid-operand path the general generator hits; that differential is what
caught two `nextToward` zero-result flag-rule errors the conformance vectors
alone allowed. Verification stays differential plus decTest plus property and
example tests, with no Kani: the heap coefficient is intractable for it,
consistent with ADR-0038.

The release is **0.3.0**, an additive minor: the surface only grows, and the 1.0
bar set in ADR-0038 also requires the deferred `DecBig` performance pass
(schoolbook multiply, Knuth Algorithm D divide) and a final API settle, neither
of which this arc touches. The crate stays on the 0.x line.

## Related

- Plan: `plans/verified-against-the-code-parallel-hejlsberg.md`
- Builds on ADR-0038 (the design and the 1.0 gates), ADR-0039 (the decTest
  runner this arc extends), ADR-0040 (the numerical surface it completes), and
  ADR-0031 (the sibling miscellaneous surface it mirrors). ADR-0036 is the
  fixed-width `from_f64` signature the lossless `TryFrom` deliberately diverges
  from.
- Issues: `fd-0dl` (the arc umbrella) and `fd-0dl.1` through `fd-0dl.10` (the
  slices).
