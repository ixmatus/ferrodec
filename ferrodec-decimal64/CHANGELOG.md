# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Skeleton crate. `Decimal64(u64)` type wrapper, no methods yet.
  Initial groundwork for the full Decimal64 implementation per the
  plan archived at
  `docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`.
  Inherits workspace lints, edition, MSRV (1.84), license, and
  repository metadata. `fmt` and `kani` features are declared with
  empty bodies for future use.
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
