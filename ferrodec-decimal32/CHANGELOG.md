# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
