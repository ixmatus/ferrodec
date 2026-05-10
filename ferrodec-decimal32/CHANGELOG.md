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
