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
