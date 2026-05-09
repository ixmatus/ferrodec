# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.14.3] - 2026-05-09

Documentation-only release. No code change; the published artifact
behaves identically to 1.14.2. Closes the polish backlog from the
6-agent review of 1.12.0.

### Documentation

- **tanh saturation threshold rationale** in `src/math/hyperbolic.rs`.
  The `|x| > 80` saturation cutoff is conservative; the true
  `|tanh(x) − 1| < ulp(1)` boundary at 34-digit precision sits at
  `|x| ≈ 38`. The gap is intentional (composes with sinh / cosh's
  shared `EXP_OVERFLOW_LIMIT = 14150` ceiling) and rarely hit in
  practice. Comment now documents both the reasoning and the
  rounding-correctness floor.
- **acosh log1p threshold cancellation budget** in
  `src/math/hyperbolic.rs`. The `0.01` boundary was justified by
  Taylor convergence on the log1p side; comment now also documents
  the cancellation budget on the direct `x² − 1` side (loss of
  ≈ 2 digits at the boundary, well inside Extended's ~16-digit
  headroom over Decimal128). Future tweaks know what they're
  trading.
- **FMA overflow sub-ULP sticky-sign safety note** in
  `src/ops/fma.rs`. The overflow branch seeds `sticky = true` (a
  positive residue) regardless of c's sign relative to the
  product. Past MAX, a sub-ULP residue can't pull magnitude back
  across the boundary, so `overflow_result` picks ±MAX or ±∞ from
  `(sign, rm)` alone — the sticky-sign confusion that drove the
  M6 c_too_wide and ab_too_wide fixes is not observable here.
  Comment makes the safety argument explicit so a future reader
  doesn't try to "fix" it by mirror.

The HIGH / MEDIUM / LOW correctness backlog from the 6-agent
review is now fully closed.

## [1.14.2] - 2026-05-09

### Fixed

- **FMA single-rounds the opposite-sign ab-dominates-in-range
  path** (M6, second half). Phase O closed the same-sign half of
  M6 in 1.13.1; the opposite-sign half stayed deferred under a
  documented caveat. Now closed too. The new
  `fma_ab_dom_in_range_eff_sub` helper splits on
  `digits(cab) ≤ PRECISION` (D_a == 0) versus `digits(cab) >
  PRECISION` (D_a > 0):

  * **D_a > 0**: cab's natural rounding agrees with the IEEE 754
    §5.4.1 single-rounding contract everywhere except the exact
    `(round_digit_a == 5, !sticky_a)` tie under nearest modes.
    There the legacy mul-then-add formulation rounds UP under
    banker's-tie-with-odd-parity (NearestEven) or away-from-zero
    (NearestAway), but the true value `kept + 0.5 − epsilon_c`
    lies just below half-ULP, so the correctly rounded answer is
    `kept`. The helper detects the tie and forces round-down.

  * **D_a == 0**: cab is exact at quantum qab; c is the only
    sub-ULP residue. The shape is exactly addsub's
    effective-subtraction domain, including cases where the
    epsilon is *near* 0.5 ULP (which the local
    `sub_ulp_eff_sub_c_dominates` cannot handle because it
    hardcodes "eps ≪ 0.5 ULP" for the c_too_wide regime). Defer
    to addsub: build cab and c as Decimal128 values and call
    `cab.add(c, rm)`, which routes through addsub's
    `sub_ulp_effective_sub` for the actual eps-vs-half-ULP
    comparison. Closes the conformance regression on
    `dqadd371322..324` (`fma 1 1E34 -0.50…01`) that an earlier
    attempt to use `sub_ulp_eff_sub_c_dominates` introduced.

  Closes the M6 finding from the 6-agent correctness review in
  full. Conformance unchanged at 9080 / 0 / 253.

### Internal

- New unit tests:
  * `fma_ab_dominates_in_range_opposite_sign_ties_down` constructs
    the `(5, 0)` tie + odd parity + opposite-sign + sub-ULP c
    shape (cab = 5 × (2×10^33 + 3) = 10^34 + 15) and asserts the
    single-rounded answer is one ULP smaller than the legacy
    mul-then-add answer.
  * `fma_ab_dominates_in_range_opposite_sign_directional` covers
    the no-disagreement path (round_digit > 5 always rounds up
    regardless of c sign) plus TowardZero on positive.

## [1.14.1] - 2026-05-09

### Performance

