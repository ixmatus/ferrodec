# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `exp2` now resolves nearest-mode ties exactly (ADR-0059 M7). An integer
  input `n` whose `2^n` is expressible in at most `PRECISION + 1` digits is
  delivered from the exact coefficient through the format rounder instead of
  the approximation kernel, whose error lands on an arbitrary side of a true
  value that is itself a rounding boundary (`5^n` ends in 5, so a
  `PRECISION + 1`-digit `5^n` makes `exp2(-n)` an exact midpoint). Changed
  values, all at ties: `exp2(-49)` at `NearestAway` and `exp2(-50)` at
  `NearestEven` for a 34-digit format; `exp2(-23)` and `exp2(-24)` at
  `NearestAway` for 16 digits; `exp2(-11)` at `NearestAway` for 7 digits.
  Every other mode and input is unchanged (the non-tie `PRECISION + 1` cases
  were already correct and are now pinned).

## [0.2.0] - 2026-07-03

### Changed

- **Breaking:** `DecimalFormat::to_extended_parts` now returns
  `Option<(U256, i32, bool)>` instead of `(U256, i32, bool)`, returning
  `None` for NaN and infinity rather than panicking. Implementors and callers
  of the public `DecimalFormat` trait must handle the `Option` (fd-aqs.13).

### Fixed

- The `cbrt` and `pow` kernels no longer raise `INEXACT` on an exactly
  representable result. A new `exact` module, which allocates nothing, proves
  a perfect cube root (`cbrt(8) = 2`) or an exact integer or rational power
  (`pow(10, 300) = 1E+300`, `pow(4, 0.5) = 2`) in fixed width `U256` / `U384`
  integer arithmetic, and the kernel clears the flag only on that proof.
  `exp`, `ln`, and the trigonometric and hyperbolic families are unchanged
  (their results are irrational for every input that reaches the rounding
  step). The value is unchanged; the fix matches IEEE 754-2019 §7.5. See
  ADR-0047 (fd-92w.8).

## [0.1.0] - 2026-05-17

Initial release. The shared faithful Extended-precision transcendental
kernel (`exp` / `ln` / `exp2` / `log2` / `log10` / `cbrt` / `sin` /
`cos` / `tan` / `asin` / `acos` / `atan` / `atan2` / the hyperbolic
family / `pow`, the Payne-Hanek argument reduction, and the
`Extended` 50-digit intermediate with its constants) was extracted
from `ferrodec`'s private `math` module into this standalone `no_std`
crate (fd-r0l P0a.2, commits `d9106b0`..`756d336`), generic over the
`DecimalFormat` seam so every decimal sibling reuses one verified
implementation instead of a per-precision copy.

Behaviour-neutral for the formally-verified `Decimal128` parent: its
instantiation is byte-identical to the pre-extraction kernel, proven
by the unchanged property, conformance, and per-kernel suites. The
faithful-rounding contract is ADR-0021; the family-wide decision is
recorded in ADR-0024. Depends on `ferrodec-ieee` 0.1.4 (the decoded
`IeeeDecodedClass`) and `ferrodec-multiword` 0.1.0 (wide-integer
primitives).
