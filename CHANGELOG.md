# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.6.0] - 2026-05-06

The "project hygiene" release. Adds badges, a fuzz harness, an
MSRV-stability policy, and a crate-level rustdoc lede that opens
with both audiences.

### Added

- README header badges for CI status, crates.io version, docs.rs,
  and license.
- `fuzz/` directory with two `cargo-fuzz` targets (`parse` and
  `arith`). Catches the long tail of malformed inputs and arbitrary
  arithmetic operand pairs that Kani's bounded special-case
  harnesses don't cover. Requires a nightly toolchain plus
  `cargo install cargo-fuzz`; not part of stable CI.
- README "MSRV policy" section. ferrodec holds MSRV for at least
  six months after each Rust release; bumping is a minor-version
  event, never a patch.
- Crate-level rustdoc lede in `src/lib.rs` (above the
  `include_str!("../README.md")`). Opens with the two-audience story
  so prospective users see immediately whether ferrodec is for them.

## [1.5.0] - 2026-05-06

The "user-facing polish" release: format specifiers, engineering
notation, and a clearer story for prospective users picking between
ferrodec and `rust_decimal`.

### Added

- `Display` format specifiers. `{:.N}` rounds via `quantize` to `N`
  digits after the decimal point at `NearestEven` (pads with zeros
  for short inputs). `{:e}` and `{:E}` force scientific notation
  with the corresponding exponent letter; combine with `.N` for
  mantissa precision. `LowerExp` / `UpperExp` impls.
- `Decimal128::engineering()` — wrapper that formats in engineering
  notation (scientific with the exponent forced to a multiple of 3,
  mantissa in `[1, 1000)`). Returns the new public `Engineering`
  adapter that implements `Display`. Useful for finance and
  SI-scaled output.

### Changed

- README opens with both audiences (embedded + general decimal
  arithmetic). The "three design choices" trio acknowledges the
  opt-in `ops` feature so the prose matches the post-1.4 reality.
- New "Choosing between ferrodec and `rust_decimal`" section in the
  README. Honest side-by-side covering precision, conformance,
  verification, `no_std`, ecosystem maturity, and default ergonomics.
  Followed by "pick X when" guidance.

## [1.4.0] - 2026-05-05

The "drop-in alternative to rust_decimal for users who want IEEE 754
conformance" release. Closes the ecosystem-integration gap.

### Added

- `serde` feature flag. `Serialize` / `Deserialize` route through the
  canonical decimal string by default (survives every format,
  human-readable in JSON / TOML / YAML). For binary formats, opt
  into the new `ferrodec::serde_bid` helper via
  `#[serde(with = "ferrodec::serde_bid")]` to serialize the raw 128-bit
  BID pattern. The bid module's deserializer accepts both u128 and a
  string fallback so the same struct works in both binary and
  human-readable formats.
- `num-traits` feature flag. Implements `Zero`, `One`, `Bounded`,
  `Num`, `Signed`, `FromPrimitive`, `ToPrimitive`. Closes the SMIL
  adapter ask. Transitively enables `ops` because `Num` requires the
  `core::ops` traits. New `FromStrRadixError` enum for the
  `Num::from_str_radix` path.
- `ops` feature flag. Enables `core::ops::{Add, Sub, Mul, Div, Rem}`,
  the `*Assign` variants, and unary `Neg` on `Decimal128`. Each
  operator routes through the corresponding explicit method at
  `RoundingMode::NearestEven` and discards the per-call `Status`.
  Default profile is unchanged: the principled API trio stays intact
  unless callers opt in.
- `core::str::FromStr` for `Decimal128`. Idiomatic Rust spells
  parsing as `"1.23".parse::<Decimal128>()`; the impl wraps
  `parse_str` at NearestEven and drops `Status`.
- `core::iter::Sum<Self>` / `Sum<&Self>` / `Product<Self>` /
  `Product<&Self>` for `Decimal128`. Iterator chains like
  `decimals.iter().sum::<Decimal128>()` and `.product()` now work
  out of the box. No new feature gate.
- `core::error::Error` impls for `Decimal128BuildError`,
  `ParseDecimalError`, and `Decimal128FromFloatError`. Plus
  `Display` impls for the two that lacked them. Lets these compose
  with `?`, `Box<dyn Error>`, and `anyhow::Error` chains. (Stable in
  `core` since Rust 1.81; ferrodec's MSRV is 1.84.)

### Changed

- README "Why no `core::ops`" section retitled to "Why no `core::ops`
  (and how to opt in)" and extended with the `ops` feature design.
  The default-rationale prose is unchanged.
- README "Feature surface" table gains rows for `ops`, `serde`, and
  `num-traits` with their code-size deltas.

### Note

The default profile (`cargo build --no-default-features --features=fmt`)
is byte-identical to 1.3.0 on `thumbv6m-none-eabi`. None of the new
ecosystem features cross the embedded floor unless a user opts in.

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

[Unreleased]: https://github.com/ixmatus/ferrodec/compare/v1.6.0...HEAD
[1.6.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.6.0
[1.5.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.5.0
[1.4.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.4.0
[1.3.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.3.0
[1.2.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.2.0
[1.1.1]: https://github.com/ixmatus/ferrodec/releases/tag/v1.1.1
[1.1.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.1.0
[1.0.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.0.0
