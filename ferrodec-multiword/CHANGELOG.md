# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `U1024`, the 1024-bit fixed-width type (ADR-0060 exact integer
  adjudicator): the comparison width for the adjudicator's widest
  aligned relations (`rootn` at `|n| = 6`, `pown` at `n = −6`, both
  past `U768`'s 231-digit envelope). Minimal compare-oriented surface
  (`add`, `sub`, `cmp`, `mul_u128`, `mul10`, `mul_pow10`, `div_rem10`,
  `decimal_digit_count`, `from_u128`, `from_u768`, the overflow-honest
  `checked_mul` for the adjudicator's powering folds) plus the
  widening product `u768_mul_u128_to_u1024`; no division, no
  collapse, same array-limb representation as `U768`.

## [0.2.0] - 2026-08-02

### Added

- The `alloc` feature and `DecBig`, a growable base-`10^9` decimal-limb
  unsigned integer: schoolbook and Karatsuba multiplication (crossover at
  32 limbs, ADR-0043/ADR-0044), Knuth Algorithm D division, power-of-ten
  scaling, ASCII digit conversion, and a Newton integer square root. The
  coefficient backend for `ferrodec-decimal` (ADR-0038) and for the
  transcendental ladder's unbounded rung; the default build stays
  alloc-free and the fixed-width types are unaffected. Landed across the
  `ferrodec-decimal` arcs; recorded here on the first `ferrodec-multiword`
  release that carries it.
- `U768`, the 768-bit fixed-width type (ADR-0059 M1): the product and
  alignment width for the transcendental ladder's 110-digit rung and its
  wide Payne–Hanek window.
- `bigconst` (ADR-0059 M8b): runtime arbitrary-precision constant
  generators on `DecBig` — `π` (Machin), `2/π` (division into computed
  `π`), `ln 2` and `ln 10` (atanh series), `e` (factorial series),
  `tan(π/8)` (exact via `isqrt`), and `1/ln 2` / `1/ln 10` (division into
  the computed originals) — each with a derived truncation bound (within
  one unit of the last digit; `tan(π/8)` exact), a stated scale, mpmath
  oracle pins at four depths, and algebraic cross-identities. Contract
  range 8 to 100,000 digits; built for the unbounded rung, whose Ziv
  doubling no stored table can follow.

## [0.1.0] - 2026-05-17

Initial release. The fixed-width wide-integer primitives (`U256` /
`U384` / `U512` and their base-10 and bit operations) were extracted
from `ferrodec`'s private `multiword` module into this standalone
`no_std` foundation crate (fd-r0l P0a.1, commit `82a7fe1`) so the
frozen, Kani-proven arithmetic and transcendental cores depend on a
stable base rather than on `ferrodec-transcend`. Behaviour-neutral:
the moved code is byte-identical to the pre-move `ferrodec` module and
its callers' tests stay green unchanged.