- **Subnormal-underflow fast path** (M10). The
  `MIN_SUBNORMAL / MAX` shape (and any other path that hits
  `round_and_pack_finite`'s underflow branch with `shift >= digits`,
  i.e. the entire coefficient drops below the smallest
  subnormal LSD) used to spin up to ~6111 U256 `div_rem10`
  iterations before returning ±0 with `UNDERFLOW + INEXACT`. On
  Cortex-M0+ (no hardware divide) every iteration pulls in
  `__udivti3`, so the loop dominated the soft-realtime budget on
  any embedded target that ever divided two extreme-magnitude
  decimals.

  New `round_digit_for_full_drop` helper computes the rounding
  inputs in O(digits) bounded work (worst case ≤ 78 iterations
  for a U256 coefficient, typically ≤ 34). Apple Silicon bench:
  `div_min_subnormal_over_max` 66.8 µs → 1.16 µs (**57× faster**).
  Common `div` path is unchanged.

  The `extract_dropped_digits` hot loop is also unchanged; the
  digit-count check that the original Phase U attempt put inside
  it cost ~30% on the common `div` path because
  `decimal_digit_count` is itself O(digits). Pushing the gate up
  to the underflow call site (where `digits` is already in scope
  from the precision-overflow check) avoids that overhead.

### Internal

- New bench `div_min_subnormal_over_max` in `benches/core_ops.rs`
  pins the M10 perf floor so any future regression that
  reintroduces the loop becomes visible in CI.

## [1.14.0] - 2026-05-09

### Added

- **`IeeeClass` enum and `Decimal128::ieee_class()`** (M9). The
  IEEE 754-2019 §5.7.2 `class(x)` operation, exposing all ten
  classes the standard distinguishes: `SignalingNaN`,
  `QuietNaN`, `NegativeInfinity`, `NegativeNormal`,
  `NegativeSubnormal`, `NegativeZero`, `PositiveZero`,
  `PositiveSubnormal`, `PositiveNormal`, `PositiveInfinity`.
  Quiet by IEEE definition: an sNaN input does not raise
  `Status::INVALID`. The existing `Decimal128::classify() ->
  core::num::FpCategory` (five variants) stays unchanged for
  parity with `f32` / `f64`. NaN classes are unsigned by
  convention; the sign bit on a NaN remains observable through
  `Decimal128::is_sign_negative` but does not split
  `QuietNaN` / `SignalingNaN` into signed variants.

  This closes the M9 finding from the 6-agent review of 1.12.0,
  which flagged the absence of a public ten-class enum as an API
  completeness gap against the IEEE standard.

  Minor version bump because the new enum and method are public
  API additions.

## [1.13.1] - 2026-05-09

Closes the remaining MEDIUM and LOW backlog from the 6-agent
review of 1.12.0. No public API changes; one observable behavior
change for a previously untriggered FMA tie shape (M6).

### Fixed

- **`total_cmp` same-rank arms assert sign agreement** (M1).
  The same-rank dispatch destructured both operands but passed
  only the *left* operand's sign to the tie-break helpers,
  relying on the rank table's implicit "same rank means same
  sign" invariant. The discard worked in practice but made the
  kernel fragile under future rank-table changes. Each same-rank
  arm (NaN, Zero, Finite) now captures both signs and asserts
  they agree, with two new regression tests covering NaN payload
  and zero cohort antisymmetry across both signs.

- **DPD round-trip contract documented as projection-via-canonicalize**
  (M11). NaN payloads at or above `10^33` cannot be represented as
  11 declets, so `from_dpd_bytes(to_dpd_bytes(x))` is bit-equal
  to `x.canonicalize()`, not necessarily to `x`. The encode
  behavior was already correct; the public docs were silent
  about the contract. `to_dpd_bytes` rustdoc gains an explicit
  "Round-trip contract" section listing the three classes of
  non-canonical input that collapse on encode. The proptest in
  `property_from_bits.rs` tightens its NaN branch to bit-equal
  comparison against `d.canonicalize().to_bits()`, and a new
  unit test walks the boundary across both NaN flavours and
  both signs.

