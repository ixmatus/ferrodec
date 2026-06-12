# ADR-0053: Exponent-gap guards in ferrodec-decimal alignment paths

- **Status**: accepted
- **Date**: 2026-06-11

## Context

The 2026-06-09 rigorous review (fd-aqs.3, report at
`docs/archive/REPORT-rigorous-review-2026-06-09.md`) confirmed two related
defects in ferrodec-decimal, both rooted in the same shape: an `i64`
exponent gap is fed to `DecBig::mul_pow10` as a `u32`, and the
multiplication materializes `10^gap` before any validity check runs.

1. `arith::combine_finite` computed `(ea - min_e) as u32`. Add and
   subtract take exponents from `i32` operands, so their gap fits `u32`
   exactly. `fma` does not: the exact product's exponent is the sum of two
   `i32` exponents (to about `±4.3e9`), so the gap to the addend reaches
   about `6.4e9`, the cast wraps, and in-range operands produce silently
   wrong finite results. Witness:
   `fma(1E+2147483647, 1E+2147483647, 1E-2147483648)` must overflow to
   `+Infinity`, not return a finite number.
2. `quantize` (pad direction) and the `integer_divide` helper behind
   `divide_integer` / `remainder` / `remainder_near` aligned operands by
   materializing `10^gap` before the precision validity check, so
   `1E+2147483647 quantize 1E-2147483648` allocated a multi-gigabyte
   intermediate and then returned `(NaN, INVALID)` anyway. The gap casts
   there do not wrap (both exponents are `i32`), but the allocation is
   attacker-reachable from in-range operands.

The decTest corpus and the libmpdec differential never exercise gaps near
`2^32`, which is why the 27591-vector 0-fail pin was structurally blind to
all of this.

`DecBig::div_rem_pow10` needs no guard: it splits at limb boundaries and
never materializes `10^k`, so `round_finite` and the rounding direction of
quantize already handle huge drops cheaply. `compare` also needs no guard:
it only aligns after an adjusted-exponent equality check, which bounds the
gap by the operands' digit counts.

## Decision

Bound every operand-derived gap against `precision` plus the other
operand's digit count before any `mul_pow10`, and short-circuit to a path
that cannot allocate beyond that bound.

**`combine_finite` (add, subtract, fma accumulation).** Zero coefficients
short-circuit first: two zeros resolve by the sign rule directly, and a
zero beside a nonzero operand rounds the nonzero operand alone (same value
and flags, no alignment). For two nonzero operands whose gap exceeds
`precision + digits(lo) + 2`, the smaller operand can no longer reach the
round digit; it influences the result only through the sticky bit. The
implementation replaces exact alignment with a sticky surrogate: shift the
larger operand so it carries at least `precision + 2` digits, subtract one
unit in its last place when the signs differ, and hand `round_finite` the
result with `pre_sticky = true`. The true sum then lies strictly inside
the open interval between the surrogate coefficient and its successor;
because the surrogate carries at least `precision + 2` digits, that whole
interval sits strictly below the round digit, so the kept digits, the
round digit, and the sticky bit (forced true, and truly true, since the
discarded tail is nonzero) all match the exact computation, for every
rounding mode. The pre-rounding adjusted exponent that drives tininess
detection also matches. Alignment cost is now bounded by
`precision + digits(a) + digits(b)` in every case; the cast to `u32` can
no longer wrap for any gap the exact path receives.

**`quantize` (pad direction).** Check `digits + gap > precision` before
materializing, and return the operation's existing `(NaN, INVALID)`.

**`divide_integer`, `remainder`, `remainder_near`.** Guard on the
difference of adjusted exponents before calling `integer_divide`. A
quotient needs at least `adj(a) - adj(b)` digits, so a difference above
`precision` is the operations' existing INVALID without any alignment.
A negative difference means `|a| < |b|`: the integer quotient is zero and
the remainder is the dividend itself, both returnable without alignment
(for `remainder_near`, only a difference of `-2` or below short-circuits,
since at `-1` the residue can still flip around half the divisor; the
fall-through gap there is digit-bounded). A zero dividend
short-circuits likewise. Inside the guards the remaining gaps are bounded
by `precision + digits(a) + digits(b)`.

## Consequences

- `fma` at extreme exponents now overflows and underflows per the
  specification instead of returning silently wrong finite values. This is
  the only value-visible change, and it changes outputs that were
  previously garbage.
- No operation in the add/subtract/fma/quantize/divrem families can be
  driven to allocations beyond `O(precision + operand digits)` by exponent
  manipulation alone. The prior behavior invited denial of service from
  i32-range operands.
- decTest add/subtract vectors with gaps like `1E+999999999 + 1` now take
  the surrogate path, so the conformance suite exercises it directly; the
  27591-vector pin must stay 0-fail, which is the regression gate for the
  bracketing argument above.
- The surrogate path duplicates the rounding-interval reasoning in prose
  (here and at the call site) rather than in types. A Kani harness over
  `DecBig` of this size is intractable (ADR-0038's heap finding), so the
  argument is discharged by the conformance pin plus directed unit tests
  at the witnesses, including moderate oversize gaps that complete without
  materialization.
- The three plausible failure modes, inverted up front: a wrong surrogate
  bound (off by one against the round digit) would misround directed modes
  at gap boundaries, which the directed-mode witnesses pin; a missed zero
  case would regress sign rules for zero results, which the existing
  decTest add/subtract zero vectors cover; an over-eager divrem guard
  would reject valid quotients near `adj` difference `precision`, which
  the boundary unit test pins.

## Related

- Beads: fd-aqs.3 (2026-06-09 review findings 5a/5b)
- Report: `docs/archive/REPORT-rigorous-review-2026-06-09.md`
- Other ADRs: ADR-0038 (DecBig heap intractability under Kani),
  ADR-0042 (decTest content pins)
