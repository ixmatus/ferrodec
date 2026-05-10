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
