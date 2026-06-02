# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `Decimal32::from_parts`, the `const` inverse of `decode`. Reconstructs a
  finite value from `{ negative, coefficient, exponent }`, returning `None`
  when the coefficient or exponent is out of range. Always available (no
  `fmt` feature), and unlike `try_new` it carries an explicit sign, so it
  can build negative zero. Forms a documented bijection with `decode` on
  canonical finite values, proved by Kani in both directions.
- `Decimal32::from_str_const` and the `dec!` macro, for embedding exact
  published constants in `const` initializers
  (`Decimal32::from_str_const("9.80665")` or `dec!("...")`). A `const`
  decimal literal parser that requires exact representability: a malformed,
  oversized, or out of range literal is a compile error, never silent
  rounding. Finite only, gated by `fmt`. Equivalence with `parse_str` on
  the exactly representable subset is pinned by a property test. See
  ADR-0037.

## [3.1.0] - 2026-05-31

### Added

- `Decimal32::decode` and the `Decimal32Parts` struct. A quantum preserving
  accessor that decomposes a finite value into `{ negative, coefficient, exponent }`,
  returning `None` for NaN and infinity. The represented value is exactly
  `(-1)^negative * coefficient * 10^exponent`. Covered by unit tests, a proptest
  round trip over the full bit space, and a Kani totality harness.

## [3.0.0] - 2026-05-30

Released lockstep with the `ferrodec` (Decimal128) parity train
(ADR-0035). The parent reached major version 3.0.0 for a breaking
binary float constructor change (ADR-0036); Decimal32 carries no
breaking change and bumps to 3.0.0 only to keep the workspace versions
aligned. The changes are additive public API, two behavior fixes ported
up from `decimal64`, a conversion accuracy fix, and new GDA cross check
tests.

### Added

- **Public API parity with Decimal128 (fd-92w.11).** Inherent `signum`,
  `is_integer`, and `ulp` on `Decimal32`; `radix()` returning 10;
  inherent `from_f32(f32, RoundingMode) -> (Decimal32, Status)`; `Sum`
  and `Product` over owned and borrowed iterators; and the `pi`, `e`,
  `ln2`, `ln10` constants (gated under `trig` or `exp-log`, as on the
  parent). Each item is adapted to Decimal32's coefficient width and
  parameters rather than pasted from the parent. All additive.
- **Display honors the precision specifier.** `{:.N}`, `{:.Ne}`, and
  `{:.NE}` now quantize and pad as on the parent, where they were
  previously ignored.
- **GDA decNumber extension cross check tests (fd-92w.12).** The
  `decimal32` analogue of the ADR-0031 cross check vectors, previously
  promised but absent.

### Fixed

- **`sub` no longer flips a NaN operand's sign.** Matching the parent and
  `decimal64` fix, a guard preserves the propagated NaN sign instead of
  toggling it through the operand negation (ADR-0035).
- **`CLAMPED` is raised on the `mul` quantum clamp path** where
  `decimal64` already emitted it (ADR-0035).
- **`FromPrimitive::from_i64` / `from_u64` are exact.** The conversion
  now rounds the integer directly to the 7 digit format precision rather
  than through an `f64` intermediate, removing a double rounding
  (ADR-0035).

## [2.2.0] - 2026-05-21

Sibling-only release restoring conversion API parity with the
`ferrodec` (Decimal128) parent and the `ferrodec-decimal64`
sibling. The parent crate stays at 2.1.0; the change is
sibling-only and additive, hence SemVer minor.

### Added

- **From-integer constructors (fd-o98).** `Decimal32::from_i32` /
  `from_u32` / `from_i64` / `from_u64` / `from_i128` / `from_u128`,
  all taking a `RoundingMode` and returning `(Decimal32, Status)`.
  No constructor is exact: Decimal32's 7-digit precision is
  narrower than any standard integer type, so every conversion
  may round. The helper packs a `u128` absolute value down to a
  `u64` coefficient with sticky tracking, then delegates the
  precision-boundary round to `round_and_pack_finite` which
  narrows the kept u64 to Decimal32's 7-digit canonical form.
- **`impl TryFrom<f64>` / `impl TryFrom<f32>` for `Decimal32`
  (fd-o98).** Behind the existing `binary-float` feature gate.
  NaN rejects with `Decimal32FromFloatError::NotANumber`, ±∞
  with `Infinite`; finite values flow through
  `Decimal32::from_f64(value, RoundingMode::NearestEven)`. Very
  large finite f64 magnitudes saturate to `±∞` per the standard
  f64-to-decimal behaviour; the caller must check `is_finite` if
  that distinction matters.
- **`Decimal32FromFloatError`.** New public error type matching
  the Decimal128 parent's `Decimal128FromFloatError` shape, with
  the per-format identifier in `Display`. Behind `binary-float`.

