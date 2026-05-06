# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.1.1] - 2026-05-05

### Added

- `examples/`: `money` (quantize-to-cents money math), `rounding_modes`
  (the five IEEE 754 directions on a half-way value), `transcendentals`
  (exp / ln round-trip and `sin(π/2)` via Payne-Hanek argument reduction).
- This `CHANGELOG.md`.

## [1.1.0] - 2026-05-05

### Added

- IEEE 754-2019 §5.7.2 / §5.4.2 canonical predicates:
  `Decimal128::is_canonical` and `Decimal128::canonicalize`. Both
  `const fn`, no `Status` return; `canonicalize` is "homogeneous quiet"
  per the spec and never raises an exception.
- Inline runnable doctests on the eleven post-v1 public methods. Doctest
  count goes from 1 to 14, running identically under `--features fmt`
  and `--no-default-features`.

### Changed

- README Quick Start expanded from one example to three (parse + add,
  `try_new` integer-pair construction, status-flag inspection).
- "What you can call" surfaces the post-v1 surface explicitly:
  `try_new`, the canonical pair, `compare_total_magnitude`, and a new
  "Quantum operations" subsection.
- Dependency snippet bumped: `ferrodec = "0"` → `"1"`.
- Lib-test count and conformance totals refreshed (376 / 8721 cases).

## [1.0.0] - 2026-05-04

Initial release. Includes the full v1 plan scope plus the post-v1
IEEE 754 §5.3 / §5.10 quantum gap-fill.

### Added

- BID-128 encoding, 34 decimal digits of precision, exponent range
  `10⁻⁶¹⁴³` through `10⁺⁶¹⁴⁴`. `no_std`, `forbid(unsafe_code)`,
  MSRV 1.84.
- Distinguished constants: `ZERO`, `NEG_ZERO`, `ONE`, `NEG_ONE`,
  `TEN`, `MAX`, `MIN`, `MIN_POSITIVE`, `MIN_POSITIVE_NORMAL`,
  `INFINITY`, `NEG_INFINITY`, `NAN`, `SIGNALING_NAN`.
- Construction / conversion: `try_new`, the integer round-trip
  (`from_i32` … `to_u128`), the bit-pattern round-trip
  (`from_bits`, `to_bits`).
- Classification: `is_nan`, `is_signaling_nan`, `is_quiet_nan`,
  `is_infinite`, `is_finite`, `is_zero`, `is_normal`, `is_subnormal`,
  `is_sign_negative`, `is_sign_positive`, `classify`.
- Sign and ordering: `abs`, `neg`, `copysign`, `signum`,
  `partial_cmp`, `total_cmp`.
- Arithmetic: `add`, `sub`, `mul`, `div`, `fma`, `sqrt`, `rem`. All
  return `(Decimal128, Status)`; all but `rem` take a `RoundingMode`.
- Rounding to integral: `floor`, `ceil`, `trunc`, `round`,
  `round_ties_even`, `round_to_integral`, `round_to_integral_exact`.
- Quantum operations (§5.3): `quantize`, `same_quantum`, `scaleb`,
  `logb`, `next_up`, `next_down`, `radix`. Total-magnitude
  comparison (§5.10): `compare_total_magnitude`.
- Transcendentals (`transcendentals` feature): `exp`, `exp2`, `ln`,
  `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`,
  `atan2`, `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `pow`,
  `cbrt`. Faithful rounding (≤ 1 ULP) on the typical input domain;
  ≤ 5 ULP on hyperbolic compositions for `|x| ≥ 0.5`. `sin` / `cos`
  argument reduction handles the full magnitude range via
  Payne-Hanek with a 6 300-digit table of 2/π.
- Binary-float conversions (`binary-float` feature): `to_f32`,
  `to_f64`, `from_f32`, `from_f64`.
- String parse + format (`fmt` feature, default): `parse_str`,
  `Display`. Uses `core::fmt::Write`; no allocator.
- Verification: 376 lib unit tests; 12 proptest files
  cross-checking against `astro-float`; 8 149 / 0 / 572 conformance
  results across the speleotrove `dq*.decTest` suite; 50 Kani
  formal-verification harnesses for the IEEE special-case dispatch.

[Unreleased]: https://github.com/ixmatus/ferrodec/compare/v1.1.1...HEAD
[1.1.1]: https://github.com/ixmatus/ferrodec/releases/tag/v1.1.1
[1.1.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.1.0
[1.0.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.0.0