- **FMA single-rounds the same-sign ab-dominates-in-range path** (M6).
  When the exact product `a × b` has its 35th digit on a `5000…0`
  tie that round-half-even would resolve down (kept LSB even),
  and `c` is sub-ULP same-sign at a quantum far below `qab`, the
  legacy mul-then-add formulation lost the ability to use c's
  sticky to break the tie up. The FMA kernel now routes
  same-sign through `sub_ulp_round(cab, qab, sab, false, rm)`,
  which gives the correctly single-rounded answer. Opposite-sign
  retains the legacy path under a documented caveat (the proper
  fix would have to subtract c's sub-ULP residue from cab's
  natural drop residue and re-decide the round; impact bounded
  by the rarity of an opposite-sign 35th-digit exact tie).

### Internal

- **addsub `decide_round_up` Equal arm** (L1). The case-by-case
  bound analysis showed `compare_two_cs_to_ten_pow` can never
  return `Equal` on this path under the caller's preconditions.
  Replace the silent "fall back to round up" comment with
  `debug_assert!(false, …)` so any future relaxation of
  `align_limit_for` fails loud rather than silently exposing a
  parity bug. Release builds keep the round-up fallback as the
  only sound choice if the invariant breaks.

- **addsub `add_finite_finite` post-cancellation** (L4).
  Document and assert that the `combined.is_zero() && !sticky`
  branch is reachable only via `effective_sub`.

- **`min` / `max` doc clarification**. The module header claimed
  "IEEE 754-2019 §9.6 minimum/maximum", but the implementation
  matches the §9.6 *minimumNumber* / *maximumNumber* variants
  (qNaN as missing value, not as poison). Rewrite to make the
  variant explicit and cross-reference both 754-2008 minNum and
  the General Decimal Arithmetic spec.

- **trig qNaN payload preservation test**. New unit test
  `trig_qnan_preserves_payload_bit_for_bit` walks `sin` / `cos` /
  `tan` against a distinctive qNaN and sNaN payload, asserts
  bit-identity for qNaN and payload survival across the sNaN
  quietening step. Pins the contract a future refactor that
  funnelled qNaN through `nan_from`'s canonicalize call would
  break.

- **argred docstrings corrected** (M8 plus trig M3).
  `FRAC_DIGITS` docstring now states the real margin (43 − 34 = 9
  digits past Decimal128's 34-digit envelope, which is the margin
  callers care about). The `I_HI_OFFSET` figure in
  `make_residual`'s comment was 77 from a much older
  `FRAC_DIGITS`; it is now 113 (`76 + 33 + 4`), with `q_max +
  113 = 6224 ≤ 6300` recomputed accordingly. No code change.

## [1.13.0] - 2026-05-09

Eleven correctness bugs surfaced by a 6-agent review of the 1.12.0
release: six HIGH-severity (closed first; an intermediate 1.12.1
version was drafted but never published) plus five MEDIUM-severity
(the additions that justified the 1.13.0 minor bump). Each fix is
observable through the public API and each ships with a regression
test that locks down the new contract.
[ADR-0010](docs/decisions/0010-testing-strategy-after-six-agent-review.md)
documents the testing-strategy response that should prevent the
same shapes recurring.

A minor version bump because some of these changes alter the
status or return value for previously-reachable inputs:
* `pow(-1, ±∞)` no longer panics; returns `1` per IEEE 754-2019
  §9.2.1.
* `to_u32(-0.4)` now returns `(0, INEXACT)` instead of
  `(0, INVALID)` (consumers that pattern-matched on INVALID for
  any negative input will need to handle the new shape).
* `Decimal128::sqrt(0E+k)` now returns a zero with quantum ⌊k/2⌋
  instead of preserving the input quantum.
* `to_f32` now returns the directly-rounded result instead of
  `to_f64() as f32` — for values on f32 half-ULP boundaries the
  bit pattern can change by 1 ULP.
* `Decimal128::exp(x)` for `x ∈ (−14221, −14150]` now returns the
  representable subnormal instead of saturating to 0.
* `fma(0, ∞, NaN_c)` now propagates `c`'s NaN payload instead of
  returning canonical `NaN`.
* `to_f64` / `to_f32` now raise INVALID on a signaling-NaN input
  (was silently returning a quiet NaN with OK status).
* Parsing pure integer literals longer than 76 digits no longer
  silently drops magnitude (`"1" × 77` previously parsed to
  `1.111…E+75` instead of `1.111…E+76`).
* `Decimal128::from_bits` with a non-canonical Form A bit pattern
  (coefficient ≥ 10^34) now decodes as zero per IEEE 754-2019
  §3.5.2; arithmetic on such inputs previously produced silently
  wrong results (~3.8%) and `to_dpd_bytes` panicked.