No `impl From<intN>` impls are provided. Lossless `From` requires
the integer type to fit Decimal32's 7-digit precision envelope,
which none of `i32` / `u32` / `i64` / `u64` / `i128` / `u128` do
(even `i32`'s 10 decimal digits exceed 7).

### Changed

- **`src/lib.rs` now `include_str`s the crate `README.md`** into
  the rustdoc, matching the parent's pattern. The docs.rs
  Decimal32 page now renders the full README narrative after the
  lib.rs preamble.
- **README backtick polish for `x86_64`, `total_cmp`, `no_std`**
  in three lines that were prose-rendered before the
  include_str. No content change.

The shared infrastructure crates stay at their current versions.

## [2.1.0] - 2026-05-21

The §9.2 transcendental contract tightens from faithful (≤ 1 ULP at
7 digits, ADR-0024) to correctly rounded (the single nearest
representable `Decimal32` value at every IEEE 754-2019 rounding
direction, ADR-0032). The change ships lockstep with `ferrodec`
2.1.0 and `ferrodec-decimal64` 2.1.0; the shared
`ferrodec-transcend` Extended kernel that all three formats share
is unchanged at 50 decimal digits of working precision, so latency
is unchanged from 2.0.0. SemVer minor: correctly rounded is a
strict tightening of faithful.

### Changed

- **§9.2 transcendentals are correctly rounded (fd-1pv, ADR-0032,
  supersedes ADR-0024).** `exp`, `ln`, `exp2`, `log2`, `log10`,
  `cbrt`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`,
  `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, and `pow` now
  carry the correctly rounded contract across the supported domain
  on every IEEE 754-2019 rounding mode.
  - Evidence: the ADR-0026 frozen Arb corpus (`tests/transcend_vectors.rs`,
    renamed to `frozen_arb_vectors_correctly_rounded`) gates exact
    match per vector; 2338 `Decimal32` vectors across five rounding
    modes including binary `pow` and `atan2` all land on the
    correctly rounded value. MPFR confirms.
  - Mechanism: Lefèvre / Muller wider fixed working precision with
    rigorous a priori error bounds; the 50 digit kernel clears the
    smallest empirical Arb worst case half ULP margin (`cosh` at
    `Decimal32`, 4.167e-8, the smallest margin in the entire
    corpus) by more than thirty orders of magnitude. The per
    function derivations live in the shared `ferrodec-transcend`
    module rustdoc.

The shared infrastructure crates (`ferrodec-ieee`,
`ferrodec-multiword`, `ferrodec-transcend`, `ferrodec-test-support`)
stay at their current versions per ADR-0029's intent.

## [2.0.0] - 2026-05-21

The first major-version release. Two consolidated breaking changes
that ADR-0029 froze into the 2.0 set, shipped together with the
parent `ferrodec` 2.0.0 and the `ferrodec-decimal64` 2.0.0. The
shared-infrastructure crates (`ferrodec-ieee`, `ferrodec-multiword`,
`ferrodec-transcend`, `ferrodec-test-support`) are unchanged.

### Breaking

- **Retired the ambiguous bare `rem` method (fd-9n5, ADR-0027 (b),
  ADR-0029 item 1).** The 1.x bare `Decimal32::rem(self, other,
  _rm: RoundingMode)` is the GDA / C99 `fmod` truncated remainder
  (`r = a − trunc(a / b) × b`); the parent crate's bare
  `Decimal128::rem` was the *nearest-even* remainder, so the same
  name carried different math depending on the type. 2.0 retires
  the ambiguous spelling.
  - `Decimal32::rem` is renamed in place to `Decimal32::rem_trunc`.
    The (always unused) trailing `RoundingMode` argument is dropped
    in the rename; the new signature is
    `pub fn rem_trunc(self, other: Self) -> (Self, Status)`.
  - `Decimal32::rem_near` (the IEEE 754-2019 §5.3.1 nearest-even op
    that shipped in 1.6.0) is unchanged.
  - The `core::ops::Rem` (`%`) and `core::ops::RemAssign` impls are
    retained (so `impl num_traits::Num` continues to hold under the
    `num-traits` feature) but now route through `Decimal32::rem_trunc`
    explicitly; the dispatch destination is the rule `%` already had
    on this format in 1.x.
  - **Migration.** Rewrite `value.rem(other, rm)` call sites as
    `value.rem_trunc(other)`. Code that uses `%` does not change.

- **Widened `ParseDecimalError` (fd-7f1, ADR-0018 L14, ADR-0029
  item 2).** The 1.x enum collapsed every parse failure into four
  coarse variants. 2.0 widens the enum so callers can match on each
  failure mode.
  - New shape (byte-identical with the parent and the decimal64
    sibling):
    ```text
    #[non_exhaustive]
    enum ParseDecimalError {
        Empty,
        MisplacedSign       { position: usize },
        InvalidCharacter    { position: usize },
        InvalidExponent     { position: usize },
        ExponentOutOfRange,
        CoefficientOverflow,
    }
    ```
    Variants that carry a position are struct-shaped. The enum is
    `#[non_exhaustive]`.
  - `MisplacedSign` is new: inputs like `"+-1"`, `"1+2"`, `"1e++3"`
    now report `MisplacedSign { position: idx }` rather than
    `InvalidCharacter(idx)`.
  - `CoefficientOverflow` is new and absorbs the H8 saturation cases
    (`leading_fractional_zeros_past_cap`,
    `trailing_integer_zeros_past_cap`) that previously routed to the
    catch-all `ExponentOutOfRange`. Code that asserted
    `Err(ParseDecimalError::ExponentOutOfRange)` on those inputs
    now matches `Err(ParseDecimalError::CoefficientOverflow)`.
  - **Migration.** Rewrite tuple-shape patterns
    (`Err(ParseDecimalError::InvalidCharacter(n))`) as
    `Err(ParseDecimalError::InvalidCharacter { position: n })`.
    Exhaustive matches need a wildcard arm or a `MisplacedSign` /
    `CoefficientOverflow` arm. Serde decode errors that route through
    `Display` see the new wording (`"misplaced sign at byte N"`,
    `"coefficient digit count out of range"`).

### Added

- `Decimal32::fixed_preferred()` returns a `FixedPreferred(Decimal32)`
  adapter whose `Display` impl applies the 1.x parent
  `Decimal128::Display` rule (plain notation preferred when the
  scale fits `(-6, 21]`, otherwise scientific). Additive — the
  default `Decimal32::Display` continues to follow GDA `toSci`, the
  rule it always used — exposed so callers can opt into the legacy
  parent rendering uniformly across the family alongside the
  parent's new `Decimal128::fixed_preferred()`. ADR-0014, ADR-0029
  item 3.

### Notes

- The parent `ferrodec` 2.0.0 harmonized `Decimal128::Display` onto
  GDA `toSci`. The default `Decimal32::Display` is unchanged in
  this release (it was already `toSci`); the cross-format
  inconsistency in 1.x is resolved on the parent side.
- The shared infrastructure crates (`ferrodec-ieee` 0.1.4,
  `ferrodec-multiword` 0.1.0, `ferrodec-transcend` 0.1.0,
  `ferrodec-test-support` unpinned) are unchanged and stay at
  their pre-2.0 versions.
- The conformance suite re-ran with zero per-bucket count drift.
  The `rem` rename was source-side only; the `ParseDecimalError`
  widening did not change which inputs the harness skips.

## [1.8.0] - 2026-05-20

### Added

- `Decimal32::reduce`: General Decimal Arithmetic trailing-zero
  strip. No upstream `dsReduce.decTest` exists (decSingle was
  historically storage-only); verification rests on hand-derived
  unit tests at PRECISION = 7. ADR-0031.
- `Decimal32::divide_integer`: General Decimal Arithmetic truncated
  integer quotient at exponent 0. Unit-tested at PRECISION = 7;
  10^7 / 1 is the `Division_impossible` boundary. ADR-0031.
- `Decimal32::logical_invert`: digit-wise complement of a logical
  operand, padded to 7 digits. NaN of any kind raises `INVALID`
  (the logical-op uniform rule). Unit-tested. ADR-0031.
- `Decimal32::logical_and` / `logical_or` / `logical_xor`:
  digit-wise truth-table ops over two logical operands. Unit-tested.
  ADR-0031.
- `Decimal32::shift`: coefficient-digit shift inside the
  precision-wide window with zero fill. Unit-tested at the
  precision-7 boundary. ADR-0031.
- `Decimal32::rotate`: coefficient-digit modular rotation inside
  the precision-wide window. Unit-tested. ADR-0031.

### Fixed

- `Decimal32::fma`: subnormal results are now single-rounded,
  fixing the double-rounding family the standing fd-dpg
  exact-oracle sweep surfaced. The funnel rounded the exact product
  to PRECISION first and then re-rounded into the subnormal
  quantum; a residue straddling the two rounding boundaries was
  collapsed into an exact tie and carried the wrong way (e.g.
  `fma(3.142290e-17, -2.033196e-78, 5.38890e-95)` ended `…992` for
  the correctly-rounded `…991`). The kernel now drops to the wider
  of the precision and subnormal-quantum requirements in one
  rounding, the IEEE 754-2019 single-rounding contract, matching
  the parent `Decimal128`. ADR-0030.

## [1.7.0] - 2026-05-18

### Added

- `Decimal32::min_magnitude` / `Decimal32::max_magnitude`: the
  IEEE 754-2019 §9.6 `minimumMagnitudeNumber` /
  `maximumMagnitudeNumber` operations (smaller / larger numeric
  magnitude; an equal-magnitude tie defers to `min` / `max`; NaN
  handling as for `min` / `max`). Byte-identical semantics to the
  `ferrodec` and `ferrodec-decimal64` siblings; unit-tested (no
  `ds*` magnitude conformance vectors are vendored). Additive and
  non-breaking; ADR-0028 records the design and the
  `…Number`-variant choice.

## [1.6.0] - 2026-05-18

### Added

- `Decimal32::rem_near`: the IEEE 754-2019 §5.3.1 nearest-even
  remainder (`r = a − n·b` with `n` the round-half-even integer
  quotient, `|r| ≤ |b|/2`, always exact), the sibling analogue of
  `Decimal128::rem`. The existing `Decimal32::rem` is the *truncated*
  remainder and is unchanged; the two names previously had no
  nearest-even counterpart on this format, a documented API hazard
  (ADR-0027). Validated bit-for-bit against the exact integer oracle
  over the full finite encoding domain. Additive and non-breaking;
  ADR-0027 records the 2.0 plan to retire the ambiguous bare `rem`
  and `%` spellings.

## [1.5.0] - 2026-05-17

### Changed

- `Decimal32::exp`, `Decimal32::ln`, and `Decimal32::cbrt` are now
  faithfully rounded (≤ 1 ULP at 7 digits, every IEEE 754-2019
  rounding direction) via the shared `ferrodec-transcend`
  Extended-precision kernel, replacing the lossy `f64` / `libm`
  detour. The kernel is the same verified implementation the
  `ferrodec` (Decimal128) parent and the `ferrodec-decimal64` sibling
  use, instantiated at `F = Decimal32` through the `DecimalFormat`
  seam. The rewrite also reaches `Decimal32`'s true domain rather than
  the narrower `f64` saturation window. This is behaviour-improving,
  not a bug fix: previously wrong-by-rounding results become correctly
  faithful, and previously saturated inputs now return their true
  finite value. Special-value semantics (NaN / infinity / zero /
  negative) and the ADR-0016 Kani shims are byte-identical to before;
  only the finite result path changes. New dependencies on the
  `ferrodec-transcend` and `ferrodec-multiword` workspace crates
  (pulled by `exp-log`) change the published dependency graph;
  `ferrodec-decimal32` itself stays astro-float-free (the
  faithful-rounding oracle compiles only inside the
  `ferrodec-test-support` dev-dependency).

- `Decimal32::sin`, `cos`, `tan`, `asin`, `acos`, `atan`, and
  `atan2` are now faithfully rounded (≤ 1 ULP at 7 digits, every
  IEEE 754-2019 rounding direction) via the shared
  `ferrodec-transcend` Extended-precision kernel, replacing the lossy
  `f64` / `libm` detour, at exact parity with the Decimal128 parent
  and the `ferrodec-decimal64` sibling. The forward functions use the
  Payne-Hanek argument reduction, faithful across the full
  `Decimal32` magnitude range. Behaviour-improving, not a bug fix.
  The `asin` / `acos` `|x| > 1` domain INVALID is now decided at
  Extended precision rather than on a rounded f64 value;
  special-value semantics and the ADR-0016 Kani shims are
  byte-identical to before. The faithful contract is proven by the
  new `tests/property_sincos.rs`, `tests/property_sincos_large.rs`,
  and `tests/property_inverse_trig.rs` suites, which stay
  astro-float-free through the shared `transcend_oracle` builders.
  The `trig` feature now pulls the `ferrodec-transcend` /
  `ferrodec-multiword` workspace crates (already in the graph via
  `exp-log`).

- `Decimal32::sinh`, `cosh`, `tanh`, `asinh`, `acosh`, and `atanh`
  are now faithfully rounded (≤ 1 ULP at 7 digits, every IEEE
  754-2019 rounding direction) via the shared `ferrodec-transcend`
  Extended-precision kernel (built on the already-faithful `exp` /
  `ln` primitives), replacing the lossy `f64` / `libm` detour, at
  exact parity with the Decimal128 parent and the
  `ferrodec-decimal64` sibling. The hyperbolic family is faithful
  across the full `Decimal32` magnitude range up to the true
  overflow boundary (the pre-fd-r0l `sinh` / `cosh` f64-overflow cap
  is lifted). Behaviour-improving, not a bug fix. The `acosh`
  `x < 1` domain INVALID and `acosh(1) = 0`, and the `atanh`
  `|x| == 1` pole (`±∞ + DIV_BY_ZERO`) and `|x| > 1` domain INVALID,
  are now decided at Extended precision rather than on a rounded f64
  value; special-value semantics and the ADR-0016 Kani shims are
  byte-identical to before. The faithful contract is proven by the
  new `tests/property_hyperbolic.rs` suite, which stays
  astro-float-free through the shared `transcend_oracle` builders.
  The `hyperbolic` feature now forwards
  `ferrodec-transcend/hyperbolic` (the `ferrodec-transcend` /
  `ferrodec-multiword` crates were already in the graph via
  `exp-log`). `hyper.rs` was the last functional caller of the
  internal `f64`-routed `ops/f64_bridge.rs` adapter (trig left it in
  the P3 phase; `pow` used `libm::pow` directly), so the now-dead
  `f64_bridge` shim was removed in the same change.

- `Decimal32::pow` is now faithfully rounded (≤ 1 ULP at 7 digits,
  every IEEE 754-2019 rounding direction) via the shared
  `ferrodec-transcend` Extended-precision kernel, replacing the lossy
  `f64` / `libm::pow` detour. `pow(x, y)` evaluates
  `exp(y · ln(|x|))` entirely at Extended precision (with the
  bit-exact integer-exponent fast path), at exact parity with the
  Decimal128 parent through the `DecimalFormat` seam.
  Behaviour-improving, not a bug fix: the negative-base /
  non-integer-exponent INVALID and the `pow(±0, y)` / `pow(±∞, y)` /
  `pow(x, ±∞)` rules are now decided at Extended precision rather
  than on a rounded f64 exponent. The `pow_special_cases`
  short-circuit and the ADR-0016 Kani shim are byte-identical to
  before. The faithful contract is proven by the new
  `tests/property_pow.rs` and `tests/property_pow_specials.rs`
  suites (astro-float-free, Design A: the oracle reaches them only
  through the shared `ferrodec-test-support` builders). The `pow`
  feature now forwards `ferrodec-transcend/pow` (the workspace
  crates were already in the graph via `exp-log`).

- `libm` is no longer a dependency of this crate. It had been
  retained only for the pre-fd-r0l f64 transcendental detour; with
  `pow` now on the shared kernel, no `src/` code makes any
  functional `libm` call, so `dep:libm` was dropped from the
  `exp-log` / `trig` feature arrays and the `libm` dependency line
  removed. This shrinks the published dependency graph; the public
  surface is unchanged.

### Added

- `Decimal32::exp2`, `Decimal32::log2`, and `Decimal32::log10`,
  faithfully rounded (≤ 1 ULP at 7 digits, every IEEE 754-2019
  rounding direction) via the shared `ferrodec-transcend` kernel as
  pure delegations (the kernel resolves every special case, exactly
  as on the Decimal128 parent). They ship under the existing
  `exp-log` feature. With `exp` / `ln` / `cbrt` now faithful (above),
  the exp-log family reaches exact capability parity with the
  Decimal128 parent and the `ferrodec-decimal64` sibling, closing the
  documented asymmetry. The faithful contract is proven by the new
  `tests/property_exp.rs`, `tests/property_ln.rs`, and
  `tests/property_cbrt.rs` suites across every rounding direction.

## [1.4.0] - 2026-05-16

The decimal32 correctness train, the sibling of the decimal64 1.4.0
slice. ADR-0017 and ADR-0018 deferred decimal32 to its own slice on
the reasoning that its `KNOWN_ISSUES` was mostly coverage gap with no
confirmed defect. A `Decimal64` cross-check oracle was stood up
(every finite `Decimal32` is exactly representable in the now
conformance validated `Decimal64`), and it refuted that reasoning
immediately: a six agent review (ADR-0010 methodology) over the whole
op surface produced eight H, six M, and four L findings. This release
closes them, ports the Kani special case coverage to parity with
decimal64, adds a DPD interchange codec, and records the train in
ADR-0019. See the parent `ferrodec` crate's CHANGELOG for cycle
context.

This is a minor bump rather than a patch: several fixes change
observable outputs on inputs that were previously wrong against the
spec, and `to_f64` gains a breaking signature change (below). A
downstream consumer that implicitly relied on the old behaviour
should read the Changed section. The version posture mirrors the
deliberate decimal64 1.4.0 decision (ADR-0018).

### Changed

- **BREAKING**: `Decimal32::to_f64` signature changes from
  `fn to_f64(self) -> f64` to
  `fn to_f64(self, rm: RoundingMode) -> (f64, Status)`. A signaling
  NaN input now raises `Status::INVALID` per IEEE 754-2019 §5.4.2
  instead of being silently quieted (H6). Decimal32 to f64 is exact
  (a 7 digit coefficient and the Decimal32 exponent range fit
  binary64 without rounding), so no `INEXACT` is raised. Migration:
  pass a rounding mode and take `.0` for the value, `.1` for the
  status.

- `<Decimal32 as ToPrimitive>::to_f32` now routes through a new
  inherent `Decimal32::to_f32(self, RoundingMode) -> (f32, Status)`
  that renders the decimal once onto the binary32 grid, instead of
  the old `to_f64(..) as f32` double rounding chain, and it now
  signals on a signaling NaN (H7). The numeric result is unchanged
  for every Decimal32 (the old f64 step was already exact at 7
  digits); the change removes the double rounding error class
  structurally and restores the lost signal.

### Added

- `Decimal32::to_f32(self, RoundingMode) -> (f32, Status)`, the
  single correctly rounded decimal to binary32 path (H7).

- A DPD interchange codec behind the off by default `dpd` feature:
  `Decimal32::to_dpd_bytes(self) -> [u8; 4]` and
  `from_dpd_bytes([u8; 4]) -> Self`, pure IEEE 754-2008 §3.5.2
  boolean declet equations with no lookup tables (ADR-0009 posture;
  N+1). BID stays the arithmetic storage encoding; DPD is a byte
  level adapter. `no_std` clean.

- **M4** an exact integer conversion surface,
  `Decimal32::to_i32 / i64 / i128 / u32 / u64 / u128`, each
  `(self, RoundingMode) -> (T, Status)` per IEEE 754-2019 §5.4.1,
  replacing the previous f64 plus `libm_round` detour in the
  `num-traits` delegates (no double rounding, correct None iff
  `INVALID`). The decimal64 M5 shape. Followup (fd-fq6): with the
  `libm_round` detour gone the `num-traits` feature no longer needs
  `dep:libm`, so it is dropped from that feature's enable list;
  `libm` still resolves for users through the transcendental gates
  (`exp-log`, `trig`, ...). The decimal64 fd-17 analogue.

### Fixed

- **H1** `Decimal32::add` / `sub`: the static `ALIGN_LIMIT` /
  `WORKING_PRECISION` alignment window dropped the lower operand's
  residue and treated a signed zero as the dominant operand,
  losing magnitude. Replaced with a dynamic per side shift over a
  u128 register keyed on the actual digit count, plus an explicit
  zero operand fast path (IEEE 754-2019 §5.4.1, §6.3). Example:
  `add(-1E-101, 1E-88, TowardZero)` returned `1.000000E-88`, now
  `9.999999E-89`; `sub(-0E-74, -3.145728E-95)` returned `1E-101`,
  now `3.145728E-95`. The decimal64 fd-d47 and H1 shape.

- **H2** `Decimal32::rem`: the static `MAX_SAFE_SHIFT` raised a
  spurious `Invalid_operation` on operand pairs whose true integer
  quotient is small. Replaced with the dynamic per side bound;
  `quotient >= COEFFICIENT_LIMIT` is the sole `INVALID` predicate
  (IEEE 754-2019 §5.3.1, §7.2). Example: `rem(1E+13, 9999999)`
  returned `(NaN, INVALID)`, now `1.000000E+6`. The decimal64 H5
  shape. The cross-check rem oracle was also corrected: it had been
  unsound when the integer quotient has 8 to 16 digits.

- **H3** typed `BiasedExp` and `Coefficient` newtypes in `bid.rs`
  lift the former `pack_finite` `debug_assert!` preconditions into
  the type system, and the FMA both zero and exact cancellation
  early returns now clamp the §6.3 preferred quantum and raise
  `Status::CLAMPED` instead of wrapping an out of range biased
  exponent into garbage bits in release builds (IEEE 754-2019 §6.3,
  §7.4). The decimal64 H3 shape.

- **H4** `Decimal32::fma`: the overflow early returns now apply the
  effective subtract borrow and extend, so a directed rounding no
  longer tips one ULP the wrong way on an opposite sign residue
  (IEEE 754-2019 §4.3, §7). The decimal64 fd-d47 mirror.

- **H5** `Decimal32::quantize`: a zero coefficient at a deep target
  quantum no longer returns `(NaN, INVALID)`; a zero is
  representable at every encodable quantum (IEEE 754-2019 §5.3.3).
  The decimal64 H6 shape.

- **H8** `Decimal32::parse_str`: the implicit exponent counters are
  saturated and capped at `MAX_EXPONENT_MAGNITUDE`, closing an
  adversarial input path that panicked in debug builds and wrapped
  silently in release (IEEE 754-2019 §5.12). Security fix.

- **M2** `Decimal32::scaleb`: an out of envelope `n`
  (`|n| > 2 * (Emax + precision)`) now returns `(NaN, INVALID)`
  before the exponent arithmetic, which also closes an i32 overflow
  on extreme `n` (IEEE 754-2019 §5.3.3). The decimal64 M2 shape.

- **M3** `Decimal32::from_f64`: a binary64 signaling NaN bit pattern
  now raises `Status::INVALID` (IEEE 754-2019 §5.4.2). The
  decimal64 M3 shape.

- **L1** `Display` engineering notation: a zero coefficient is
  rendered as a lone `0` at the adjusted exponent instead of being
  padded with positional zeros (`0E+5` rendered `000E+3`). The
  decimal64 L13 shape.

### Verification

- **M5** the five Kani special case shim groups decimal32 lacked
  (`exp`/`ln`, `trig`, `hyper`, `pow`/`cbrt`, the quantum family)
  are ported under the ADR-0016 routing rule, bringing decimal32's
  proof coverage level with decimal64. Kani, all 0 failures (the
  quantum group is unconditional, so it stacks into every run): 37
  under `fmt`, 47 under `exp-log`, 49 under `trig`, 54 under
  `hyperbolic`, 55 under `pow`; no CBMC budget skip needed.

- **M6** the `Decimal64` cross-check is the formalized permanent
  arithmetic regression net (`tests/d64_crosscheck.rs`): add,
  subtract, multiply, divide active plus the GDA correct
  `rem_oracle_check`, seven blocks, zero ignored. A randomized
  `Display` then `parse_str` round-trip guard (L2) and the closing
  audited safe invariant comments (L3) and the rem proof note (L4)
  round out the L tier.

- **N+1** conformance: with the `dpd` feature on,
  `dsEncode.decTest` passes 250 of 268 (up from 2), `dsBase`
  unchanged at 698; the per file expected counts are
  feature conditional and exact match (ADR-0010). The 18 residual
  `dsEncode` skips are the `value -> #hex` cases carrying a
  `Clamped` condition, the same §7.4 `parse_str` quantization
  policy edge tracked for `dsBase`, not a codec defect.

### Known issues

- `parse_str` does not apply the IEEE 754-2019 §7.4 preferred
  exponent clamp, so a small set of `dsBase` and `dsEncode` cases
  skip. A cross crate quantization policy decision, see
  `KNOWN_ISSUES.md`.

- Transcendentals route through `f64` / `libm` (faithfully rounded,
  not correctly rounded). Documented v1.0 baseline; routing through
  Decimal128's `Extended` kernel is a 1.16 era follow up.

## [1.3.0] - 2026-05-11

The post-publish six-agent correctness review (2026-05-10) found
two real IEEE 754-2019 violations in the 1.2.0 release plus a
spec-but-misimplemented integer-conversion path. The 1.15 cycle of
the parent `ferrodec` crate fixed them across Slice A. See the
parent crate's CHANGELOG for the full cycle context and ADR-0016
for the Kani-harness convention this slice inherited.

### Fixed

- `Decimal32::fma`: the `0 × ∞ + NaN_c` branch now propagates `c`'s
  payload per IEEE 754-2019 §6.2.3. Pre-1.3.0 the branch returned
  canonical `Decimal32::NAN` regardless of `c`, silently discarding
  the input NaN's signal. Sibling drift from the
  correctly-implemented `Decimal128::fma`; same shape, same fix.

- `Decimal32::min` and `Decimal32::max`: signaling NaN's payload is
  now preserved (signal cleared) on the `INVALID` arm per
  §6.2.3. Pre-1.3.0 the methods returned `Self::NAN` (canonical
  zero payload) on any sNaN input.

- `<Decimal32 as FromPrimitive>::from_i64` / `from_u64`: accept
  integers above `2^53` with rounding instead of returning `None`.
  The pre-1.3.0 gate refused any `|n| > 2^53` on the (mistaken)
  grounds that the f64 round-trip would be lossy. Decimal32 holds
  only 7 digits, so the actual rounding is bounded by Decimal32's
  envelope, not f64's. The new path tries `Self::try_new(coef, 0)`
  for exact fits and falls through to the f64 round-trip otherwise
  — matching Decimal64's existing convention.

- `serde_bid::deserialize`'s `BitsVisitor` accepts the full
  integer-width matrix (u8 / u16 / u32 / u64 unsigned, i8 / i16 /
  i32 / i64 signed). MessagePack, CBOR, and bincode can hand
  integers in any of those widths; the pre-1.3.0 visitor rejected
  everything narrower than u32.

### Verification

- Decimal32's verify tree now follows the `_special_only_for_kani`
  shim convention introduced in the parent crate (ADR-0016). Each
  `src/ops/<op>.rs` exposes a `#[cfg(kani)]` shim that returns the
  special-case dispatcher's `Option` directly, bypassing the
  finite-finite alignment / rounding pipeline that CBMC can't
  tractably enumerate. Every `src/verify/<op>.rs` harness routes
  through these shims. Local timing: every harness finishes in
  under 1.3 seconds (pre-1.3.0 several timed out the 60-second
  budget the runner used to enforce).

- New harnesses: `add_special_resolves_on_nan_or_infinity`,
  `sub_special_resolves_on_nan_or_infinity`,
  `mul_special_resolves_on_nan_or_infinity`,
  `div_special_resolves_on_nan_infinity_or_zero_divisor`,
  `sqrt_special_resolves_on_non_positive_finite`,
  `add_snan_raises_invalid`, `mul_snan_raises_invalid`,
  `div_snan_raises_invalid`, `sqrt_snan_raises_invalid`,
  `min_max_snan_preserves_payload` (pinning the Slice A fix above).

- `total_cmp_no_panic_and_total` is renamed to
  `total_cmp_reflexive` to match the actual claim — the harness
  proves reflexivity, not totality (which is implicit in
  `total_cmp` returning `Ordering` rather than `Option<Ordering>`).

### Known issues

- New file `KNOWN_ISSUES.md` catalogues two coverage gaps inherited
  from before this release:
  * `dsEncode`'s `#hex` BID-interchange dispatch (266 of 268 cases
    skip pending a dedicated `#hex` decoder arm).
  * Transcendentals route through f64 / libm rather than
    Decimal128's `Extended` kernel; documented as a v1.0 baseline.
    Routing through Extended needs an architectural decision and
    is tracked for 1.16-era.

