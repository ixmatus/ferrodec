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
- Shared IEEE 754 metadata types: `Status`, `RoundingMode`,
  `IeeeClass`. `Status` and `RoundingMode` are duplicated verbatim
  from ferrodec-decimal32 (the file is fully precision-agnostic).
  `IeeeClass` is adapted from ferrodec-decimal32: same enum shape
  with doc text retargeted from Decimal32 to Decimal64. Three
  consumers now exist (ferrodec, ferrodec-decimal32,
  ferrodec-decimal64); the shared `ferrodec-ieee` extraction lands
  in a follow-on Phase D commit.