### Fixed — HIGH-severity

- **Non-canonical Form A coefficients arithmetised as real values**
  (closes a contract gap reachable through `Decimal128::from_bits`
  with a hand-built 128-bit pattern). IEEE 754-2019 §3.5.2 requires
  Form A encodings whose coefficient field exceeds 10^p − 1 to be
  treated as the floating-point datum zero with the encoded sign
  and biased exponent. The decoder previously deferred this to the
  arithmetic layer but no kernel actually performed the check, so a
  poisoned coefficient (`≈ 1.038 × 10^34`) participated in
  arithmetic and produced results ~3.8% wrong. The same input
  panicked `to_dpd_bytes` in debug and corrupted DPD output in
  release. Fixed by canonicalising on decode in `bid::classify_bits`;
  every downstream consumer (arithmetic, DPD encode, format) is
  safe by construction. Closes review findings H3 and H4.

- **`pow(-1, ±∞) → 1` instead of panicking** (H1). IEEE 754-2019
  §9.2.1 rule 5 specifies `pow(±1, ±∞) = 1`; rule 2's short-circuit
  only matched `x = +1` (deliberately, so `pow(-1, qNaN)` can still
  propagate NaN), so the negative-base case landed in rule 6's
  `unreachable!()`. Fixed by handling the |x| = 1 arm directly in
  rule 6.

- **Parse correctly scales integer literals beyond 76 digits** (H2).
  The mantissa loop folded the first 76 digits into a U256
  coefficient and pushed any further digits into a sticky bit, but
  a saturating-counter TODO never compensated the quantum, so any
  pure-integer literal longer than 76 digits silently dropped 10×
  per excess digit (`"1" × 77` parsed to `1.111…E+75` instead of
  `1.111…E+76`). Fixed by tracking `extra_int_digits` and adding
  to the final unbiased exponent.

- **FMA sub-ULP effective-subtraction directional rounding** (H5).
  When `c` dominates and the product `a × b` is sub-ULP, the
  existing `sub_ulp_round` path correctly handles same-sign
  (epsilon pushes magnitude up) but silently picks the wrong
  neighbour for opposite-sign sub-ULP (true value sits *below*
  `cc · 10^qc`). Symptom: `ONE.fma(1e-6176, NEG_ONE, TowardPositive)`
  returned `-1.0` instead of `-0.999…9`. Fixed by routing
  opposite-sign through a new helper that mirrors
  `addsub::sub_ulp_effective_sub`'s candidate selection, threading
  the IEEE 754 §6.3 preferred quantum so `round_and_pack_finite`
  pads trailing zeros correctly. Picks up four previously-failing
  dqFMA conformance cases (dqadd36466 / 36476 / 36506 / 36516).

- **`to_f64` / `to_f32` raise INVALID on signaling NaN** (H6). IEEE
  754-2019 §5.4.2 (convertFormat) requires sNaN to raise INVALID
  and yield a quiet NaN; the previous implementation collapsed
  both qNaN and sNaN to `(f64::NAN, OK)`. A pinned unit test
  actively held the wrong behaviour in place.

### Fixed — MEDIUM-severity

- **`sqrt(±0)` returns dec-spec quantum ⌊q/2⌋** (M2). The sqrt
  special-case path acknowledged the gap in a comment but kept the
  input quantum verbatim, leaving cohorts wrong for any non-zero
  quantum input. Fix uses `i32::div_euclid(2)` to floor toward −∞
  (matching the spec for negative quanta — q = −33 → q′ = −17,
  not −16).

- **`to_unsigned` routes negative finites through rounding** (M4).
  Per IEEE 754-2019 §5.4.1, INVALID is raised only when the
  *rounded* integer is out of range. The old `to_unsigned`
  short-circuited on `Class::Finite { sign: true, .. }` before any
  rounding step, so `to_u32(-0.4)` reported INVALID instead of
  `(0, INEXACT)`. The fix is narrow: rounded_abs == 0 returns
  `(0, status)`; rounded_abs ≥ 1 still raises INVALID for
  out-of-range negatives.

- **`to_f32` takes a direct decimal-string path** (M3). The old
  `to_f64() as f32` collapsing was the classic C `(float)(double)x`
  double-rounding hazard. Now mirrors `to_f64`'s structure but calls
  `f32::from_str` on the canonical Display output so a single
  rounding step keeps the result inside the f32 envelope. Inherits
  the H6 sNaN-raises-INVALID rule.