- No correctness bugs in Decimal32 surfaced during the post-publish
  investigation. The `KNOWN_ISSUES.md` entries are coverage / scope
  gaps, not correctness defects.

## [1.2.0] - 2026-05-10

### Changed (behaviour, not API)

- `min` / `max` now follow IEEE 754-2019 §9.6 `minimumNumber` /
  `maximumNumber` semantics — a *quiet* NaN is treated as a
  "missing value" and the non-NaN operand is returned. Both
  operands NaN → NaN; signaling NaN still poisons with INVALID.
  This matches `Decimal128::min` / `Decimal128::max`, the General
  Decimal Arithmetic specification, and the IBM decTest
  conformance suite. Previously the siblings implemented §5.3.1
  `minimum` / `maximum`, which propagates qNaN. Both behaviours
  are 754-2019 conforming under different op names; the
  cross-precision-consistent choice is `minimumNumber`. Surfaced
  by the 6-agent review.

## [1.1.1] - 2026-05-10

### Fixed

- `fma` no longer drops `c` when one product factor is zero and the
  other has a far exponent. The previous alignment-shift early-
  return assumed `shift > MAX_SHIFT` implies the shifted side
  dominates, but `0 × anything = 0` so the *other* side is the
  answer. `fma(1e30, 0, 1)` now returns 1, not 0; `fma(1, 1,
  0E+30)` returns 1, not 0.
