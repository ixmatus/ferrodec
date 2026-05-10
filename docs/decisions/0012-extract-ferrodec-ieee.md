# ADR-0012: Extract `ferrodec-ieee` after three concrete consumers

- **Status**: accepted
- **Date**: 2026-05-10

## Context

The ferrodec family ships three precision-specific crates:

- `ferrodec` — Decimal128 (128-bit storage, 34 digits)
- `ferrodec-decimal32` — Decimal32 (32-bit storage, 7 digits)
- `ferrodec-decimal64` — Decimal64 (64-bit storage, 16 digits)

Three precision-agnostic types appear byte-identical (modulo doc
comments) in all three crates: `Status` (the IEEE 754-2019 §7
exception flag set), `RoundingMode` (the five §4.3.3 directions),
and `IeeeClass` (the §5.7.2 ten-class enum). When `ferrodec-decimal32`
landed it copy-pasted these from `ferrodec` and recorded the
duplication in a header comment naming the future extraction. When
`ferrodec-decimal64` landed it copied the same shape from
`ferrodec-decimal32`.

The plan archived at `plans/2026-05-09-workspace-and-decimal-siblings.md`
deferred the extraction explicitly until three concrete consumers
existed, honouring the principle "stand alone first; resist framework
abstraction until 3 concrete uses exist." All three siblings now exist
at v1.x, so the threshold is met.

A second motivation: cross-precision interop. Without a shared
crate, `ferrodec::Status` and `ferrodec_decimal32::Status` are
distinct concrete types — code that wants to merge a Decimal128
status flag with a Decimal32 one has to convert. With a shared crate
they're the *same* type and compose naturally.

## Decision

Extract `Status`, `RoundingMode`, and `IeeeClass` into a new
workspace member crate `ferrodec-ieee` at v0.1.0. Each sibling crate
replaces its local definitions with `pub use ferrodec_ieee::{Status,
RoundingMode, IeeeClass};` so its public API is byte-compatible with
the previous release.

Doc comments on `IeeeClass` are generalised away from precision-
specific phrasing ("Decimal32::MIN_POSITIVE_NORMAL", etc.) into
"the format's minimum positive normal magnitude," which reads
correctly for all three precisions.

Each sibling bumps its patch version to advertise the dependency
addition: `ferrodec` → 1.14.4, `ferrodec-decimal32` → 1.0.1,
`ferrodec-decimal64` → 1.0.1. (No public-API change beyond the
re-export; same type identities the previous releases offered.)

## Consequences

**Wins.**

- Cross-precision interop: code calling
  `ferrodec_decimal32::Status::OVERFLOW | ferrodec::Status::INEXACT`
  now compiles, because both names resolve to
  `ferrodec_ieee::Status`.
- Single source of truth for IEEE 754 metadata; no risk of subtle
  drift between sibling copies.
- Future siblings (a hypothetical Decimal16 or extensions) inherit
  the shared types without further duplication.

**Costs.**

- One additional dependency in each sibling's `Cargo.toml`. The
  shared crate is small (~270 LOC, no deps of its own) and
  `default-features = false` everywhere — minimal footprint impact.
- The sibling source trees lose a self-contained file. Readers
  tracing `Status` now jump across crate boundaries; the trade is
  worth it for the interop win.

**Non-consequences.**

- No change to any public API surface beyond name-equivalence.
  `Decimal128::partial_cmp(...)` still returns
  `(Option<Ordering>, Status)`; `Status` is just resolved through
  the new crate.
- Wire format unchanged: `Status` is `#[repr(transparent)] u8` in
  both old and new crates; serialised representations round-trip.

## Related

- Plan: `plans/2026-05-09-workspace-and-decimal-siblings.md` (Phase D, step D1).
- Principle source: `~/.claude/CLAUDE.md` ("stand alone first; resist
  framework abstraction until 3 concrete uses exist") and
  `~/Development/plant-flag/PRINCIPLES.md`.
- ADR-0011: workspace structure that made the in-tree extraction
  possible without breaking publication independence.
