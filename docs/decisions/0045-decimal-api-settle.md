# ADR-0045: ferrodec-decimal public API settle (1.0)

- **Status**: accepted
- **Date**: 2026-06-03

## Context

ADR-0038 set four gates between `ferrodec-decimal` 0.x and 1.0: arbitrary
precision transcendentals (ADR-0040), full general decTest conformance
(ADR-0039), a settled public API, and a performance pass (ADR-0043/0044). The
transcendentals, the conformance, and the performance pass are done. This ADR
settles the remaining gate: it records the public API as stable for a 1.0
commitment and names the deliberate divergences a maintainer would otherwise
rediscover.

The surface grew across 0.1.0 to 0.3.0 to the whole General Decimal Arithmetic
operation set, so the open question for 1.0 was never coverage. It was whether
the shape of the surface is the one to commit to, given it diverges in places
from the fixed-width siblings (`ferrodec`, `ferrodec-decimal64`,
`ferrodec-decimal32`).

## Decision

The public API is settled as it stands at 0.3.0. The choices below are
deliberate and are what 1.0 commits to.

**General Decimal Arithmetic method names, not the siblings' abbreviations.**
This crate spells every operation after the specification, rendered in snake
case: `divide`, `multiply`, `subtract`, `divide_integer`, `remainder` /
`remainder_near`, the logical `and` / `or` / `xor` / `invert`, `next_plus` /
`next_minus` / `next_toward`, `compare_total`, `same_quantum`, and the rest. The
fixed-width siblings use shorter, operator-aligned names (`div`, `mul`, `sub`,
`rem_near` / `rem_trunc`, `logical_and`, `next_up` / `next_down`) because they
carry an `ops` feature with `core::ops` operator overloads (ADR-0003), where
`div` maps to `Div`. This crate has no such feature and no operator overloads,
and its spec authority is General Decimal Arithmetic, so the spec spelling is the
right surface and is internally consistent. ADR-0027 and ADR-0029 settled the
siblings' names for the siblings' reasons (operator traits, cross-sibling `%`
consistency); those reasons do not bind the arbitrary precision crate. The
divergence is intentional and recorded here rather than reconciled away.

**Context by reference, status by return.** Each operation takes a `Context`
(precision, exponent bounds, rounding, clamp) by reference and returns
`(Decimal, Status)`, never touching global state (ADR-0002, ADR-0003). The fixed
formats take only a `RoundingMode` because their precision and exponent range are
fixed; this crate needs the full context because they are not.

**Two rounding-mode types, by design.** `Context` carries the eight General
Decimal Arithmetic modes as a crate-local `Rounding` enum; the fixed formats keep
`ferrodec_ieee::RoundingMode` at the five IEEE directions (ADR-0005). The
`interop` narrowing conversions take `RoundingMode`, which makes narrowing into a
fixed format under a non-IEEE mode unrepresentable. The split stays.

**Smaller settled points.**

- `Decimal` implements `PartialEq` / `Eq` (representation equality: `1.0` and
  `1.00` differ) but deliberately not `PartialOrd` / `Ord`, so `<` / `>` cannot
  be mistaken for numeric comparison; that is `compare` / `compare_total`. The
  fixed formats implement the ordering traits; this divergence is intentional.
- `ParseDecimalError` stays `#[non_exhaustive]`, reserving room for a future
  parse condition without a breaking change.
- The feature set is final: `fmt` (default; parse and the to-scientific /
  to-engineering string forms), `binary-float` (lossless `TryFrom<f64>` /
  `<f32>`), `interop` (the fixed-format bridges), and the local-only
  `differential` test feature.

## Consequences

With this gate met, all four ADR-0038 prerequisites are satisfied, so
`ferrodec-decimal` is released as 1.0.0. The surface becomes a SemVer-stable
commitment, and the divergences from the fixed-width family are named here so a
maintainer reads them as decisions rather than as drift, consistent with the
family's posture that every divergence is recorded in an ADR.

The remaining performance candidates (Newton reciprocal division, `exp`-series
splitting, constant-series splitting) are filed follow-ups and are performance
only, not API, so they do not bear on the 1.0 surface and can land as patch
releases.

## Related

- The 1.0 gates: [ADR-0038](0038-arbitrary-precision-decimal.md).
- The other gates, now met: [ADR-0039](0039-general-dectest-conformance.md),
  [ADR-0040](0040-arbitrary-precision-transcendentals.md),
  [ADR-0043](0043-decbig-perf-baseline.md),
  [ADR-0044](0044-decbig-perf-pass-results.md).
- Divergence precedents: [ADR-0002](0002-per-op-status.md),
  [ADR-0003](0003-method-only-api.md), [ADR-0005](0005-half-down-05up-wontfix.md),
  [ADR-0027](0027-rem-semantic-asymmetry.md),
  [ADR-0029](0029-ferrodec-2-0-breaking-change-plan.md).