- `fma`'s alignment dispatch now uses a *dynamic* shift bound
  (`digit_count(operand) + shift ≤ 38`) instead of the previous
  static `MAX_SHIFT = 24`. The static bound mis-classified small
  products with comparable `c`: `fma(1, 1, 0.999999)` returned 1
  instead of 1.999999. The dynamic bound admits the case through
  the normal align-and-sum path whenever `u128` headroom permits.

## [1.1.0] - 2026-05-10

### Breaking

- `Decimal32::next_up` and `Decimal32::next_down` now return
  `(Decimal32, Status)` instead of `Decimal32`. Required to
  honour IEEE 754-2019 §5.3.1 for signaling-NaN inputs: a
  signaling NaN is quieted *and* raises `INVALID`. The previous
  `-> Self` signature couldn't carry the flag. Migration: where
  you wrote `let x = d.next_up();`, write `let (x, _) =
  d.next_up();` (or destructure the status if you care).
  Surfaced by the 6-agent review.

### Fixed

- `next_up` / `next_down` now correctly return the
  *numerically adjacent* representable value, not the next value
  in the *stored* cohort. The previous implementation
  incremented the coefficient at the input's stored exponent, so
  `next_up(5)` returned `6` instead of the actual ULP `5
  + 10⁻⁶ = 5.000001`. The fix renormalises to the lowest
  representable cohort (max coefficient digits, bounded by
  biased_exp = 0) before stepping. Same algorithmic shape as
  Decimal128's mature implementation.
