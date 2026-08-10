# ADR-0051: A signed residual crosses the kernel's rounding seam for grid-exact small arguments

- **Status**: accepted (completes the ADR-0050 program; closes fd-aqs.7)
- **Date**: 2026-06-10

## Context

ADR-0050 left one named residue open. When `f(x) = x + c·x³ + …` (or
`f(x) = 1 + c·x^k + …`) and the correction sits below the kernel's 50
significant digit working resolution, the Taylor sum absorbs it: the
Extended value is exactly `x` (or exactly 1), which is exactly a
format grid point. `Extended::to_format` hands the bare value to
`round_and_pack_finite` with no enclosure information, so the four
directed rounding modes deliver the grid point where IEEE 754-2019
§4.3.3 requires its neighbour. Witness (2026-06-09 review, runtime
confirmed): `sin(1E-40)` at `Decimal32` under `TowardNegative`
returned `1E-40`; the correct result is `9.999999E-41`, since
`sin x < x` for positive `x`. The class covers `sin`, `cos`, `tan`,
`asin`, `atan`, `sinh`, `cosh`, `tanh`, `asinh`, `atanh` near 0 and
the `exp` family (`exp`, `exp2`, and `pow`/`cbrt` through
`exp_from_extended`) for tiny exponents, on all three formats. The
nearest modes are unaffected (the absorbed correction is far below
half an ULP), which is why the faithful astro-float layer and every
NearestEven-margin search were structurally blind to it.

The seam is the structural problem: the kernel *knows* the true
result sits strictly above or below its 50 digit value (the sign of
the first absorbed term is the sign of the whole absorbed tail for
these series), but the `to_format` boundary cannot express that
knowledge.

## Decision

Carry the enclosure across the seam, in the narrowest form that
decides every case: a magnitude direction on an absorbed
sub-resolution residual.

- `Extended::to_format_with_residual(magnitude_grows, rm)` widens
  the coefficient to the full 50 digit working width (exact) and
  routes through `round_and_pack_finite`: with `magnitude_grows`,
  the widened coefficient with `pre_sticky = true` denotes the open
  interval one unit-in-the-50th-digit above the value; otherwise the
  coefficient is decremented by one such unit first, denoting the
  open interval below. Every point of the denoted interval rounds
  identically to the true result at every direction and every format
  precision, because the true result lies inside it (the absorbed
  tail is strictly smaller than one unit in the 50th digit, see the
  trigger) and the nearest format grid points are at least 10^15
  units away.
- Each affected kernel takes the residual path on a **post-hoc
  anchor equality test**: after the series (or composition) produces
  its Extended result, the kernel checks whether that result equals
  the anchor exactly — the input `x` for the `f(x) ≈ x` family, 1
  for `cos`, `cosh`, and the `exp` family. Equality at 50 digits is
  precisely the grid-stuck condition (the series absorbed every
  correction, so the absorbed tail is strictly below one unit in the
  50th digit and the interval claim holds), and it is decided by the
  arithmetic itself rather than approximated by a decade threshold,
  which would either leave a gap (absorbed but untreated) or overlap
  (treated where the interval claim is unsound). Anchor equality is
  also *complete*: a kernel result can sit on a non-anchor grid
  point only if the true value approached a representable number to
  within one part in 10^50, and the empirical worst case margins
  (ADR-0033) bound every §9.2 function's closest approach many
  orders of magnitude above that. The `cos` path near even multiples
  of π is the instructive case: the reduction residual can collapse
  the 50 digit value to exactly 1 far from zero, and the test
  catches it there too, where any small-argument threshold would
  not (`cos < 1` strictly at every finite nonzero decimal, so the
  shrink direction remains a theorem).
- The residual's direction is the sign structure of each series:
  the magnitude grows for `tan`, `asin`, `sinh`, `atanh`, `cosh`,
  and `exp` of a positive argument; it shrinks for `sin`, `atan`,
  `tanh`, `asinh`, `cos`, and `exp` of a negative argument. These
  are theorems about the leading correction term's sign, not
  measurements.
- `exp_from_extended` carries the path once for the whole `exp`
  family, so `exp2`, `pow`, and `cbrt` inherit it through their
  composition without further seams.

The anchor band corpus generator drops its "nearest modes only"
narrowing for grid-hugging values wherever the oracle can certify
the side (boundary distance above the 10^-100 ULP oracle floor) and
emits all five modes there; the per (function, mode) pins in the
three gate tests move accordingly, and the corpus gains small
argument files for the full family (`sin`, `cos`, `tan`, `atan`,
`sinh`, `cosh`, `tanh`, `exp`, `exp2`). Cases whose correction sits
below the oracle floor (e.g. `atanh(1.000001e-95)` at `Decimal32`,
correction ~10^-191 relative) keep nearest-mode lines: the kernel
now delivers the directed neighbours there too, but this tooling
cannot independently certify them, and an uncertified pin would be
faith, not verification.

## Consequences

- The last named directed-mode defect class closes; KNOWN_ISSUES
  drops its final transcendental entry. With ADR-0050's
  reformulations and fd-aqs.5's gate and negation fixes, the §9.2
  surface's correctly-rounded claim is whole again at every rounding
  direction, on the repaired error model.
- The seam now expresses what the kernel knows. The residual channel
  is available to any future kernel path that detects absorption;
  the bare `to_format` remains correct for every value the series
  genuinely moved off-grid (one representable 50 digit step of
  separation decides every format rounding).
- The anchor equality test costs one Extended comparison per
  evaluation on the affected kernels — noise against the series
  work it follows.
- ~~The oracle-floor scope note above is the honest residual: a
  directed-mode result whose correction is below ~10^-100 relative
  is delivered by the same proven mechanism but pinned only at the
  nearest modes. Certifying those few decades needs a
  higher-precision offline oracle pass (Arb at raised working
  precision would do); it is bookkeeping, not a suspected defect.~~
  Closed by the S4 addendum below.

## Addendum (fd-4zo.6, 2026-08-09): the oracle floor is certified

The predicted bookkeeping pass ran and the note above closes.
`tools/certify_anchor_floor.py` certifies every nearest-mode-only
group in the anchor band corpus with FLINT/Arb ball arithmetic (an
enclosure excluding zero is a proof of sign, not an estimate): for
each of the 14 groups (the ±1000001e-95 pair at Decimal32 across
`sin`, `tan`, `atan`, `sinh`, `asinh`, `tanh`, `atanh`), the side of
the hugged grid point and the 10^-100-ULP bracket were both proven at
1024 working bits (the tool escalates to a 65536 bit cap; nothing
needed past the first attempt). The certified corrections sit near
10^-268 absolute — the |x|³/6 and |x|³/3 leading terms of the odd
series, signs matching the side theorems the kernel's seam uses —
and the 42 emitted directed-mode lines replay against the kernel
with zero disagreements: the residual seam's deliveries were correct
all along, and are now pinned rather than trusted. The corpus's
per (function, mode) pins moved from 4 to 6 on the seven affected
Decimal32 directed buckets; no format's nearest-mode pin moved. The
suspected-defect clause was not needed.

## References

- ADR-0050 (the anchor band program this completes), ADR-0032 /
  ADR-0033 (the contract and its empirical side)
- `docs/archive/REPORT-rigorous-review-2026-06-09.md` §4 (the
  directed-mode findings and witnesses)
- IEEE 754-2019 §4.3.3 (rounding-direction attributes)
