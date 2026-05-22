# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.2.0] - 2026-05-21

Sibling-only release restoring conversion API parity with the
`ferrodec` (Decimal128) parent. Decimal128 had a fuller surface of
from-integer constructors and `TryFrom<f64>` / `TryFrom<f32>`
impls since 1.x; the sibling crates kept only the to-integer
direction plus a bare `from_f64` method. This release closes the
gap. The parent crate stays at 2.1.0; the change is sibling-only
and additive, hence SemVer minor.

### Added

- **From-integer constructors (fd-o98).** `Decimal64::from_i32` /
  `from_u32` (const, exact, infallible — `i32` / `u32` fit
  Decimal64's 16-digit precision losslessly) plus
  `Decimal64::from_i64` / `from_u64` / `from_i128` / `from_u128`,
  all four taking a `RoundingMode` and returning `(Decimal64,
  Status)` because the integer range exceeds 16 digits. The
  helper packs a `u128` absolute value down to the kernel's `u64`
  coefficient width with sticky tracking, then delegates the
  precision-boundary round to the shared `round_and_pack_finite`.
- **`impl From<i32>` / `impl From<u32>` for `Decimal64`.** Lossless
  conversions get the trait impl. Larger integer types do not (a
  lossy `From` impl would misrepresent the API).
- **`impl TryFrom<f64>` / `impl TryFrom<f32>` for `Decimal64`
  (fd-o98).** Behind the existing `binary-float` feature gate. NaN
  rejects with `Decimal64FromFloatError::NotANumber`, ±∞ with
  `Infinite`; finite values flow through
  `Decimal64::from_f64(value, RoundingMode::NearestEven)`. The
  error enum mirrors the parent's `Decimal128FromFloatError`
  shape with the format name folded into the variant message.
- **`Decimal64FromFloatError`.** New public error type matching
  the Decimal128 parent's `Decimal128FromFloatError` shape, with
  the per-format identifier in `Display` ("cannot convert NaN to
  Decimal64", "cannot convert ±∞ to Decimal64"). Behind
  `binary-float`.

### Changed

- **`src/lib.rs` now `include_str`s the crate `README.md`** into
  the rustdoc, matching the parent's pattern. The docs.rs
  Decimal64 page renders the full README narrative (feature flag
  table, target matrix, comparison table) after the lib.rs
  preamble. Pre-fix, docs.rs visitors saw only the preamble.
- **README backtick polish for `x86_64`, `total_cmp`, `no_std`**
  in three lines that were prose-rendered before the
  include_str. No content change.

The shared infrastructure crates (`ferrodec-ieee`,
`ferrodec-multiword`, `ferrodec-transcend`, `ferrodec-test-support`)
stay at their current versions.

## [2.1.0] - 2026-05-21

The §9.2 transcendental contract tightens from faithful (≤ 1 ULP at
16 digits, ADR-0024) to correctly rounded (the single nearest
representable `Decimal64` value at every IEEE 754-2019 rounding
direction, ADR-0032). The change ships lockstep with `ferrodec`
2.1.0 and `ferrodec-decimal32` 2.1.0; the shared
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
    match per vector; 2413 `Decimal64` vectors across five rounding
    modes including binary `pow` and `atan2` all land on the
    correctly rounded value. MPFR confirms.
  - Mechanism: Lefèvre / Muller wider fixed working precision with
    rigorous a priori error bounds; the 50 digit kernel clears the
    smallest empirical Arb worst case half ULP margin by more than
    thirty orders of magnitude at `Decimal64` precision. The per
    function derivations live in the shared `ferrodec-transcend`
    module rustdoc.

The shared infrastructure crates (`ferrodec-ieee`,
`ferrodec-multiword`, `ferrodec-transcend`, `ferrodec-test-support`)
stay at their current versions per ADR-0029's intent.

## [2.0.0] - 2026-05-21

The first major-version release. Two consolidated breaking changes
that ADR-0029 froze into the 2.0 set, shipped together with the
parent `ferrodec` 2.0.0 and the `ferrodec-decimal32` 2.0.0. The
shared-infrastructure crates (`ferrodec-ieee`, `ferrodec-multiword`,
`ferrodec-transcend`, `ferrodec-test-support`) are unchanged.

### Breaking

- **Retired the ambiguous bare `rem` method (fd-9n5, ADR-0027 (b),
  ADR-0029 item 1).** The 1.x bare `Decimal64::rem(self, other,
  _rm: RoundingMode)` is the GDA / C99 `fmod` truncated remainder
  (`r = a − trunc(a / b) × b`); the parent crate's bare `Decimal128::rem`
  was the *nearest-even* remainder, so the same name carried different
  math depending on the type. 2.0 retires the ambiguous spelling.
  - `Decimal64::rem` is renamed in place to `Decimal64::rem_trunc`.
    The (always unused) trailing `RoundingMode` argument is dropped
    in the rename; the new signature is
    `pub fn rem_trunc(self, other: Self) -> (Self, Status)`.
  - `Decimal64::rem_near` (the IEEE 754-2019 §5.3.1 nearest-even op
    that shipped in 1.6.0) is unchanged.
  - The `core::ops::Rem` (`%`) and `core::ops::RemAssign` impls are
    retained (so `impl num_traits::Num` continues to hold under the
    `num-traits` feature) but now route through `Decimal64::rem_trunc`
    explicitly; the dispatch destination is the rule `%` already had
    on this format in 1.x.
  - **Migration.** Rewrite `value.rem(other, rm)` call sites as
    `value.rem_trunc(other)`. Code that uses `%` does not change.

- **Widened `ParseDecimalError` (fd-7f1, ADR-0018 L14, ADR-0029
  item 2).** The 1.x enum collapsed every parse failure into four
  coarse variants. 2.0 widens the enum so callers can match on each
  failure mode.
  - New shape (byte-identical with the parent and the decimal32
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

- `Decimal64::fixed_preferred()` returns a `FixedPreferred(Decimal64)`
  adapter whose `Display` impl applies the 1.x parent
  `Decimal128::Display` rule (plain notation preferred when the
  scale fits `(-6, 21]`, otherwise scientific). Additive — the
  default `Decimal64::Display` continues to follow GDA `toSci`, the
  rule it always used — exposed so callers can opt into the legacy
  parent rendering uniformly across the family alongside the
  parent's new `Decimal128::fixed_preferred()`. ADR-0014, ADR-0029
  item 3.

### Notes

- The parent `ferrodec` 2.0.0 harmonized `Decimal128::Display` onto
  GDA `toSci`. The default `Decimal64::Display` is unchanged in
  this release (it was already `toSci`); the cross-format
  inconsistency in 1.x is resolved on the parent side.
- The shared infrastructure crates (`ferrodec-ieee` 0.1.4,
  `ferrodec-multiword` 0.1.0, `ferrodec-transcend` 0.1.0,
  `ferrodec-test-support` unpinned) are unchanged and stay at
  their pre-2.0 versions.
- The conformance suite re-ran with zero per-bucket count drift.
  The `rem` rename was source-side only (the dispatch already
  routed via `rem_trunc` / `rem_near` under the hood); the
  `ParseDecimalError` widening did not change which inputs the
  harness skips (`CoefficientOverflow` joins `ExponentOutOfRange`
  in the skip arm at `tests/conformance.rs:643-647, :712-723`).

## [1.8.0] - 2026-05-20

### Added

- `Decimal64::reduce`: General Decimal Arithmetic trailing-zero
  strip. Conformance-validated against `ddReduce` (133 of 134; the
  one skip is the `#`-hex BID-interchange case). ADR-0031.
- `Decimal64::divide_integer`: General Decimal Arithmetic truncated
  integer quotient at exponent 0. `Inf / 0` returns signed Infinity
  with no flag (infinity-arithmetic rule), while `finite_nonzero / 0`
  returns signed Infinity with `DIV_BY_ZERO`. `Division_impossible`
  (quotient exceeds 16 digits) raises `INVALID`. Conformance-
  validated against `ddDivideInt` (371 of 373). ADR-0031.
- `Decimal64::logical_invert`: digit-wise complement of a logical
  operand, padded to 16 digits. NaN of any kind raises `INVALID`
  (the logical-op uniform rule, unlike the rest of the GDA
  surface). Conformance-validated against `ddInvert` (151 of 151).
  ADR-0031.
- `Decimal64::logical_and` / `logical_or` / `logical_xor`:
  digit-wise truth-table ops over two logical operands.
  Conformance-validated against `ddAnd` (287 of 287), `ddOr` (237
  of 237), `ddXor` (278 of 278). ADR-0031.
- `Decimal64::shift`: coefficient-digit shift inside the
  precision-wide window with zero fill. Conformance-validated
  against `ddShift` (212 of 212). ADR-0031.
- `Decimal64::rotate`: coefficient-digit modular rotation inside
  the precision-wide window. Conformance-validated against
  `ddRotate` (212 of 212). ADR-0031.

### Fixed

- `Decimal64::fma`: subnormal results are now single-rounded,
  fixing the double-rounding family the standing fd-dpg
  exact-oracle sweep surfaced. The funnel rounded the exact product
  to PRECISION first and then re-rounded into the subnormal
  quantum; a residue straddling the two rounding boundaries was
  collapsed into an exact tie and carried the wrong way. The
  kernel now drops to the wider of the precision and
  subnormal-quantum requirements in one rounding, the IEEE
  754-2019 single-rounding contract, matching the parent
  `Decimal128`. ADR-0030.

## [1.7.0] - 2026-05-18

### Added

- `Decimal64::min_magnitude` / `Decimal64::max_magnitude`: the
  IEEE 754-2019 §9.6 `minimumMagnitudeNumber` /
  `maximumMagnitudeNumber` operations (smaller / larger numeric
  magnitude; an equal-magnitude tie defers to `min` / `max`; NaN
  handling as for `min` / `max`). Conformance-validated against the
  vendored decTest `ddMaxMag` (241 of 243) and `ddMinMag` (231 of
  233) vectors with zero failures. Additive and non-breaking;
  ADR-0028 records the design and the `…Number`-variant choice.

## [1.6.0] - 2026-05-18

### Added

- `Decimal64::rem_near`: the IEEE 754-2019 §5.3.1 nearest-even
  remainder (`r = a − n·b` with `n` the round-half-even integer
  quotient, `|r| ≤ |b|/2`, always exact), the sibling analogue of
  `Decimal128::rem`. The existing `Decimal64::rem` is the *truncated*
  remainder and is unchanged; the two names previously had no
  nearest-even counterpart on this format, a documented API hazard
  (ADR-0027). Validated bit-for-bit against the exact integer oracle
  over the full finite encoding domain and against the decTest
  `remaindernear` conformance vectors (now dispatched, 527 of 529, 0
  failures; `remainder` 503 of 505). Additive and non-breaking;
  ADR-0027 records the 2.0 plan to retire the ambiguous bare `rem`
  and `%` spellings.

## [1.5.0] - 2026-05-17

### Changed

- `Decimal64::exp` and `Decimal64::ln` are now faithfully rounded
  (≤ 1 ULP at 16 digits, every IEEE 754-2019 rounding direction) via
  the shared `ferrodec-transcend` Extended-precision kernel, replacing
  the lossy `f64` / `libm` detour that capped precision at ~10⁻¹⁵
  relative. The kernel is the same verified implementation the
  `ferrodec` (Decimal128) parent uses, instantiated at
  `F = Decimal64` through the `DecimalFormat` seam. The rewrite also
  reaches `Decimal64`'s true domain: `exp(800)` is now finite where
  the `f64` path saturated near `x ≈ 709`, and `ln` is faithful
  across the full positive range down to `~10⁻³⁹⁸` and up to
  `~10³⁸⁴`. This is behaviour-improving, not a bug fix: previously
  wrong-by-rounding results become correctly faithful, and previously
  saturated inputs now return their true finite value. Special-value
  semantics (NaN / infinity / zero / negative) and the ADR-0016 Kani
  shims are byte-identical to before. The faithful contract is proven
  by the new `tests/property_exp.rs` and `tests/property_ln.rs`
  suites. New dependencies on the `ferrodec-transcend` and
  `ferrodec-multiword` workspace crates (pulled by `exp-log`) change
  the published dependency graph.

- `Decimal64::sin`, `cos`, `tan`, `asin`, `acos`, `atan`, and
  `atan2` are now faithfully rounded (≤ 1 ULP at 16 digits, every
  IEEE 754-2019 rounding direction) via the shared
  `ferrodec-transcend` Extended-precision kernel, replacing the lossy
  `f64` / `libm` detour. The forward functions use the Payne-Hanek
  argument reduction, so the pre-fd-r0l `|x| < 2^53` accuracy
  limitation (the f64 round-trip lost the low digits before
  reduction began) is lifted: `sin` / `cos` / `tan` are now faithful
  across the full `Decimal64` magnitude range. Behaviour-improving,
  not a bug fix. The `asin` / `acos` `|x| > 1` domain INVALID is now
  decided at Extended precision rather than on a rounded f64 value;
  special-value semantics and the ADR-0016 Kani shims are
  byte-identical to before. The faithful contract is proven by the
  new `tests/property_sincos.rs`, `tests/property_sincos_large.rs`,
  and `tests/property_inverse_trig.rs` suites. The `trig` feature now
  pulls the `ferrodec-transcend` / `ferrodec-multiword` workspace
  crates (already in the graph via `exp-log`).

- `Decimal64::sinh`, `cosh`, `tanh`, `asinh`, `acosh`, and `atanh`
  are now faithfully rounded (≤ 1 ULP at 16 digits, every IEEE
  754-2019 rounding direction) via the shared `ferrodec-transcend`
  Extended-precision kernel (built on the already-faithful `exp` /
  `ln` primitives), replacing the lossy `f64` / `libm` detour, at
  exact parity with the Decimal128 parent. The pre-fd-r0l `sinh` /
  `cosh` f64-overflow cap at `|x| ≳ 710` is lifted: the kernel
  computes `eˣ` at Extended precision and saturates only at the
  format's own overflow boundary, so the hyperbolic family is
  faithful across the full `Decimal64` magnitude range.
  Behaviour-improving, not a bug fix. The `acosh` `x < 1` domain
  INVALID and `acosh(1) = 0`, and the `atanh` `|x| == 1` pole
  (`±∞ + DIV_BY_ZERO`) and `|x| > 1` domain INVALID, are now decided
  at Extended precision rather than on a rounded f64 value;
  special-value semantics and the ADR-0016 Kani shims are
  byte-identical to before. The faithful contract is proven by the
  new `tests/property_hyperbolic.rs` suite. The `hyperbolic` feature
  now forwards `ferrodec-transcend/hyperbolic` (the
  `ferrodec-transcend` / `ferrodec-multiword` crates were already in
  the graph via `exp-log`). `hyper.rs` was the last functional
  caller of the internal `f64`-routed `ops/f64_bridge.rs` adapter
  (trig left it in the P3 phase; `pow` used `libm::pow` directly), so
  the now-dead `f64_bridge` shim was removed in the same change.

- `Decimal64::pow` is now faithfully rounded (≤ 1 ULP at 16 digits,
  every IEEE 754-2019 rounding direction) via the shared
  `ferrodec-transcend` Extended-precision kernel, replacing the lossy
  `f64` / `libm::pow` detour that capped precision at ~10⁻¹⁵
  relative. `pow(x, y)` evaluates `exp(y · ln(|x|))` entirely at
  Extended precision (with the bit-exact integer-exponent fast path),
  at exact parity with the Decimal128 parent through the
  `DecimalFormat` seam. Behaviour-improving, not a bug fix: the
  negative-base / non-integer-exponent INVALID and the `pow(±0, y)` /
  `pow(±∞, y)` / `pow(x, ±∞)` rules are now decided at Extended
  precision rather than on a rounded f64 exponent. The `pow_special_-
  cases` short-circuit and the ADR-0016 Kani shim are byte-identical
  to before. The faithful contract is proven by the new
  `tests/property_pow.rs` and `tests/property_pow_specials.rs`
  suites. The `pow` feature now forwards `ferrodec-transcend/pow`
  (the workspace crates were already in the graph via `exp-log`).

- `libm` is no longer a dependency of this crate. It had been
  retained only for the pre-fd-r0l f64 transcendental detour; with
  `pow` now on the shared kernel, no `src/` code makes any
  functional `libm` call, so `dep:libm` was dropped from the
  `exp-log` / `trig` feature arrays and the `libm` dependency line
  removed. This shrinks the published dependency graph; the public
  surface is unchanged.

### Added

- `Decimal64::exp2`, `Decimal64::log2`, and `Decimal64::log10`,
  faithfully rounded (≤ 1 ULP at 16 digits, every IEEE 754-2019
  rounding direction) via the shared `ferrodec-transcend` kernel as
  pure delegations (the kernel resolves every special case, exactly
  as on the `ferrodec` Decimal128 parent). They ship under the
  existing `exp-log` feature. With these, `Decimal64::cbrt` now
  faithfully rounded via the same kernel (below), the exp-log family
  reaches exact capability parity with the Decimal128 parent, closing
  the documented asymmetry. The faithful contract is proven by the
  extended `tests/property_exp.rs` / `tests/property_ln.rs` and the
  new `tests/property_cbrt.rs`.

### Changed

- `Decimal64::cbrt` is now faithfully rounded (≤ 1 ULP at 16 digits,
  every IEEE 754-2019 rounding direction) via the shared
  `ferrodec-transcend` kernel, replacing the `f64` / `libm::cbrt`
  detour. Behaviour-improving, not a bug fix. The `cbrt` special-value
  short-circuit and the ADR-0016 Kani shim are byte-identical to
  before; only the finite non-zero result path changes. `cbrt` stays
  under the `pow` feature.

## [1.4.0] - 2026-05-15

The decimal64 correctness train. ADR-0017 carved this slice out of
the 1.15 cycle after Slice D's conformance dispatcher found three
H-class correctness bugs in `Decimal64` that mirror `Decimal128`'s
pre-1.13 H-tier shapes; the dispatcher gap had masked their absence
during the 1.13.x decimal128 fixes. A six-agent correctness review
(ADR-0010 methodology) then swept the whole decimal64 op surface and
produced 9 H, 14 M, 17 L findings. This release closes the H and M
tiers, the L-tier drift, wires the full arithmetic conformance
dispatch, and records the train in ADR-0018 (which supersedes
ADR-0017). See the parent `ferrodec` crate's CHANGELOG for cycle
context.

This is a minor bump rather than a patch: the H1 and H2 fixes change
observable outputs on inputs that were previously wrong against the
spec, and `to_f64` gains a breaking signature change (below). A
downstream consumer that implicitly relied on the old behaviour
should read the Changed section.

### Changed

- **BREAKING**: `Decimal64::to_f64` signature changes from
  `fn to_f64(self) -> f64` to
  `fn to_f64(self, rm: RoundingMode) -> (f64, Status)`. A signaling
  NaN input now raises `Status::INVALID` per IEEE 754-2019 §5.4.1
  instead of being silently quieted (H7). Mirrors the `Decimal128`
  `67bd45c` change. Migration: pass a rounding mode and take `.0`
  for the value, `.1` for the status.

- `Decimal64::to_f32` takes the direct decimal path instead of a
  `to_f64` detour, removing a double-rounding error on values near
  the f32 boundary; raises `INVALID` on signaling NaN and
  `OVERFLOW` / `UNDERFLOW` / `INEXACT` consistently (M4).

- Integer conversions (`to_i32` / `to_i64` / `to_i128` / `to_u32` /
  `to_u64` / `to_u128`) are now exact per IEEE 754-2019 §5.4.1 with
  no `f64` detour, rounding `NearestEven`; the `num-traits` feature
  no longer pulls `dep:libm` (M5).

### Fixed

- **H1**: finite-finite addition no longer loses the result
  magnitude when exactly one operand is zero with a wide exponent
  gap (`ddadd360`: `add(...)` returned `0E+50` where `1.0000E+5`
  was expected). The other operand is now requantised to the §6.3
  preferred quantum.

- **H2**: effective-subtract residue attribution at the 16-digit
  precision boundary now borrows correctly
  (`ddadd71100..71119` plus the negated mirror, one `ddMultiply`
  case, and 20 `ddFMA` mirrors).

- **fd-d47** (residual H2, surfaced by the conformance dispatch):
  the add and FMA alignment used a static 22-digit window that
  truncated the lower operand prematurely when the dominant
  coefficient was small (`1E16` is stored as `coef 1, exp 16`),
  misrounding the boundary tie. Replaced with a dynamic per-side
  shift bound keyed on the actual digit count (the `rem.rs` H5
  approach), so the subtraction stays exact whenever it fits in
  `u128`. Cleared 20 `ddAdd` and 8 `ddFMA` boundary cases.

- **H3**: `Decimal64::fma` no longer feeds an out-of-range biased
  exponent into `pack_finite` (`ddfma2504`). Introduced typed
  `BiasedExp` and `Coefficient` newtypes in `bid.rs` whose
  constructors prove the range, removing the `debug_assert!` that
  release builds compiled out; the §6.3 + §7.4 clamp now raises
  `Status::CLAMPED`.

- **H4**: `Decimal64::fma`'s early-return paths thread the §6.3
  preferred quantum into the funnel, so a zero or cancelled product
  result returns in the canonical cohort (`fma0306`).

- **H5**: `Decimal64::rem`'s `Division_impossible` predicate now
  bounds the per-side alignment shift dynamically by quotient digit
  count rather than a static gap, so `rem(1E+25, 10^16-1)` resolves.

- **H6**: `Decimal64::quantize` returns a zero coefficient at any
  target quantum in the format's exponent range, instead of
  `(NaN, INVALID)` (`ddqua537`).

- **H7**: `Decimal64::to_f64` raises `INVALID` on signaling NaN
  (see the breaking signature note above).

- **H8**: `Decimal64::parse_str` no longer debug-panics on
  adversarial leading fractional zeros; the digit counters saturate
  and clamp at the exponent magnitude cap.

- **H9**: the IEEE 754-2019 §7.4 informational `Status::CLAMPED`
  flag is now raised at the in-operation clamp sites (`round.rs`
  §6.3 pad and zero-exponent clamp, `div.rs` finite-or-zero over
  Infinity Etiny path).

- **M tier**: subnormal-inexact `UNDERFLOW` (M1); `scaleb`
  GDA n-envelope, removing an `i32` overflow (M2); `from_f64`
  signaling bit-pattern `INVALID` (M3); plus the M4 / M5 conversion
  changes listed under Changed.

- **subtract** NaN-sign: `subtract x NaN` propagates `NaN` (and
  `subtract x -NaN` propagates `-NaN`) with the operand's original
  sign; the prior unconditional `neg()` flipped it. Surfaced by
  wiring the `subtract` conformance arm.

### Added

- The arithmetic conformance dispatch now covers `add`, `subtract`,
  `multiply`, `divide`, and `fma` against the vendored Cowlishaw
  `dd*.decTest` corpus, with exact per-file pass counts
  (`ddAdd` 973, `ddSubtract` 514, `ddMultiply` 444, `ddDivide` 702,
  `ddFMA` 1318, `ddBase` 708) guarded per ADR-0010. The full corpus
  runs with zero failures.

- Kani harnesses and ADR-0016 special-case-only shims for the
  transcendental cluster: `exp` / `ln` (M10), the trigonometric
  family (M11), the hyperbolic family (M12), `pow` / `cbrt` (M13),
  and the quantum-manipulating family (M14).

- `tests/property_transcendentals.rs`: an `astro-float` oracle
  cross-check of the transcendentals at the documented f64-pipeline
  envelope (M15).

- Documentation: a parser threat model (M6), the transcendental
  saturation and argument-reduction precision envelopes (M7 / M8 /
  M9), and the L-tier invariant and diagnostics cleanup (L1..L13,
  L15..L17). L14 (a richer parse error surface) is deferred to v2.0
  as a breaking change.

## [1.3.0] - 2026-05-11

The post-publish six-agent correctness review (2026-05-10) found
two real IEEE 754-2019 violations in the 1.2.0 release plus three
classes of latent correctness bugs in `Decimal64`'s finite-finite
arithmetic that mirror the pre-1.13 `Decimal128` H-tier shapes. The
two IEEE violations ship here in 1.3.0; the three latent
correctness bugs are recorded in the new `KNOWN_ISSUES.md` and will
be fixed in a dedicated decimal64 correctness slice post-cycle (see
ADR-0017 in the workspace root for the discovery narrative). See
the parent `ferrodec` crate's CHANGELOG for the full 1.15 cycle
context.

### Fixed

- `Decimal64::fma`: the `0 × ∞ + NaN_c` branch now propagates `c`'s
  payload per IEEE 754-2019 §6.2.3. Pre-1.3.0 the branch returned
  canonical `Decimal64::NAN` regardless of `c`, silently discarding
  the input NaN's signal. Sibling drift from the correctly-
  implemented `Decimal128::fma`; same shape, same fix.

- `Decimal64::min` and `Decimal64::max`: signaling NaN's payload is
  now preserved (signal cleared) on the `INVALID` arm per
  §6.2.3. Pre-1.3.0 the methods returned `Self::NAN` (canonical
  zero payload) on any sNaN input.

- `serde_bid::deserialize`'s `BitsVisitor` accepts the full
  integer-width matrix (u8 / u16 / u32 / u64 unsigned, i8 / i16 /
  i32 / i64 signed). MessagePack, CBOR, and bincode can hand
  integers in any of those widths; the pre-1.3.0 visitor rejected
  everything narrower than u32.

### Verification

- Decimal64's verify tree now follows the `_special_only_for_kani`
  shim convention (ADR-0016 of the workspace root). Each
  `src/ops/<op>.rs` exposes a `#[cfg(kani)]` shim that returns the
  special-case dispatcher's `Option` directly. Every
  `src/verify/<op>.rs` harness routes through these shims. Local
  timing: full Decimal64 Kani run completes in 19 seconds (pre-
  1.3.0 several harnesses timed out).

- New harnesses match the Decimal32 set, with the addition of
  `min_max_snan_preserves_payload` over a symbolic 4-bit payload
  (Decimal64's `T_MASK` is 50 bits; the propagation path is
  uniform on width, so a low-bits bug is a full-payload bug).

- `total_cmp_no_panic_and_total` is renamed to
  `total_cmp_reflexive` to match the actual claim. Same rationale
  as Decimal32.

### Known issues

- New file `KNOWN_ISSUES.md` catalogues three open correctness bug
  classes that 1.3.0 *does not* fix. Each entry names a decTest
  reproducer ID:

  * **Finite-finite addition magnitude loss** — `ddAdd.decTest:358`
    (`ddadd360`). Result `0E+50` where `1.0000E+5` is expected; the
    value is dropped entirely. Same bug class as Decimal128's
    pre-1.13 H2 parse magnitude finding (which was fixed in 1.13.x
    for Decimal128 but never propagated to the sibling).

  * **Wrong rounding direction at the 16-digit precision boundary**
    — `ddAdd.decTest:802..821` (`ddadd71100..71119`) plus negated
    mirrors at `ddadd71200..71219`. Result `100.0…0` where
    `99.999…9` is expected under `half_even`. ≥ 40 cases in
    `ddAdd` alone; similar failures in `ddSubtract` and
    `ddMultiply`. Same bug class as Decimal128's pre-1.13 H5 FMA
    sub-ULP directional finding.

  * **pack_finite biased_exp precondition panic in FMA** —
    `src/bid.rs:216` debug-asserts on some `ddFMA.decTest` case
    (not yet narrowed). Internal saturation overshoots
    `BIASED_EXP_MAX`.

- The conformance dispatcher in `tests/conformance.rs` remains
  `Apply` / `tosci` only until those three bug classes close. 38
  of the 43 `dd*.decTest` files report 0 passes by design; this
  understates Decimal64's actual surface coverage but accurately
  reflects the *spec-conforming* surface against the IBM decTest
  corpus.

## [1.2.0] - 2026-05-10

### Changed (behaviour, not API)

- `min` / `max` now follow IEEE 754-2019 §9.6 `minimumNumber` /
  `maximumNumber` semantics — a *quiet* NaN is treated as a
  "missing value" and the non-NaN operand is returned. Both
  operands NaN → NaN; signaling NaN still poisons with INVALID.
  This matches `Decimal128::min` / `Decimal128::max`, the General
  Decimal Arithmetic specification, and the IBM decTest
  conformance suite. Previously implemented §5.3.1 `minimum` /
  `maximum`, which propagates qNaN. Both behaviours are 754-2019
  conforming under different op names; the cross-precision-
  consistent choice is `minimumNumber`. Surfaced by the 6-agent
  review.

## [1.1.1] - 2026-05-10

### Fixed

- `fma` no longer drops `c` when one product factor is zero and the
  other has a far exponent. The previous alignment-shift early-
  return assumed `shift > MAX_SHIFT` implies the shifted side
  dominates, but `0 × anything = 0` so the *other* side is the
  answer. `fma(1e50, 0, 1)` now returns 1, not 0; `fma(1, 1,
  0E+50)` returns 1, not 0.
- `fma`'s alignment dispatch now uses a *dynamic* shift bound
  (`digit_count(operand) + shift ≤ 38`) instead of the previous
  static `MAX_SHIFT = 6`. The static bound mis-classified small
  products with comparable `c`: `fma(1, 1, 0.999_999_999_999_999)`
  returned 1 instead of 1.999_999_999_999_999. The dynamic bound
  admits the case through the normal align-and-sum path whenever
  `u128` headroom permits. `POW10_U128` table extended from 7 to
  39 entries to support the wider shift range.

## [1.1.0] - 2026-05-10

### Breaking

- `Decimal64::next_up` and `Decimal64::next_down` now return
  `(Decimal64, Status)` instead of `Decimal64`. Required to
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
  + 10⁻¹⁵ = 5.000000000000001`. The fix renormalises to the
  lowest representable cohort (max coefficient digits, bounded
  by biased_exp = 0) before stepping. Same algorithmic shape as
  Decimal128's mature implementation.
- `next_up` of a signaling NaN now correctly quiets the NaN
  *and* raises INVALID (was: silently passed sNaN through). See
  the Breaking entry above.

## [1.0.3] - 2026-05-10

### Fixed

- `pow(self, exponent)` now correctly short-circuits the
  `pow(1, y) = 1` rule for every cohort of the value 1, not just
  the canonical `1 × 10⁰` bit pattern. `10 × 10⁻¹`, …, `10¹⁵ ×
  10⁻¹⁵` all now route through the short-circuit. Per IEEE
  754-2019 §9.2 the rule is value-bound, not cohort-bound.
  Surfaced by the 6-agent review.

## [1.0.2] - 2026-05-10

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

## [1.0.1] - 2026-05-10

### Changed

- `Status`, `RoundingMode`, and `IeeeClass` now re-export from the
  new [`ferrodec-ieee`](https://crates.io/crates/ferrodec-ieee)
  crate (v0.1.0). The types are byte-compatible with the previous
  release — `ferrodec_decimal64::Status` and `ferrodec::Status`
  resolve to the *same* concrete type, so cross-precision interop
  works without conversion. ADR-0012 records the rationale.

## [1.0.0] - 2026-05-10

Initial release. IEEE 754-2019 Decimal64 in pure Rust, `no_std`-
capable, sized for financial general-ledger arithmetic and
scientific aggregates that outgrow Decimal32's 7 digits without
needing Decimal128's 128 bits of storage.

### Added

- Examples at `examples/`: `money` (16-digit ledger arithmetic with
  cent quantisation and tax accumulation), `rounding_modes` (the
  five IEEE 754 modes applied to halfway cases), `transcendentals`
  (tour of exp / ln / sin / cos / sqrt / acos with INVALID
  propagation).
- README at v1.0 shape: parameters table, feature surface,
  callable methods, accuracy posture (Decimal64 / f64 precision-
  boundary tradeoff documented), supported targets, verification
  pillars, "Why no core::ops" rationale, and the three-way
  decision matrix between ferrodec-decimal32 / ferrodec-decimal64
  / ferrodec.
- Skeleton crate. `Decimal64(u64)` type wrapper, no methods yet.
  Initial groundwork for the full Decimal64 implementation per the
  plan archived at
  `docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`.
  Inherits workspace lints, edition, MSRV (1.84), license, and
  repository metadata. `fmt` and `kani` features are declared with
  empty bodies for future use.
- `binary-float` feature: `Decimal64::to_f64` and
  `Decimal64::from_f64(x, rm) -> (Self, Status)`. Decimal64-to-f64
  computes `coef × 10^exp` via doubling-square `pow10_f64`; the
  cast is rounded to f64 mantissa width (Decimal64's 16 digits vs
  f64's ~15.95). f64-to-Decimal64 renders `{x:.17e}` into a 32-byte
  stack buffer and routes through `parse_str`. 7 unit tests cover
  basic round-trips, specials, and f64::MAX clamping.
- `exp-log` feature: `Decimal64::exp(rm)` and `Decimal64::ln(rm)` per
  IEEE 754-2019 §9.2. Both route through `libm` via the
  binary-float path (pure Rust, no_std-compatible, no FFI).
  v1.0 ships the f64 path as the canonical baseline; a future
  commit can replace it with a pure-decimal Taylor / Newton kernel
  at u128 working precision (the public surface is drop-in
  compatible). Special cases: NaN propagation (sNaN raises
  INVALID), `exp(±0) = 1`, `exp(+∞) = +∞`, `exp(−∞) = +0`, `ln(±0)
  = −∞ + DIV_BY_ZERO`, `ln(negative) = NaN + INVALID`, `ln(+∞) =
  +∞`. 10 unit tests cover all of these plus a round-trip property
  test. Decimal64's exp overflow threshold is at x ≈ 885 (since
  e⁸⁸⁵ ≈ 10³⁸⁴).
- `trig` feature: `Decimal64::sin`, `cos`, `tan`, `asin`, `acos`,
  `atan`, `atan2` per IEEE 754-2019 §9.2. All route through `libm`
  via the binary-float path. Special-case dispatch matches
  ferrodec-decimal32's: NaN propagation, `sin / cos / tan(±∞) =
  NaN + INVALID`, `cos(±0) = 1`, `atan(±∞) = ±π/2`, domain
  enforcement for `asin` / `acos` (|x| > 1 → INVALID). 11 unit
  tests.
- `hyperbolic` feature: `Decimal64::sinh`, `cosh`, `tanh`, `asinh`,
  `acosh`, `atanh` per IEEE 754-2019 §9.2. Routes through libm.
  Domain enforcement: `acosh(x < 1) = NaN + INVALID`, `atanh(±1) =
  ±∞ + DIV_BY_ZERO`, `atanh(|x| > 1) = NaN + INVALID`. 9 unit
  tests. Implies `exp-log`.
- `pow` feature: `Decimal64::pow(self, exponent, rm)` and
  `Decimal64::cbrt(self, rm)` per IEEE 754-2019 §9.2. `pow` follows
  ISO C / f64::powf semantics: `pow(±0, +y) = +0`, `pow(±0, −y) =
  ±∞ + DIV_BY_ZERO`, `pow(1, y) = 1` for any y including NaN, `pow(x,
  0) = 1` for any x including NaN, negative-base non-integer
  exponent → NaN + INVALID. Overflow propagates through f64::INFINITY
  detection. cbrt accepts negative inputs (real cube root).
  9 unit tests.
- `ops` feature: `core::ops` operator overloads for `Decimal64`:
  Add, Sub, Mul, Div, Rem and their *Assign variants, plus Neg.
  Default rounding is NearestEven; per-operation Status is dropped.
  Callers needing explicit rounding-mode or status control should
  keep using the methods. 3 unit tests cover basic arithmetic,
  Neg, and the *Assign forms.
- `serde` feature: `Serialize` and `Deserialize` for `Decimal64`.
  Default path serializes the canonical decimal string. The
  `serde_bid` helper module serializes the raw 64-bit BID pattern
  for binary formats; visit_u32 / visit_u64 / visit_str accepted
  so binary and text deserializers both work.
- `num-traits` feature: `Zero`, `One`, `Bounded`, `Num`, `Signed`,
  `FromPrimitive`, `ToPrimitive`. `FromPrimitive::from_i64` and
  `from_u64` route through `try_new` for the exact Decimal64 range
  (|n| < 10¹⁶), falling back to the f64 round-trip for larger
  magnitudes (Decimal64 has more range than Decimal32, so most
  i64 / u64 values land in the exact path). Implies `ops` and
  `binary-float`.
- cargo-fuzz harness suite at `fuzz/`. Four targets mirror
  ferrodec-decimal32's:
  * `parse` — arbitrary byte strings through `parse_str`. Verifies
    no panic, Display round-trips back through parse_str.
  * `arith` — arbitrary `(u64, u64)` bit-pattern pairs through
    add / sub / mul / div / rem. Asserts no panic, plus `a + 0 = a`
    (numerically) and `a − a = ±0` for non-NaN, non-Inf a.
  * `transcendentals` — arbitrary `u64` bit patterns through all
    enabled transcendental kernels (exp, ln, sin, cos, tan,
    asin, acos, atan, sinh, cosh, tanh, asinh, acosh, atanh,
    cbrt, sqrt). Panic-freedom only.
  * `total_cmp` — arbitrary `(u64, u64, u64)` triples through
    `total_cmp` / `partial_cmp` / `compare_total_magnitude`.
    Verifies anti-symmetry, reflexivity, and a transitivity
    surrogate (a ≤ b ≤ c → a ≤ c).
  Bit-pattern type bumped from Decimal32's `u32` to Decimal64's
  `u64` throughout. Run via `cargo +nightly fuzz run <target>`
  from the `ferrodec-decimal64` package; CI does not gate fuzz
  (smoke runs are local). The fuzz sub-crate lives outside the
  Cargo workspace by convention.
- Kani verification harnesses at `src/verify/` (cfg(kani)-gated).
  Six modules (addsub, mul, div, sqrt, fma, cmp) mirror
  ferrodec-decimal32's verify/ tree: bounded 10-constant operand
  selector keeps the SAT problem tractable; harnesses prove
  no-panic and IEEE 754 special-case propagation. CI's `kani` job
  extended with `cargo kani --package ferrodec-decimal64
  --features=fmt`.
- Comparison and ordering: `Decimal64::partial_cmp`,
  `Decimal64::total_cmp`, `Decimal64::compare_total_magnitude`,
  `Decimal64::min`, `Decimal64::max` (mirrors Decimal32's surface
  per IEEE 754-2019 §5.6.1, §5.10, §5.3.1). 13 unit tests.
- Quantum-manipulating operations: `Decimal64::quantize`, `scaleb`,
  `logb`, `next_up`, `next_down` (mirrors Decimal32's per
  IEEE 754-2019 §5.3.1 / §5.3.3). 13 unit tests.
- `Decimal64::sqrt` and `Decimal64::fma` per IEEE 754-2019.
  sqrt scales the working coefficient to 33 or 34 decimal digits in
  u128 (parity-matched to the working exponent's parity), takes
  `u128::isqrt` to land in `[10¹⁶, 10¹⁷)` for 17-digit precision,
  and routes through `round_and_pack_into_u64`. Special cases per
  §5.4.1: NaN propagation, sqrt(±0) = ±0, sqrt(+∞) = +∞,
  sqrt(−∞) and sqrt(−finite) → NaN+INVALID. Preferred quantum is
  floor(Q(x) / 2). 8 unit tests cover perfect squares, sqrt(2)
  inexact, zero (with sign preservation), one, negative-input
  INVALID, infinities, and NaN propagation. fma forms the exact
  product u64 × u64 → u128, aligns with `c` over u128 with
  MAX_SHIFT = 6 (since the product can have up to ~32 digits and
  u128 caps at ~38 digits), then routes through
  `round_and_pack_into_u64`. Special cases per §5.4.1 and §7:
  0×∞ in the product → NaN+INVALID, ±∞×finite → ±∞ XOR, ±∞ + ∓∞
  from the addition → NaN+INVALID, NaN propagation in argument
  order. 8 unit tests.
- `Decimal64::mul`, `Decimal64::div`, `Decimal64::rem` per
  IEEE 754-2019 §6.3 / §7. mul: `u64 × u64 → u128` (max product
  (10¹⁶ − 1)² ≈ 10³², fits in u128 with headroom), routed through
  `round_and_pack_into_u64` with the q_preferred = exp_a + exp_b
  cohort rule. div: scale dividend by `10^(db − da + PRECISION + 1)
  = 10^(db − da + 17)` over u128 working precision so the integer
  quotient holds ≥ 17 digits, then `round_and_pack_into_u64` with
  the post-scale remainder as the sticky bit. rem: align both
  operands at `target_q = min(Q(a), Q(b))` over u128 working
  precision; `MAX_SAFE_SHIFT = 22` short-circuits to NaN+INVALID
  on `|a| ≫ |b|` and to `a` itself on `|b| ≫ |a|`. 11 unit tests
  for mul, 10 for div, 8 for rem.
- `Decimal64::add(self, other, rm)` and `Decimal64::sub(self, other, rm)`
  per IEEE 754-2019 §6.3 / §7. Same algorithmic shape as
  ferrodec-decimal32's add / sub but at u128 working width because
  Decimal64's 16-digit coefficients plus alignment shifts can
  exceed u64. ALIGN_LIMIT = 22 (max safe shift in u128 with
  10^16 coefficient max ≈ 3.4 × 10³⁸); WORKING_PRECISION = 23.
  `round_and_pack_into_u64` helper compresses the u128 result back
  to u64 via sticky tracking before routing through
  `round_and_pack_finite`. 11 unit tests cover basic arithmetic,
  carry-renormalisation across the 16-digit boundary
  (9_999_999_999_999_999 + 1 = 10¹⁶), alignment-induced
  inexactness, sign-disagreement cancellation, zero combinations,
  NaN propagation, Infinity arithmetic, overflow, and
  finite-plus-zero cohort preservation.
- Rounding kernel at `src/ops/round.rs`. The
  `round_and_pack_finite(coef: u64, unbiased_exp, q_preferred, sign,
  pre_sticky, rm, status)` entry point handles digit drop with
  guard / sticky tracking, applies the five IEEE 754 rounding modes,
  renormalises across power-of-10 boundaries, shifts toward the
  preferred quantum, and emits `INEXACT` / `OVERFLOW` / `UNDERFLOW`
  flags. Includes IEEE 754-2019 §6.3 exponent clamping: when the
  result's biased exponent exceeds `BIASED_EXP_MAX` but the
  adjusted exponent is in range, the coefficient is padded with
  trailing zeros to fit the encoding (the "Clamped" condition).
  7 unit tests cover the rounding axes, overflow, underflow,
  carry-renormalisation, and zero-quantum preservation.
- `parse_str(&str, RoundingMode) -> Result<(Decimal64, Status),
  ParseDecimalError>` under the `fmt` feature. Up to 19 mantissa
  digits accumulated exactly in `u64`; trailing digits feed the
  rounding sticky bit. Leading zeros after the decimal point shift
  the quantum without spending the `MAX_PARSED_DIGITS` budget — a
  bug-fix carried back to ferrodec-decimal32 in the same commit.
- `Display`, `LowerExp`, `UpperExp`, and `Engineering` adapters
  under the `fmt` feature. Same toSci convention as
  ferrodec-decimal32: plain decimal notation when unbiased exponent
  ≤ 0 and adjusted exponent ≥ -6, otherwise scientific with `E±N`.
  18-byte stack scratch buffer for the digit string (room for 16
  digits + transient overflow during pre-rounded rendering).
- Conformance harness now dispatches `tosci` / `apply`. Per-file
  expectation table records the C7 + C8 baseline:
  * `ddBase.decTest`: 708 of 945 pass.
  * `ddAdd.decTest`: 2 of 1091 (toSci edge cases not exercising
    add).
  * `ddFMA.decTest`: 2 of 1378 (same shape).
  * `ddEncode.decTest`: 0 of 268 (deferred to a dpd-feature
    commit).
  Total: 712 of 14 428 cases pass, 0 fail, 13 716 skip.
- `Decimal64` struct moved from `lib.rs` into `decimal.rs` alongside
  IEEE 754 distinguished constants (`ZERO`, `NEG_ZERO`, `ONE`,
  `NEG_ONE`, `TEN`, `MAX`, `MIN`, `MIN_POSITIVE`,
  `MIN_POSITIVE_NORMAL`, `INFINITY`, `NEG_INFINITY`, `NAN`,
  `SIGNALING_NAN`), `from_bits` / `to_bits` (raw u64 round-trip),
  `try_new` and `try_new_unsigned` constructors (return
  `Decimal64BuildError` on coefficient or exponent out-of-range),
  and a `Debug` impl that surfaces the bit pattern and decoded
  class.
- Classification predicates and operations: `is_nan`,
  `is_signaling_nan`, `is_quiet_nan`, `is_infinite`, `is_finite`,
  `is_zero`, `is_normal`, `is_subnormal`, `is_sign_negative`,
  `is_sign_positive`, `classify` (returning `core::num::FpCategory`),
  `ieee_class` (returning `IeeeClass`), `abs` and `neg` (no status),
  `abs_with_status` and `neg_with_status` (raise `Status::INVALID`
  on signaling-NaN input, otherwise quiet), `copysign`,
  `is_canonical` (handles BID-64's Form A / Form B asymmetry: Form
  A always canonical because its 53-bit coefficient field is
  bounded below 10¹⁶; Form B canonical iff the decoded coefficient
  is < 10¹⁶), and `canonicalize` (rewrites non-canonical inputs to
  the equivalent canonical encoding). 21 unit tests cover the
  predicates, sign manipulation, canonicalisation, and the
  `Decimal64` constructor surface.
- Conformance harness skeleton at `tests/conformance.rs` (gated on
  the `fmt` feature). Parses `.decTest` files into structured cases
  with directive-aware context (precision, max/min exponent,
  rounding); dispatches every case to a stub that returns `Skip`
  pending implementation of the operations. Loads all 14 428 cases
  across the 42 vendored `dd*.decTest` files. The asymmetric
  per-file expectation guard (per ADR-0010) starts with
  `ddBase.decTest = 0` and `ddEncode.decTest = 0`; each subsequent
  commit that wires a dispatch arm raises the rows it now passes.
  CI runs the harness in the `decimal64` job under `--features=fmt`.
- Vendored IBM decTest conformance vectors at `tests/vectors/`. All
  42 `dd*.decTest` files extracted from the speleotrove archive
  (17 901 lines total). Coverage spans every IEEE 754-2019 §5
  operation at decimal64 precision (16 digits, exponent range
  `10⁻³⁸³..=10⁺³⁸⁴`), plus GDA-specific ops (`and`, `or`, `xor`,
  `rotate`, `shift`, `invert`, `copy*`) that sit outside IEEE 754
  and will skip in the harness. The conformance harness consuming
  these vectors lands in C5.
- BID encoding foundation: parameters, decoder, encoder, helpers per
  IEEE 754-2019 §3.5.2 for decimal64. Form A (coefficient < 2⁵³)
  and Form B (coefficient ∈ [2⁵³, 10¹⁶)) are both canonical and
  handled symmetrically; non-canonical Form B encodings (coefficient
  ≥ 10¹⁶) decode to ±0 with the encoded sign and biased exponent,
  matching ferrodec / ferrodec-decimal32 canonicalisation
  discipline. 13 unit tests cover round-trip pack/unpack across a
  sweep of (sign, biased_exp, coefficient) triples spanning both
  forms and the canonical boundary, plus Intel-reference bit
  patterns for Inf and NaN. The module-level `#![allow(dead_code)]`
  is transient: BID items become consumed when classify, parse,
  format, and arithmetic modules land in subsequent commits.
- Shared IEEE 754 metadata types: `Status`, `RoundingMode`,
  `IeeeClass`. `Status` and `RoundingMode` are duplicated verbatim
  from ferrodec-decimal32 (the file is fully precision-agnostic).
  `IeeeClass` is adapted from ferrodec-decimal32: same enum shape
  with doc text retargeted from Decimal32 to Decimal64. Three
  consumers now exist (ferrodec, ferrodec-decimal32,
  ferrodec-decimal64); the shared `ferrodec-ieee` extraction lands
  in a follow-on Phase D commit.