- `next_up` of a signaling NaN now correctly quiets the NaN
  *and* raises INVALID (was: silently passed sNaN through). See
  the Breaking entry above.

## [1.0.4] - 2026-05-10

### Fixed

- `pow(self, exponent)` now correctly short-circuits the
  `pow(1, y) = 1` rule for every cohort of the value 1, not just
  the canonical `1 × 10⁰` bit pattern. `10 × 10⁻¹`, `100 × 10⁻²`,
  …, `10⁶ × 10⁻⁶` all now route through the short-circuit. Per
  IEEE 754-2019 §9.2 the rule is value-bound, not cohort-bound.
  Surfaced by the 6-agent review.

## [1.0.3] - 2026-05-10

### Fixed

- `finalise_finite` now short-circuits a zero coefficient before
  the OVERFLOW check. The up-front zero fast path at
  `round_and_pack_finite`'s entry only fires when `pre_sticky =
  false`; an extreme alignment cancellation that produced a zero
  coefficient with `pre_sticky = true` and an out-of-range biased
  exponent used to fall through to the overflow path and round to
  ±∞ + OVERFLOW. Now: zero with any quantum clamps the encoded
  biased exponent into the encodable range and returns canonical
  zero. Surfaced by the 6-agent review.

## [1.0.2] - 2026-05-10

### Fixed

- IEEE 754-2019 §6.3 exponent clamping (the "Clamped" condition).
  Values whose adjusted exponent is in range but whose biased
  exponent exceeds `BIASED_EXP_MAX = 191` are now padded with
  trailing zeros into the canonical encoding rather than rounded
  to ±∞ + OVERFLOW. The smallest demonstrator: `1E+96` now packs
  as `1000000E+90` (coef = 10^6, biased_exp = 191) instead of
  rounding to ±∞. Decimal64 has had this fix since 0.1.0; the
  surface review surfaced its absence on Decimal32. The previous
  unit test that baked in the wrong overflow behaviour at this
  boundary has been split: `round_clamp_at_emax_nearest` now
  asserts the §6.3 clamp; `round_overflow_to_infinity_nearest`
  uses `1E+97` (genuine overflow at adjusted = 97 > E_MAX = 96).

## [1.0.1] - 2026-05-10

### Changed

- `Status`, `RoundingMode`, and `IeeeClass` now re-export from the
  new [`ferrodec-ieee`](https://crates.io/crates/ferrodec-ieee)
  crate (v0.1.0). The types are byte-compatible with the previous
  release — `ferrodec_decimal32::Status` and `ferrodec::Status`
  resolve to the *same* concrete type, so cross-precision interop
  works without conversion. ADR-0012 records the rationale.

### Fixed

- `parse_str` now correctly handles inputs with leading zeros after
  the decimal point that exceed `MAX_PARSED_DIGITS = 16`. Previously
  the leading fractional zeros (e.g. the four `0`s in
  `0.00001234567890123456`) "spent" digit-budget slots and pushed
  the last significant digit into the sticky bit, dropping the
  result by one significant figure. The fix adds a
  `leading_frac_zero` branch that increments only
  `digits_after_point` (shifting the quantum) without incrementing
  `digits_total`. Discovered via the same algorithmic shape on
  Decimal64 where `MAX_PARSED_DIGITS = 19` and the boundary is more
  often reached.

## [1.0.0] - 2026-05-10

Initial release. IEEE 754-2019 Decimal32 in pure Rust, `no_std`-
capable, with the verification posture established by ferrodec/
Decimal128.

### Added

- Skeleton crate. `Decimal32(u32)` type wrapper, no methods yet.
  Initial groundwork for the full Decimal32 implementation per the
  plan archived at
  `docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`.
  Inherits workspace lints, edition, MSRV (1.84), license, and
  repository metadata. `fmt` and `kani` features are declared with
  empty bodies for future use.
- Shared IEEE 754 metadata types: `Status`, `RoundingMode`, `IeeeClass`.
  `Status` and `RoundingMode` are duplicated verbatim from ferrodec
  (the file is fully precision-agnostic). `IeeeClass` is adapted from
  ferrodec's `classify.rs`: same enum shape with doc text retargeted
  from Decimal128 to Decimal32. Extraction to a shared
  `ferrodec-ieee` crate is deferred until three concrete consumers
  exist; each file declares the deferral at the top.
- BID encoding foundation: parameters, decoder, encoder, helpers per
  IEEE 754-2019 §3.5.2 for decimal32. The decoder and encoder handle
  Form A (coefficient < 2²³) and Form B (coefficient ∈ [2²³, 10⁷))
  symmetrically; non-canonical Form B encodings (coefficient ≥ 10⁷)
  decode to ±0 with the encoded sign and biased exponent, matching
  ferrodec's BID-128 canonicalisation discipline. 16 unit tests
  cover round-trip pack/unpack across a sweep of (sign, biased_exp,
  coefficient) triples spanning both forms and the canonical
  boundary, plus Intel-reference bit patterns for Inf and NaN. The
  module-level `#![allow(dead_code)]` is transient: the BID items
  become consumed when classify, parse, format, and arithmetic
  modules land in subsequent commits.