- **FMA `0×Inf` preserves c's NaN payload** (M5). Per IEEE 754-2019
  §6.2.3 the result NaN should carry the input NaN's payload when
  one is available. The old `0 × Inf` branch raised INVALID
  correctly but always returned canonical `NAN`, dropping c's
  payload even when c was the only NaN operand. Fix routes through
  `propagate_nan3(a, b, c)` when c is a NaN; non-NaN c still gets
  canonical `NAN` (the fix is narrow).

- **`exp` underflow threshold matches the real boundary** (M7).
  The pre-1.13 domain gate was symmetric at ±14150, but
  `MIN_SUBNORMAL = 1×10⁻⁶¹⁷⁶` pushes the underflow boundary to
  `ln(½ × MIN_SUBNORMAL) ≈ −14220.85`. Inputs in (−14221, −14150]
  silently saturated to 0 instead of producing the subnormal result
  the Taylor pipeline can compute. `EXP_DOMAIN_LIMIT` splits into
  `EXP_OVERFLOW_LIMIT = 14150` and `EXP_UNDERFLOW_LIMIT = 14221`;
  `sinh` / `cosh` migrate to the overflow limit because both
  saturate symmetrically.

### Added

- **ADR-0010**:
  [Testing strategy after the 6-agent correctness review](docs/decisions/0010-testing-strategy-after-six-agent-review.md).
  Documents what each HIGH bug taught us and the three new test
  layers added to catch the same shapes earlier.

- **Per-file conformance expectation table** in
  `tests/conformance.rs`. Replaces the global pass-floor with
  exhaustive per-file `(name, expected_passes)` rows; any silent
  trade-off (`pass↑file_a + pass↓file_b` = total unchanged) becomes
  a hard failure. Legitimate count changes require a one-line edit
  that surfaces in code review.

- **`tests/property_from_bits.rs`** — proptest fuzzes arbitrary
  `u128` through `Decimal128::from_bits` and asserts: classify
  totality, canonicalize-as-projection, `to_dpd_bytes` totality,
  `add(d, 1)` agrees with `add(d.canonicalize(), 1)`. Pins the H3 /
  H4 surface so a future regression cannot reintroduce the same
  shape.

- **`tests/property_pow_specials.rs`** — table-driven enumeration
  of IEEE 754-2019 §9.2.1's full pow rule grid: every special-input
  pair (±0, ±1, ±2, ±0.5, ±∞, qNaN, sNaN) crossed with every
  rounding mode. The no-panic guard would have caught H1 on first
  run.

- **`tests/property_fma_oracle.rs`** — random `(a, b, c)` triples
  cross-checked against astro-float at 220-bit precision and
  against the trivial `mul`-then-`add` formulation when both stages
  are exact. Pins the H5 reproducer alongside the random fuzzing.
  Closes the FMA coverage gap the audit-leg flagged (FMA had only
  decTest signal — no proptest, no Kani, no fuzz).

- **`src/verify/pow.rs`** — Kani harnesses for `pow(±1, ±∞) = 1`,
  `pow(x, ±0) = 1` (excluding sNaN), and totality of `pow` over
  the special-input pool. Same special-only-shim convention as
  the existing arithmetic harnesses.

- **`src/verify/nan_payload.rs`** — Kani harnesses proving NaN
  payload propagation through `add` and `mul` over symbolic 8-bit
  payloads. Width is bounded for CBMC budget reasons; the
  propagation path is uniform on payload width.

## [1.12.0] - 2026-05-08

### Added

- **DPD (Densely Packed Decimal) interchange** behind a new opt-in
  `dpd` cargo feature. `Decimal128::to_dpd_bytes(self) -> [u8; 16]`
  and `Decimal128::from_dpd_bytes(bytes: [u8; 16]) -> Self` round-trip
  IEEE 754:2019 DPD byte patterns (the encoding IBM decNumber,
  z/Architecture decimal-FP hardware, and IBM POWER use as their
  wire format). Storage and arithmetic stay BID; the codec is a
  byte-level adapter, not a parallel arithmetic stack. Closes the
  interop gap [ADR-0001](docs/decisions/0001-bid-over-dpd.md) named
  in 2026-05-02; rationale, scope, and the rejected deeper
  alternatives live in
  [ADR-0009](docs/decisions/0009-dpd-interchange.md). Both directions
  are total — every `Decimal128` produces a 16-byte pattern and every
  16-byte pattern decodes to *some* valid `Decimal128`, with
  IEEE 754:2019 §3.5.2 canonicalization on read for non-canonical
  declets.

  Implementation: pure boolean equations from Mike Cowlishaw's
  *A Summary of Densely Packed Decimal Encoding*, transcribed
  verbatim so the codec audits line-by-line against the spec text.
  No lookup tables.

  Embedded code-size cost: **+7 KB `.text` on `thumbv6m-none-eabi`**
  with `--features=fmt,dpd`, measured against `--features=fmt` alone.
  Users who don't enable the feature pay zero — the entire module is
  `#[cfg(feature = "dpd")] mod dpd;` in `lib.rs`. Cost is dominated
  by 11 iterations of `u128 % 1000` / `u128 / 1000` on a target
  without hardware divide; intrinsic to the algorithm shape on
  Cortex-M0+.

