# ADR-0011: Cargo workspace for sibling decimal-precision crates

- **Status**: accepted
- **Date**: 2026-05-09

## Context

The next priority on the project roadmap is to add `ferrodec-decimal32`
and `ferrodec-decimal64` (IEEE 754-2019 Decimal32 and Decimal64) as
companions to ferrodec's Decimal128. The exploration in
`docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`
established three facts that shape the layout decision:

- Each precision is a complete crate with its own arithmetic, encoding,
  conformance vectors, fuzz targets, and Kani harnesses. They share the
  IEEE 754 metadata types (`Status`, `RoundingMode`, `IeeeClass`), the
  rounding shape, and the test/CI conventions, but the implementations
  do not parameterize cleanly: Decimal128 needs multiword arithmetic
  while Decimal32 / Decimal64 fit in single-word `u32` / `u64`, and the
  transcendental working precisions diverge proportionally.
- "Stand alone first; resist framework abstraction until at least three
  concrete uses exist" rules out a parameterized `Decimal<P>` and a
  premature shared-core extraction. Each sibling lives as its own crate
  with its own published version.
- ferrodec is at v1.14.3 on crates.io. Any restructuring must preserve
  the published name, the source paths users link to, and the existing
  development ergonomics (cargo invocations, IDE configs, ADR
  filepaths, CI job paths).

The choice is between a flat workspace (each crate a top-level directory
in the repo) and a `crates/` subdirectory layout where every member sits
under one umbrella folder.

## Decision

Convert ferrodec into a Cargo workspace with the **flat** layout:
ferrodec stays at the repo root with a dual-purpose `Cargo.toml` that
carries both a `[workspace]` table and the existing `[package]` table.
Sibling crates land as flat top-level directories alongside (e.g.,
`ferrodec-decimal32/`, `ferrodec-decimal64/`).

The first commit of this conversion adds:

```toml
[workspace]
members = ["."]
resolver = "2"
```

at the top of the existing `Cargo.toml`. Subsequent commits hoist the
shared lints into `[workspace.lints]` (ADR follow-up if the lint shape
changes materially) and shared package metadata into `[workspace.package]`.
The published crate's name, version, and feature surface are unchanged
in this conversion.

## Consequences

**Wins.**

- Zero churn to ferrodec's existing source paths, blame, links from
  external docs, IDE configs, and on-disk plans. `git log --follow`
  and source-link permalinks keep working without redirection.
- Matches the established Rust precedent at this scale. `tokio`,
  `axum`, and `serde` keep flat layouts at four-to-six member crates.
  The `crates/` umbrella pays off above roughly ten members (`bevy`,
  `wasmtime`); ferrodec's projected family of three to four does not
  cross that threshold.
- Adding a sibling crate becomes a directory plus a thirty-line
  `Cargo.toml` that inherits lints, edition, MSRV, license, and
  repository from `[workspace.package]` once the hoisting commits
  land.

**Costs.**

- The root `Cargo.toml` is now dual-purpose, holding both `[workspace]`
  and `[package]` sections. This pattern is documented and widely used,
  but it asks the reader to recognize that the workspace and the root
  crate are co-located. A passing reader who expects `[workspace]` at
  root and members under `crates/` may be momentarily disoriented.
- The workspace `resolver = "2"` is required explicitly: workspaces
  default to resolver "1" regardless of member editions, while the
  ferrodec package on `edition = "2021"` was already on resolver "2"
  implicitly. Setting it in `[workspace]` preserves current
  feature-unification behavior. Failing to set it would silently
  downgrade resolution and risk feature-flag surprises in CI.
- A future inflection point (a fifth or sixth crate joining the
  family) may justify revisiting this decision and migrating to
  `crates/`. That migration is mechanical and independent of any
  source code; the cost of deferring it is one future ADR.

## Related

- Plan: `plans/2026-05-09-workspace-and-decimal-siblings.md`
- Other ADRs: ADR-0001 (BID over DPD) and ADR-0002 (per-op Status)
  describe Decimal128-specific design choices that the sibling crates
  inherit by default; this ADR sets the structural frame within which
  those inheritances apply.