- Vendored IBM decTest conformance vectors at
  `tests/vectors/dsBase.decTest` (909 cases, parse/format/rounding)
  and `tests/vectors/dsEncode.decTest` (268 cases, BID and DPD
  bit-pattern encoding). These are the only `ds*` files in the IBM
  decTest distribution; arithmetic surface coverage will lean on
  property tests against the astro-float oracle in subsequent
  commits, with the rationale documented in
  `tests/vectors/README.md`. The conformance harness consuming these
  vectors lands in B5.
- Conformance harness skeleton at `tests/conformance.rs` (gated on
  the `fmt` feature). Parses `.decTest` files into structured cases
  with directive-aware context (precision, max/min exponent,
  rounding); dispatches every case to a stub that returns `Skip`
  pending implementation of the operations. The harness already
  loads all 1175 cases from `dsBase` and `dsEncode` and reports
  per-file pass / fail / skip counts. The asymmetric per-file
  expectation guard (per ADR-0010) starts at 0 passes for both
  files; each subsequent commit that wires a dispatch arm raises
  the corresponding row by the cases it now passes. CI runs the
  harness in the `decimal32` job under `--features=fmt`.
- `Decimal32` struct moved from `lib.rs` into `decimal.rs` alongside
  IEEE 754 distinguished constants (`ZERO`, `NEG_ZERO`, `ONE`,
  `NEG_ONE`, `TEN`, `MAX`, `MIN`, `MIN_POSITIVE`,
  `MIN_POSITIVE_NORMAL`, `INFINITY`, `NEG_INFINITY`, `NAN`,
  `SIGNALING_NAN`), `from_bits` / `to_bits` (raw u32 round-trip),
  `try_new` and `try_new_unsigned` constructors (return
  `Decimal32BuildError` on coefficient or exponent out-of-range),
  and a `Debug` impl that surfaces the bit pattern and decoded
  class.
- Examples at `examples/`: `money.rs` (small-ledger telemetry with
  tax-rate multiplication and cent-quantized totals; demonstrates
  Decimal32's 7-digit headroom suits per-transaction reporting),
  `rounding_modes.rs` (a side-by-side table showing every IEEE 754
  rounding mode applied to halfway values like 1.005 / 1.015 /
  -1.005 / -1.015), `transcendentals.rs` (exp, ln, sin, cos, sqrt
  plus an INVALID-flag demo via `acos(2)`). All three run via
  `cargo run --example NAME --features {fmt,transcendentals}`.
- New `num-traits` feature: implements `Zero`, `One`, `Bounded`,
  `Signed`, `Num`, and the `From|To|Primitive` traits on Decimal32.
  Auto-enables `ops` (because `Num` requires
  `Add + Sub + Mul + Div + Rem`), `binary-float` (because the
  integer-conversion paths route through f64), and `fmt` (because
  `Num::from_str_radix` routes through `parse_str`). 7 unit tests
  cover the trait shapes plus a banker's-rounding `signum`,
  `is_positive` / `is_negative` semantics on NaN, the
  positive-difference `abs_sub`, and `to_i64` / `to_u64` /
  `to_f64` for representative values (rejecting NaN / Infinity /
  out-of-range).
- New `serde` feature: `Serialize` / `Deserialize` impls for
  `Decimal32`. Default serialisation routes through the canonical
  decimal string (`Display` for serialise, `parse_str` for
  deserialise) so values stay human-readable in JSON / TOML / YAML
  and survive every format. The `serde_bid` helper module (used via
  `#[serde(with = "ferrodec_decimal32::serde_bid")]`) serialises the
  raw 32-bit BID pattern; binary formats get a 4-byte representation
  while text formats fall back to the string parser. Pulls in `fmt`
  and `dep:serde`. 4 integration tests cover JSON round-trip across
  finite, infinity, and NaN; the `serde_bid` round-trip; and the
  string-fallback path.
- New `ops` feature: `core::ops` overloads on `Decimal32` —
  `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg`, plus the `*Assign`
  variants. Default to `RoundingMode::NearestEven` and discard the
  per-operation `Status`. Users who need explicit rounding-mode or
  status control keep using the `add` / `sub` / `mul` / `div` /
  `rem` methods. The trade-off mirrors ferrodec/Decimal128: the
  ergonomic operators are an opt-in for callers migrating from
  `f64` or `rust_decimal`. 3 unit tests cover the basic `+ - * / %`
  shapes, `Neg`, and the `*=` family.
- Fuzz harnesses at `ferrodec-decimal32/fuzz/`. Four cargo-fuzz
  targets (mirroring ferrodec's pattern, scoped to operations the
  Decimal32 surface exposes): `parse` (asserts no panic on
  arbitrary byte input plus `Display` round-trip equality on
  successful parses), `arith` (no panic on arbitrary `(u32, u32)`
  bit-pattern pairs through add / sub / mul / div / rem; checks
  `a + 0 == a` and `a - a == 0` algebraic identities for non-NaN
  finite `a`), `transcendentals` (no panic on arbitrary `u32`
  through every kernel: exp, ln, sin, cos, tan, asin, acos, atan,
  sinh, cosh, tanh, asinh, acosh, atanh, cbrt, sqrt), and
  `total_cmp` (totalOrder anti-symmetry, reflexivity, and a
  transitivity surrogate). The `fuzz/` directory is a separate
  cargo-fuzz crate (not a workspace member) so the standard
  workspace build is unaffected.
- Trig: `Decimal32::sin`, `cos`, `tan`, `asin`, `acos`, `atan`,
  `atan2` under the `trig` feature. Same f64-via-libm pattern as
  exp/ln. Special cases per IEEE 754-2019 §9.2: `sin/cos/tan(±∞)`
  → NaN+INVALID; `asin / acos` outside `[-1, +1]` → NaN+INVALID;
  `atan(±∞) = ±π/2`. 12 unit tests cover the special cases plus
  representative values (sin(π/2) ≈ 1, cos(π) ≈ -1, asin(1) = π/2,
  atan(1) = π/4, atan2(1,1) = π/4).
- Hyperbolic: `Decimal32::sinh`, `cosh`, `tanh`, `asinh`, `acosh`,
  `atanh` under the `hyperbolic` feature (which auto-pulls
  `exp-log`). Special cases: `tanh(±∞) = ±1`, `acosh` domain `[1, ∞)`
  enforced (returns NaN+INVALID below 1), `atanh(±1) = ±∞ +
  DIV_BY_ZERO`, `atanh(|x| > 1)` NaN+INVALID. 8 unit tests cover
  the boundaries.
- `Decimal32::pow` and `Decimal32::cbrt` under the `pow` feature
  (auto-pulls `exp-log`). `pow` special-cases the IEEE 754-2019 §9.2
  rules: `pow(x, 0) = 1` (including `pow(NaN, 0)`), `pow(1, y) = 1`
  (including `pow(1, NaN)`), `pow(0, negative)` → ±∞ + DIV_BY_ZERO,
  negative-base with non-integer exponent → NaN + INVALID, NaN
  propagation. `cbrt` defined for all reals (preserves sign on ±0
  and ±∞). 7 unit tests.
- `Decimal32::exp` and `Decimal32::ln` per IEEE 754-2019 §9.2 under
  the new `exp-log` feature. Both route through `f64` via `libm`
  (pure Rust, `no_std`-compatible, no FFI). The double-rounding
  error is below 1 ULP at Decimal32's 7-digit precision because
  `f64` carries ~15.95 digits, far above what we need. Special
  cases handled directly: `exp(±0) = 1`, `exp(+∞) = +∞`,
  `exp(−∞) = +0`, `ln(±0) = −∞ + DIV_BY_ZERO`,
  `ln(negative) = NaN + INVALID`, NaN propagation; overflow /
  underflow in the f64 result raises `OVERFLOW + INEXACT` /
  `UNDERFLOW + INEXACT` explicitly. `INEXACT` is set on every
  non-zero finite output (the irrationality of typical
  transcendental results means an exact 7-digit Decimal32 match
  is essentially never coincidental). 11 unit tests cover
  `exp(0) = 1`, `exp(1) ≈ e`, `exp(-1) ≈ 1/e`, overflow,
  underflow, infinity / NaN, `ln(1) = 0`, `ln(e) ≈ 1`,
  `ln(10) ≈ ln10`, the `ln(±0)` and `ln(negative)` errors, and
  exp/ln round-trip on integer inputs.
- New optional dependency: `libm = "0.2"` (default-features = false,
  optional). Pulled in by the transcendental features (`exp-log`,
  `trig`, `hyperbolic`, `pow`).
- New `binary-float` feature for `Decimal32 ↔ f64` conversion
  (`Decimal32::to_f64`, `Decimal32::from_f64(f64, RoundingMode)`).
  No allocation: `from_f64` formats via `core::fmt::Write` into a
  32-byte stack buffer, then routes through the existing
  `parse_str`. Auto-enabled by every transcendental feature.
- New `transcendentals` meta-feature (mirrors ferrodec): enables
  every transcendental cluster at once.
- Bug fix in `parse_str`: fractional digits beyond
  `MAX_PARSED_DIGITS` no longer increment `digits_after_point`.
  Previously they did, which placed the coefficient at the wrong
  quantum (off by the count of trailing sticky digits) for inputs
  with more than 16 significant fractional digits. The bug never
  surfaced before because Decimal32 conformance vectors don't
  exercise that boundary directly; it manifested when the new
  `from_f64` path emitted 17-digit scientific-notation strings
  for parsing.
- Kani verification harnesses at `src/verify/` (compiled only under
  `cfg(kani)`). Six modules cover the major arithmetic surface
  (addsub, mul, div, sqrt, fma, cmp). Each harness binds operands to
  a 10-constant set (NaN, sNaN, ±∞, ±0, ±1, ±MAX, ±MIN_POSITIVE) so
  the SAT problem stays tractable, then asserts no-panic plus
  IEEE 754 properties: NaN propagation, sNaN raises INVALID,
  `(±∞) + (±∞)` opposite-sign → NaN+INVALID, `(+0) + (−0)` zero-sign
  rule per §6.3, `0 × ±∞` → NaN+INVALID, finite/0 raises
  DIV_BY_ZERO with XOR sign, `sqrt(−x)` and `sqrt(−∞)` → NaN+INVALID,
  `partial_cmp` returns `None` for any NaN, `total_cmp` is total
  (never panics, reflexive on equal operand selectors). Strategy
  mirrors ferrodec's verify/ tree: prove the special-case lattice
  symbolically; defer finite-finite arithmetic correctness to
  property tests. CI's `kani` job extended with
  `cargo kani --package ferrodec-decimal32 --features=fmt`.
- Quantum-manipulating operations: `Decimal32::quantize`,
  `scaleb`, `logb`, `next_up`, `next_down`. `quantize(target, rm)`
  rescales `self` to `target`'s quantum (rounding when reducing the
  exponent, padding when increasing); raises `INVALID` when the
  rescaled coefficient would exceed PRECISION digits or when an
  operand has incompatible specialness (e.g. `quantize(finite,
  ±∞)`). `scaleb(n, rm)` returns `self * 10^n` via the standard
  `round_and_pack_finite` path; routes overflow to ±∞ and underflow
  to ±0 via the rounding mode. `logb()` returns the floor of
  log₁₀(|self|) — equivalently, the adjusted exponent — at quantum
  0; `logb(±0) = −∞ + DIV_BY_ZERO`, `logb(±∞) = +∞`. `next_up`
  navigates one ULP toward +∞ (with the cohort step from
  `pack_finite(false, biased+1, 10⁶)` when the coefficient carries),
  and `next_down(self) = -next_up(-self)`. 13 unit tests cover
  pad-with-zeros quantize, round-to-target quantum, the overflow-
  to-INVALID path, infinity passthrough, scaleb basics and
  overflow / underflow, logb basics and specials, next_up at zero
  / finite / ±∞, and next_up NaN propagation.