### Conformance

- With `--features=dpd`, the runner climbs from **8 622 / 0 / 99**
  to **9 080 / 0 / 253** across 9 333 cases. The two newly vendored
  files cover encoding round-trips (`dqEncode.decTest`: 368 / 0 / 0)
  and canonical-form predicates (`dqCanonical.decTest`: 90 / 0 / 154,
  every dispatched op passes; the 154 skips are GDA bit-level
  operations — `copy`, `invert`, `and` / `or` / `xor`, `rotate`,
  `shift`, `comparesig` — outside ferrodec's surface).

  Without `--features=dpd` the existing 8 622 / 0 / 99 baseline is
  preserved exactly: the new files are skipped at the file gate.

### Verification

- Three new Kani harnesses under `src/verify/dpd.rs` (gated on
  `--features=kani --features=dpd`):

  * `declet_decode_total` — every 10-bit declet decodes to three
    valid BCD digits.
  * `from_dpd_bytes_total` — every 128-bit input produces a
    well-classified `Decimal128`. The panic-freedom proof for the
    codec.
  * `dpd_roundtrip_specials` — `INFINITY` / `NEG_INFINITY` / `NAN` /
    `SIGNALING_NAN` round-trip bit-equal.

  Aggregate Kani full-suite delta: +12 s. The "≈ 2 min full-suite"
  promise from [ADR-0008](docs/decisions/0008-perf-results.md) holds.

  A fourth harness (`dpd_roundtrip_via_try_new`, full canonical-finite
  round-trip) was drafted, ran for 10+ minutes without termination,
  and was dropped per the plan's stop-loss. Round-trip is covered by
  `tests/property_dpd.rs` instead. ADR-0009 records the omission.

## [1.11.0] - 2026-05-07

### Performance

- **Perf pass on the arithmetic kernels** delivers ~17 % aggregate
  speedup across the `core_ops` bench suite, with the headline
  operations seeing 23–27 % wall-time reductions vs 1.10.1.
  Methodology and per-candidate notes live in
  [`docs/decisions/0008-perf-results.md`](docs/decisions/0008-perf-results.md);
  the pre-pass baseline (commit `18bd5f7`) lives in
  [`docs/decisions/0007-perf-baseline.md`](docs/decisions/0007-perf-baseline.md).

  Per-bench cumulative delta vs the 1.10.1 baseline (Apple Silicon,
  rustc 1.95.0, release profile):

  | Bench                  | Before    | After    | Δ          |
  |------------------------|----------:|---------:|-----------:|
  | `add`                  |  7.98 µs  |  5.79 µs |  −27.5 %   |
  | `sub`                  | 39.92 µs  | 30.53 µs |  −23.5 %   |
  | `mul`                  | 42.45 µs  | 31.36 µs |  −26.1 %   |
  | `div`                  | 50.69 µs  | 44.78 µs |  −11.7 %   |
  | `fma`                  |   488 µs  |  415 µs  |  −14.9 %   |
  | `sub_alignment_heavy`  |  7.98 µs  |  5.80 µs |  −27.3 %   |
  | `mul_full_precision`   |  6.60 µs  |  5.23 µs |  −20.8 %   |
  | `parse_str`            |  3.68 µs  |  2.91 µs |  −20.9 %   |
  | `from_i128`            |  2.98 µs  |  2.24 µs |  −24.8 %   |

  The two load-bearing changes:
  * `round_and_pack_finite` now caches `decimal_digit_count` once per
    call instead of recomputing it 3× across the rounding /
    overflow-check / preferred-quantum branches. `decimal_digit_count`
    walks the U256 coefficient via `div_rem10`, so removing two of the
    three calls is the bulk of the aggregate uplift (commit
    `15a7b98`).
  * `U256::mul_pow10` looks up `10^k` from a precomputed `[u128; 39]`
    table instead of running an iterative `mul10` loop. Hot
    consumers: alignment shifts in `addsub`, the rounding pipeline's
    overflow-renormalize step, the up-renormalize in `finalize_finite`
    (commit `a53ddb4`).

  A third commit (`84e4598`) unified two duplicated digit-extraction
  loops in the rounding path, picking up `mul −3.2 %`. Three other
  optimization candidates were tested and reverted as no-op or
  noise-floor — see ADR-0008 for the audit log.

### Added

- **Bench coverage** for shapes the 1.10.x suite didn't cover:
  alignment-heavy add/sub, full-precision mul, magnitude-extreme div
  (`benches/core_ops.rs`); `partial_cmp` / `total_cmp` /
  `compare_total_magnitude` / 64-element sort
  (new `benches/comparison.rs`); `from_i32` / `from_u32` /
  `from_u64` / `to_i32` / `to_u64` / `to_u128` (`benches/conversions.rs`).
  These were added during the perf pass to expose specific hot paths
  but stick around as permanent regression-watching shapes.

- **Architecture Decision Records** under `docs/decisions/`. Eight
  ADRs (0001–0008) backfill the design log: BID over DPD, per-op
  status threading, method-only API, the Verus pilot outcome, the
  will-not-fix non-IEEE rounding directives, the deferred / executed
  perf pass, and the perf-baseline + results pair. Approved plans
  archive under `docs/decisions/plans/`. Future significant decisions
  drop into the same structure rather than getting lost in commit
  messages.

## [1.10.1] - 2026-05-06

### Changed

- **Conformance runner** now closes the bare `#` "null operand"
  category. The runner short-circuits any case carrying a bare
  `#` operand (the dec-spec sentinel for a missing/unparseable
  input) to the dec-spec answer `(NaN, Invalid_operation)` before
  invoking the op kernel. The 1.7.1 misestimate of "~13 + ~15
  misc" turned out to be 28 bare-`#` cases plus 2 that were also
  under non-IEEE rounding directives, so the close picks up 30
  cases.

  Implementation lives in a new `run_null_test` helper called
  from `run_case` ahead of the rounding / `invoke` dispatch; the
  earlier "skip on `parse_value` returning `None` for `#`" path
  now never trips for cases the spec covers.

### Conformance

- Suite total climbs to **8 622 / 0 / 99** across 8 721 cases
  (was 8 592 / 0 / 129). PASS_FLOOR raised 8592 → 8622. All 99
  residual skips fall under a single will-not-fix category
  (non-IEEE rounding directives `half_down` / `05up`); every
  other operation, encoding, and special-value combination in
  the suite passes.

  `KNOWN_ISSUES.md` rewritten to reflect the new state — five of
  the original six skip categories plus the `remainder` op are
  closed across the 1.9.0 → 1.10.1 trail.

## [1.10.0] - 2026-05-06

### Added

- **`Decimal128::rem_trunc(self, rhs)`** — truncating-quotient
  remainder: `r = self − trunc(self / rhs) · rhs`. The integer
  quotient rounds toward zero, so the result has the sign of
  `self` and magnitude `< |rhs|`. Matches C99 `fmod` semantics
  and decTest's `remainder` op (distinct from decTest's
  `remaindernear` and ferrodec's existing `Decimal128::rem`,
  which both implement the IEEE 754 §5.3.1 round-half-to-even
  remainder). Always exact when defined; never raises `INEXACT`.
  Three unit tests in `src/ops/rem.rs::tests` pin the
  basic-in-range, sign, and special-case behavior, plus a
  side-by-side comparison with `rem` at the half-quotient
  boundary where the two functions diverge.

### Changed

- **Conformance runner** routes the previously-skipped
  `remainder` op through `rem_trunc`. Suite total climbs to
  8 592 / 0 / 129 across 8 721 cases. PASS_FLOOR raised
  8591 → 8592.

### Out of scope this release

- A speculative perf pass was scoped out: meaningful optimization
  needs profiling against a real workload, and ferrodec's
  criterion benches are calibrated as regression guards rather
  than as optimization targets. A future release with concrete
  hot-path data can revisit.

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
