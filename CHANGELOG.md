# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.3.0] - 2026-05-05

### Added

- `Decimal128::is_integer(self) -> bool` — `const fn` predicate that
  returns true iff `self` represents a mathematical integer.
- `Decimal128::ulp(self) -> Self` — `const fn` returning the unit in
  the last place at `self`'s stored quantum (cohort-dependent;
  documented).
- `TryFrom<f64>` / `TryFrom<f32>` for `Decimal128` (behind
  `binary-float`). Rejects NaN and ±∞ via the new
  `Decimal128FromFloatError` enum (`NotANumber` / `Infinite`).
- `Decimal128::try_new_unsigned(coefficient: u128, exponent: i32)` —
  mirror of `try_new` for unsigned coefficients. Always produces
  non-negative values; reuses `Decimal128BuildError`.
- `tests/common/mod.rs` — shared `parse` / `within_ulps` /
  `bigfloat_to_decimal_string` helpers consumed by eight property
  test files.

### Changed

- `acosh(x)` near `x = 1` now uses a `log1p`-based formula
  (`acosh(x) = log1p((x − 1) + sqrt((x − 1)(x + 1)))`) for inputs
  with `x − 1 < 10⁻²`. The original `ln(x + sqrt(x² − 1))` form lost
  up to ~33 digits of precision near the singularity via the `x² − 1`
  cancellation; the new path keeps the difference explicit and
  preserves the ≤ 1 ULP envelope down to `1 + 10⁻³³`.
- `taylor_log1p_ext` (in `src/math/ln.rs`) is now exposed as
  `pub(super) fn log1p_extended` for the acosh path's reuse.
- Hot-path runtime string parses removed: `Extended::HALF`,
  `Extended::EXP_DOMAIN_LIMIT`, and `Extended::saturate_overflow(sign)`
  replace the inline `Extended::parse_str("0.5")` /
  `Extended::parse_str("14150")` calls and the magic
  `(coef = 1, exp = 7000)` struct literals in `exp` / `sinh` / `cosh`.
- README "Feature surface" table now records concrete byte deltas on
  `thumbv6m-none-eabi` (release `libferrodec.rlib`): fmt = 401 KB,
  +exp-log = +84 KB, +trig = +116 KB, +hyperbolic over exp-log =
  +22 KB, +pow over exp-log = +15 KB, +transcendentals meta = +185 KB.

### Removed

- `_PRECISION_KEEPALIVE` hack in `src/convert/int.rs`. The
  `PRECISION` import was unused; both went away.

## [1.2.0] - 2026-05-05

### Added

- `trig`, `exp-log`, `hyperbolic`, and `pow` sub-features. Embedded
  users on flash-tight targets can now pay for only the transcendental
  clusters they call. The 6 300-digit Payne-Hanek `2/π` table is
  gated under `trig` and elided when not needed. The pre-1.2
  `transcendentals` feature is preserved as a meta-feature pulling
  all four sub-features, so existing dependents are unaffected.

### Changed

- `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh` now run their
  inner kernels at 50-digit `Extended` precision end to end, rather
  than composing `Decimal128`-rounded `exp` and `ln` calls. The result
  is faithfully rounded (≤ 1 ULP at 34 digits) across the supported
  domain — down from the prior ≤ 5 ULP envelope at `|x| ≥ 0.5`. The
  test envelopes in `src/math/hyperbolic.rs` and
  `tests/property_hyperbolic.rs` are tightened from 5 ULP to 1 ULP.
- `src/math/exp.rs::exp_extended(Extended) -> Extended` and
  `src/math/ln.rs::ln_from_extended(Extended) -> Extended` are
  exposed as crate-internal building blocks that keep intermediate
  results at extended precision; the existing
  `exp_from_extended` / `ln_extended` are now thin wrappers.
- README "Accuracy" section drops the two stale hyperbolic caveats;
  README "Feature surface" table reflects the four-way split.
- `src/math/mod.rs` and `docs/PLAN.md` updated to reflect the
  current state (post-v1 quantum + canonical surface, ≤ 1 ULP
  transcendentals everywhere).

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
  hyperbolic compositions for `|x| ≥ 0.5` shipped at ≤ 5 ULP and
  were tightened to ≤ 1 ULP in 1.2.0. `sin` / `cos` argument
  reduction handles the full magnitude range via Payne-Hanek with
  a 6 300-digit table of 2/π.
- Binary-float conversions (`binary-float` feature): `to_f32`,
  `to_f64`, `from_f32`, `from_f64`.
- String parse + format (`fmt` feature, default): `parse_str`,
  `Display`. Uses `core::fmt::Write`; no allocator.
- Verification: 376 lib unit tests; 12 proptest files
  cross-checking against `astro-float`; 8 149 / 0 / 572 conformance
  results across the speleotrove `dq*.decTest` suite; 50 Kani
  formal-verification harnesses for the IEEE special-case dispatch.

[Unreleased]: https://github.com/ixmatus/ferrodec/compare/v1.3.0...HEAD
[1.3.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.3.0
[1.2.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.2.0
[1.1.1]: https://github.com/ixmatus/ferrodec/releases/tag/v1.1.1
[1.1.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.1.0
[1.0.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.0.0