- Comparison and ordering: `Decimal32::partial_cmp`,
  `Decimal32::total_cmp`, `Decimal32::compare_total_magnitude`,
  `Decimal32::min`, `Decimal32::max`. `partial_cmp` returns
  `(Option<Ordering>, Status)` per IEEE 754-2019 §5.6.1: `None` on
  any NaN with `INVALID` raised for sNaN, `Some(Ordering)` otherwise
  with cohort-equal values comparing equal numerically (so `+0 = -0`
  and `1.0 = 1.00`). `total_cmp` returns `Ordering` directly per
  §5.10 totalOrder: negative qNaN < negative sNaN < negative ∞ <
  negative finite < `−0` < `+0` < positive finite < positive ∞ <
  positive sNaN < positive qNaN, with NaN payloads breaking ties
  ascending in the positive band and descending in the negative
  band, and same-numeric-value cohorts ordered by biased exponent
  (ascending for positive, descending for negative).
  `compare_total_magnitude` is `total_cmp` applied to absolute
  values. `min` / `max` per §5.3.1: NaN propagates (sNaN raises
  INVALID and quietens; qNaN passes through), `min(+0, −0) = −0`,
  `max(+0, −0) = +0`. 13 unit tests cover basic ordering, sign
  comparison, zero-cohort equality, finite-cohort equality,
  infinities, NaN comparison and INVALID emission, total-order
  rank checks (including negative qNaN at the bottom and `−0 < +0`),
  cohort ordering for equal numerics, min/max basics, min/max zero
  signs, min/max NaN behaviour, and totalOrderMag sign
  independence.
- `Decimal32::fma(self, b, c, rm)` per IEEE 754-2019 §5.4.1 fused
  multiply-add. Computes `self * b + c` with a single rounding step;
  the intermediate product is preserved exactly before the addition.
  Returns `(Decimal32, Status)`. The finite path forms the exact
  product (`u32 × u32 → u64`, max ~10¹⁴), aligns with `c` over a
  `u128` working width (max value ~10³⁸), sign-aware combines, then
  compresses back to `u64` with sticky tracking before routing through
  `round_and_pack_finite`. Special cases: sNaN in any operand → quiet
  NaN + INVALID; qNaN propagation in argument order; `0 × ±∞` and
  `±∞ × 0` → NaN + INVALID (regardless of `c`); `(±∞) + (∓∞)` from
  product + addend → NaN + INVALID; `(±∞) + finite` → ±∞ XOR;
  `finite × finite + ±∞` → ±∞ (sign of `c`). 11 unit tests cover
  basic FMA, the single-rounding advantage (1234567² + (-1.524156×10¹²)
  yields exact -322511 even though the rounded product alone would
  lose precision), alignment, zero addend / multiplicand, sign
  combinations, the 0 × ∞ INVALID, ∞-∞ INVALID, NaN propagation
  (including sNaN in c), and zero-sum sign rule.
- `Decimal32::sqrt(self, rm)` per IEEE 754-2019 §5.4.1. Returns
  `(Decimal32, Status)`. The finite path makes the working exponent
  even (multiplying coefficient by 10 if exp was odd), scales further
  so the working coefficient has 15 or 16 decimal digits — `isqrt`
  then lands in `[10⁷, 10⁸)` (8 digits, one above PRECISION for
  correct rounding) — and routes through `round_and_pack_finite`
  with `sticky = (isqrt² != working_coef)`. Special cases: sNaN
  → quiet NaN + INVALID; qNaN propagation; `sqrt(±0) = ±0` (sign
  preserved per IEEE 754); `sqrt(+∞) = +∞`; `sqrt(−∞)` and
  `sqrt(−finite)` → NaN + INVALID. Preferred quantum is
  `floor(Q(x) / 2)` per §6.3 (using `i32::div_euclid` to floor
  correctly for negative exponents). 10 unit tests cover perfect
  squares (4, 9, 100, 10000, 1234²), inexact (sqrt(2) → 1.414214
  Inexact), zero (with sign), one, negative-input INVALID,
  infinities, NaN propagation, negative exponent (sqrt(0.04) =
  0.2), large exponent (sqrt(10⁹⁶) = 10⁴⁸), and 7-digit perfect
  square.
