# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.9.1] - 2026-05-06

### Fixed

- **`benches/core_ops.rs`** uses `Decimal128::parse_str` for its
  setup, but its `[[bench]]` entry in `Cargo.toml` lacked
  `required-features = ["fmt"]`. Under
  `cargo clippy --no-default-features --all-targets` the bench
  failed to compile because `parse_str` lives behind the `fmt`
  feature. Added the required-features pin; matches the
  declarative shape of `transcendentals` and `conversions`. CI
  doesn't currently run that exact clippy invocation, so the gap
  was only surfaced by ad-hoc local checks.
- **`src/ops/quantum.rs::tests::parse`** helper had a
  compile-time dispatch (`#[cfg(feature = "fmt")]` blocks inside
  the function body) that produced a "function never used"
  warning under `--no-default-features --all-targets`, since the
  tests calling `parse` were themselves cfg-gated to fmt. Replaced
  with a single `#[cfg(feature = "fmt")]` attribute on the helper
  itself; cleaner shape, no dead-code warning.
- **`tests/conformance.rs::parse_value`** documents the bare `#`
  null-test sentinel. A speculative routing attempt that mapped
  empty `#` to `(NaN, INVALID)` produced 28 status-mismatch
  failures because the runner's `invoke()` drops parse-time
  status to avoid bleeding flags into op results. Reverted; the
  null-test category stays in the skip bucket with an inline note
  pointing at the architectural follow-up that would close it
  (status threading through `invoke`). Conformance numbers
  unchanged from 1.9.0.

## [1.9.0] - 2026-05-06

The "conformance follow-ups" release. Closes four of the six
documented skip categories from 1.7.1's `KNOWN_ISSUES.md`. No
breaking changes to the existing public API; two surface
expansions (NaN-with-payload parse / display) make ferrodec
agree with IEEE 754:2019 §6.2.3 on diagnostic-payload behavior
that was previously dropped.

### Added

- **NaN-with-payload literal parse**. `Decimal128::parse_str`
  accepts `NaN<digits>` and `sNaN<digits>`, optionally with a
  leading sign. Payloads are decoded as decimal integers and
  stored in the BID significand's 110-bit `T_MASK` field
  (effective limit ≈ `10^33`). Larger payloads are rejected with
  `ParseDecimalError::InvalidCharacter` at the first overflowing
  digit. Empty payload behaves the same as the bare canonical
  token. Closes ~398 of the conformance suite's NaN-payload skip
  cases.
- **NaN-with-payload `Display`**. The `Display` / `LowerExp` /
  `UpperExp` impls now render the diagnostic payload as decimal
  digits when non-zero, e.g. `NaN22`, `-sNaN1234`. Zero payload
  still emits the canonical token (`NaN`, `sNaN`, etc.) so callers
  whose Display output previously matched canonical-NaN strings
  see no change.

### Fixed

- **NaN payload propagation in arithmetic**. Every NaN-producing
  arithmetic kernel — `add` / `sub` / `mul` / `div` / `rem` /
  `sqrt` / `fma`, the quantum ops (`quantize`, `scaleb`, `logb`,
  `next_up`, `next_down`), and the transcendentals (`exp`, `exp2`,
  `ln`, `log10`, `log2`, `cbrt`, `sin`, `cos`, `tan`, `asin`,
  `acos`, `atan`, `atan2`, `sinh`, `cosh`, `tanh`, `asinh`,
  `acosh`, `atanh`, `pow`) — now preserves the operand's NaN
  payload per IEEE 754:2019 §6.2.3 instead of returning the
  canonical zero-payload `Decimal128::NAN`. Two- and three-operand
  ops use the standard "first NaN wins" rule (operand `a` first,
  then `b`, then `c` for FMA). `sNaN` inputs continue to raise
  `INVALID` and convert to a quiet NaN with the same payload.
  Inline `Decimal128::NAN` returns are kept where the result is a
  *fresh* NaN (no NaN operand): `0/0`, `Inf − Inf`, `sqrt(-x)`,
  `pow(negative, non-integer)`, `0 × ∞` for FMA, etc.

  Helpers in `src/ops/nan_propagate.rs` (`nan_from`,
  `propagate_nan2`, `propagate_nan3`); pure addition, no public
  API change.

### Changed

- **Conformance runner** (`tests/conformance.rs`):
  - `parse_value` decodes decTest's hex `#XXXX...` operand syntax
    via `u128::from_str_radix(rest, 16)` + `Decimal128::from_bits`
    (closes ~28 cases). Bare `#` (no hex chars, the "null test"
    sentinel) still returns `None` and skips, matching pre-1.9.0
    behavior; this remains the only `#`-related residual skip
    category.
  - `class` op produces a `String` GDA class name
    (`+Normal`, `-Subnormal`, `+Infinity`, `NaN`, `sNaN`, etc.)
    via a new `classify_to_gda_name` helper, and `run_case`
    short-circuits class results before the value comparator
    (closes 40 cases).
  - `apply` op routes to identity (ferrodec is fixed at
    PRECISION=34, so `parse_str` already applies precision when
    constructing the operand). Closes 4 cases.
  - `decTest`'s `remainder` (truncating-quotient remainder)
    intentionally not routed: it's a different operation from
    `Decimal128::rem` (round-half-to-even quotient, IEEE 754
    §5.3.1), so the naive routing produced a real conformance
    failure on `dqrmn1070` and was reverted. The single
    `remainder` case stays in the skip bucket; documented as a
    follow-up in `KNOWN_ISSUES.md`.

