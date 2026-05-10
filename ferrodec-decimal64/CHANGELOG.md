# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