- `Decimal32::rem(self, other, rm)` per IEEE 754-2019 §5.3.1 (truncated
  remainder, sign of dividend). Returns `(Decimal32, Status)`. Result
  has the sign of `self`, magnitude strictly less than `|other|`, and
  quantum `min(Q(self), Q(other))`. Operation is exact when defined;
  the `rm` parameter is carried for API parity. Special cases:
  `±∞ % anything` and `anything % 0` → NaN + INVALID; `finite % ±∞`
  → finite (the dividend). Per the GDA spec, the integer quotient
  must fit in `PRECISION` (= 7) digits; otherwise NaN + INVALID.
  When the exponent gap exceeds `MAX_SAFE_SHIFT` (= 12) one of two
  shortcuts fires: `|a| ≫ |b|` returns NaN + INVALID, `|b| ≫ |a|`
  returns the dividend at `Q(a)`. 8 unit tests cover basic remainder,
  sign of dividend, quantum-min cohort selection, zero dividend,
  divide-by-zero, infinity cases, the too-large-quotient path
  (MAX % MIN_POSITIVE), and NaN propagation.
- `Decimal32::div(self, other, rm)` per IEEE 754-2019 §6.3 / §7.
  Returns `(Decimal32, Status)`. The finite path scales the dividend
  by `10^(db - da + PRECISION + 1)` so the integer quotient holds
  ≥ 8 digits, then routes through `round_and_pack_finite` with the
  post-scale remainder feeding the rounding sticky bit. `q_preferred
  = exp_a - exp_b`. Special cases: `0 / 0` and `±∞ / ±∞` → NaN +
  INVALID; `finite / 0` (finite ≠ 0) → ±∞ + DIV_BY_ZERO with XOR
  sign; `±∞ / finite` → ±∞ XOR; `finite / ±∞` → ±0 XOR; sNaN /
  qNaN propagation. 10 unit tests cover exact and inexact division
  (1/3 → 0.3333333 Inexact), sign combinations, divide-by-zero,
  zero-divided-by-zero, zero divided by finite, infinities,
  overflow, underflow, and NaN propagation.
- `Decimal32::mul(self, other, rm)` per IEEE 754-2019 §6.3 / §7.
  Returns `(Decimal32, Status)`. The finite path multiplies u32 × u32
  → u64 directly (no multiword machinery; the product max fits in
  ~47 bits) and adds the unbiased exponents to produce the preferred
  quantum, then routes through `round_and_pack_finite`. The
  special-case dispatcher handles sNaN propagation, qNaN propagation
  (a preferred per §6.2.3), `0 × ±∞` → NaN + INVALID, and the XOR
  sign rule for ±∞ × ±finite and ±∞ × ±∞. 11 unit tests cover
  basics, sign combinations, quantum addition, full-precision
  products, inexact rounding (1234567² → 1524156 × 10⁶ Inexact),
  zero and overflow, underflow to zero, NaN propagation, and the
  invalid 0 × ∞ case.
- `Decimal32::add(self, other, rm)` and `Decimal32::sub(self, other, rm)`
  per IEEE 754-2019 §6.3 (sign rules) and §7 (exception flags). Both
  return `(Decimal32, Status)`. Subtract is implemented as
  `add(a, neg(b))` after the special-case dispatcher quietens any
  signaling NaN. The finite path aligns coefficients over a `u64`
  working width with three regimes: shifts up to ALIGN_LIMIT = 12
  preserve full precision; shifts in (12, WORKING_PRECISION = 14]
  truncate the lower operand with sticky tracking; shifts beyond 14
  leave the lower operand entirely below the working window and
  feed its non-zeroness into sticky. Sign-aware combine handles
  cancellation cases (including the IEEE 754 §6.3 rule that
  `x + (−x)` and `(±0) + (∓0)` produce `+0` in all rounding modes
  except `roundTowardNegative`, which yields `−0`). 11 unit tests
  cover basic add, carry-renormalisation, alignment-induced
  inexactness, sign-disagreement cancellation, zero combinations,
  NaN propagation (including signaling-NaN INVALID emission),
  Infinity arithmetic (including `+∞ + (−∞) → NaN, INVALID`),
  overflow to ∞, and basic subtract.
- Conformance harness `toSci` dispatch arm wired up. The dispatch
  parses the operand string, formats the result via `Display`, and
  compares both the rendered output and the emitted IEEE 754 status
  flags against the decTest expected output and `Conversion_syntax`
  / `Inexact` / `Underflow` / `Overflow` conditions. Parse errors
  (other than `ExponentOutOfRange`, which is deferred) are translated
  into `(NaN, Status::INVALID)` to match decTest's negative-test
  shape. Per-file expectation table now records 698 passes for
  `dsBase.decTest` (of 909 cases; 209 skip under non-IEEE rounding
  directives or extreme exponents) and 2 passes for `dsEncode.decTest`
  (of 268 cases; the rest defer to the `dpd` feature in B16). 700
  conformance cases pass with 0 failures.
- Rounding kernel at `src/ops/round.rs`. The
  `round_and_pack_finite(coef: u64, unbiased_exp, q_preferred, sign,
  pre_sticky, rm, status)` entry point handles digit drop with
  guard / sticky tracking, applies the five IEEE 754 rounding modes,
  renormalises across power-of-10 boundaries, shifts toward the
  preferred quantum (pad on inexact, strip trailing zeros on exact),
  and emits `INEXACT` / `OVERFLOW` / `UNDERFLOW` flags. Both
  `parse_str` and the arithmetic ops (B7+) route through this single
  function. 9 unit tests cover the rounding axes (no rounding
  required, halfway-to-even, carry-renormalises, overflow to
  infinity vs. MAX, underflow to zero, zero-quantum preservation).
- `parse_str(&str, RoundingMode) -> Result<(Decimal32, Status),
  ParseDecimalError>` under the `fmt` feature. Accepts signed
  decimals, scientific notation, NaN / sNaN with optional payloads
  (≤ 20-bit field), and Infinity. Up to 16 mantissa digits are
  accumulated exactly in a `u64`; trailing digits feed the rounding
  sticky bit. `FromStr` defers to `parse_str` with `NearestEven`.
  11 unit tests cover zero, integers, fixed decimals, scientific,
  specials, NaN payloads (including overflow), leading zeros,
  rounding at the precision boundary, and invalid inputs.
- `Display`, `LowerExp`, `UpperExp`, and `Engineering` adapters under
  the `fmt` feature. `Display` follows the General Decimal Arithmetic
  toSci convention: plain decimal notation when the unbiased exponent
  is ≤ 0 and the adjusted exponent is ≥ -6, otherwise scientific
  with `E±N`. `LowerExp` / `UpperExp` force scientific with the
  matching letter. `Engineering` (returned by `Decimal32::engineering`)
  forces the exponent to a multiple of 3, mantissa in `[1, 1000)`.
  All four routes use a fixed 8-byte stack scratch buffer; no `alloc`
  or heap allocation. 12 unit tests cover zero with sign, distinguished
  constants, plain vs. scientific dispatch, negative finite values,
  and the engineering rebase. `{:.N}` precision support is deferred
  until `quantize` lands with the arithmetic ops.
- Classification predicates and operations: `is_nan`,
  `is_signaling_nan`, `is_quiet_nan`, `is_infinite`, `is_finite`,
  `is_zero`, `is_normal`, `is_subnormal`, `is_sign_negative`,
  `is_sign_positive`, `classify` (returning `core::num::FpCategory`),
  `ieee_class` (returning `IeeeClass`), `abs` and `neg` (no status),
  `abs_with_status` and `neg_with_status` (raise `Status::INVALID`
  on signaling-NaN input, otherwise quiet), `copysign`,
  `is_canonical` (handles BID-32's Form A and Form B canonicalisation
  symmetrically — Form A is always canonical, Form B is canonical
  iff the decoded coefficient is `< 10^7`), and `canonicalize`
  (rewrites non-canonical inputs to the equivalent canonical
  encoding). 16 unit tests cover all predicates and operations
  against the distinguished constants and dirty bit patterns.