### Conformance

- Suite total climbs from 8 149 / 0 / 572 (1.7.1) to
  8 591 / 0 / 130 across 8 721 cases. PASS_FLOOR raised to 8 591
  in two steps (8 149 → 8 193 from the runner-side closures,
  8 193 → 8 591 from the NaN-payload work). `KNOWN_ISSUES.md`
  rewritten around the new state.

## [1.8.1] - 2026-05-06

### Fixed

- `cargo test --no-default-features` failed to compile
  `tests/property_addsub.rs` and `tests/property_mul.rs` after the
  1.8.0 astro-float oracle landed. The oracle imports
  `tests/common/mod.rs` (which uses `Decimal128::parse_str`) and
  calls `format!("{a}")` (which needs `Display`); both live behind
  the `fmt` feature. The new oracle items are now gated on
  `#[cfg(feature = "fmt")]` (the `mod common` import, the
  `astro_float` imports, the `central_finite` strategy, the
  `oracle_op` / `oracle_mul` helpers, and the `proptest!` block
  containing the oracle test). The pre-1.8.0 tests in those files
  used construct-from-bits helpers plus `partial_cmp` / `to_bits`,
  none of which need `fmt`, so they still build and run on
  `--no-default-features`.

  No public API change. Caught by the new
  `--no-default-features` CI entry — the matrix expansion in 1.8.0
  was specifically meant to surface this kind of feature-gating
  oversight.

## [1.8.0] - 2026-05-06

The "verification depth + addsub correctness" release. Five
overlapping infrastructure additions plus one real arithmetic bug
fix that the new oracle surfaced. No public API changes.

### Fixed

- `add` (and `sub` via `add(-rhs)`) returned a 1-ULP-too-coarse
  result under directional rounding modes when the smaller-magnitude
  operand sat just past the kernel's `ALIGN_LIMIT`. Two
  representative cases:

  - `−1e−36 + (−1e−100)` under `TowardNegative` returned `−2e−36`
    instead of `−(10^33+1) × 10^−69` (the correct one-ULP-more-
    negative neighbor at the fine cohort).
  - `(699…99 × 10^−37) − (1 × 10^7)` (effective subtract with the
    fine-quantum operand smaller in magnitude) dropped the smaller
    operand entirely instead of preserving the digits that fit in
    the result's fine cohort, returning `−10^7` rather than the
    correct `−9999999.99993…`.

  Root cause: the kernel's `ALIGN_LIMIT` was a fixed `43`, the
  worst-case bound for `coef × 10^Δ` to fit in `U256` when `coef`
  has the maximum 34 digits. Smaller `coef` left slack the fixed
  limit didn't use, so single-digit `cl` operands with `Δ ≤ 76`
  mis-routed to the sub-ULP path and rounded at the *coarse*
  cohort.

  Replaced with `align_limit_for(d_l) = 77 − d_l`, so the exact-
  alignment regime tracks `cl`'s digit count. The residual sub-ULP
  effective-add path now pre-extends `cl` to PRECISION digits
  before the rounding pipeline so directional rounding bumps the
  coefficient at the fine cohort's ULP scale.

  Surfaced by the new astro-float oracle below; pinned by two
  regression tests in `src/ops/addsub.rs::tests`.

### Added

- **astro-float oracle** for `Decimal128::add`, `sub`, and `mul` in
  `tests/property_addsub.rs` and `tests/property_mul.rs`. 1000-bit
  `BigFloat` cross-check across all five IEEE rounding directions
  with a `within_ulps(1)` tolerance. Closes the documented `TODO`
  notes those files carried since 1.0.0. The 1-ULP envelope is
  structural — decimal exponents like `× 10^−20` have no exact
  binary representation, and the slack absorbs that without losing
  bug-catching value (the oracle is what found the addsub bug
  fixed above).
- **Four new libFuzzer targets** in `fuzz/`:
  - `transcendentals` — every transcendental kernel (`exp`, `exp2`,
    `ln`, `log10`, `log2`, `cbrt`, `sqrt`, `sin`, `cos`, `tan`,
    `asin`, `acos`, `atan`, `atan2`, `sinh`, `cosh`, `tanh`,
    `asinh`, `acosh`, `atanh`, `pow`) on arbitrary `u128` operand
    pairs; panic-freedom contract.
  - `integral` — `floor` / `ceil` / `trunc` / `round` /
    `round_ties_even` / `round_to_integral` / `round_to_integral_exact`
    panic-freedom + idempotence + `is_integer` for finite results.
  - `total_cmp` — reflexivity and antisymmetry of the §5.10
    `totalOrder` predicate plus `compare_total_magnitude` over
    arbitrary bit pairs (covers exactly the different-cohort surface
    Kani still leaves to proptest).
  - `encode` — `is_canonical` ↔ `canonicalize` fixed-point,
    canonicalize idempotence, classification stability across
    canonicalize.
  Plus `fuzz/Cargo.lock` for reproducible fuzzing builds.
- **`KNOWN_ISSUES.md`** at the repo root, categorising the 572
  conformance-suite skips into six concrete buckets (NaN-with-
  payload literals, non-IEEE rounding directives, `class` op result
  format, hex-encoded operands, unimplemented `apply` / `remainder`
  ops, residual parse failures), each with a fix path and rough
  cost. Closes the dangling `TODO` reference in
  `tests/conformance.rs:67`.

### Changed

- **CI matrix** widens. The Linux + macOS test matrix grows from 5
  to 7 feature combinations; the new entries are
  `--features serde,ops,num-traits` (the 1.4 / 1.5 ecosystem trio,
  previously uncovered) and `--all-features`. The `thumbv6m` cross-
  compile picks up the ecosystem trio plus an `--all-features` build.
  The MSRV check and the clippy job both move to `--all-features` so
  any feature-gated breakage surfaces. The Kani job loses the stale
  "(50 harnesses)" count from its label.

### Documentation

- README's Verification section bumps from "Two libFuzzer targets"
  to "Six libFuzzer targets" with one-line descriptions of each.

## [1.7.1] - 2026-05-06

### Fixed

- `next_up` (and `next_down`, which routes through it) panicked on
  non-canonical Infinity encodings — bit patterns where the type
  field is `0b11110` but the trailing 122 bits are non-zero (so
  `is_infinite()` returns true but the value doesn't bit-equal
  `Decimal128::INFINITY` / `NEG_INFINITY`). The previous code used
  bit-equality against the canonical Inf constants and hit
  `unreachable!()` when the equality failed. Replaced with
  `self.is_infinite()` plus a sign check.

  Surfaced by the `next_up_special_dispatch` Kani harness added
  in 1.7.0; the harness now passes. Regression test
  `next_up_non_canonical_infinity` covers both signs.

## [1.7.0] - 2026-05-06

The "verification depth" release. No public API changes. Closes the
two documented gaps in the verification surface: the post-v1 §5
operations had no Kani coverage, and `total_cmp`'s finite-finite
antisymmetry has been deferred since 1.0.0.

### Added

- 12 new Kani harnesses across three new files:
  - `src/verify/canonical.rs` — `is_canonical` / `canonicalize`
    idempotence, projection, and fixed-point characterisation over
    arbitrary `u128`. The canonical pair is now Kani-checked.
  - `src/verify/quantum.rs` — `same_quantum` reflexivity and
    symmetry, `compare_total_magnitude` reflexivity and
    antisymmetry-off-finite-finite, `radix()` constant, and
    `next_up` special-case dispatch.
  - `src/verify/decimal.rs` — `try_new` in-range success returning
    matching decoded bits, plus the two error-variant cases.
- 1 new harness in `src/verify/cmp.rs`:
  - `total_cmp_antisymmetric_finite_same_cohort_same_sign`. Closes
    the easy half of the documented TODO. The remaining
    different-cohort case still hits SMT-multiplication and stays
    proptest-only; the file's docstring records the partial closure.
- 3 new property test files in `tests/`:
  - `property_canonical.rs` — randomised consistency between
    `is_canonical` and `canonicalize` (3 properties).
  - `property_quantum.rs` — `next_down` inverts `next_up`
    numerically, `next_up` is strictly greater than its input,
    `same_quantum` and `compare_total_magnitude` reflexivity over
    arbitrary bit patterns (4 properties).
- 1 new test in `src/math/extended.rs::tests`:
  - `round_trip_decimal128_sweep` — `Decimal128 → Extended →
    Decimal128` is bit-identical for every combination of
    representative coefficients × representative quanta. Pins the
    contract the transcendental kernels rely on.

### Changed

- `src/verify/cmp.rs`'s docstring rewritten to describe the
  Phase 2 closure rather than the original Phase 1 deferral.
- README "Verification" section: harness count bumped from 50 to 63,
  with a sentence describing what each new cluster proves.

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

[Unreleased]: https://github.com/ixmatus/ferrodec/compare/v1.7.1...HEAD
[1.7.1]: https://github.com/ixmatus/ferrodec/releases/tag/v1.7.1
[1.7.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.7.0
[1.6.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.6.0
[1.5.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.5.0
[1.4.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.4.0
[1.3.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.3.0
[1.2.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.2.0
[1.1.1]: https://github.com/ixmatus/ferrodec/releases/tag/v1.1.1
[1.1.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.1.0
[1.0.0]: https://github.com/ixmatus/ferrodec/releases/tag/v1.0.0
