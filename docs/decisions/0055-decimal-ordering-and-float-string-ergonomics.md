# ADR-0055: ferrodec-decimal ordering, FromStr, and to_f64 (the 2.0 surface)

- **Status**: accepted
- **Date**: 2026-06-28

## Context

ADR-0045 settled the `ferrodec-decimal` public surface at 1.0 and made a
deliberate choice: `Decimal` would *not* implement `PartialOrd` / `Ord`, so that
`<` / `>` could never be mistaken for a numeric comparison (numeric ordering
being `compare` / `compare_total`). A downstream consumer (`metrology`) then
surfaced a small set of ergonomic gaps. It constructs high-precision decimal
constants from `&'static str` tables, verifies that a stored `Decimal128` is the
`NearestEven` rounding of the high-precision value, and orders, compares, and
snapshots those values in tests (it does no arithmetic on the type). It needs:
sortability and `BTreeSet` / `BTreeMap` keying (`Ord`), `"...".parse::<Decimal>()`
(`FromStr`), and a `to_f64` read-out for snapshotting.

The first of these reopens the ADR-0045 decision, and implementing it revealed a
hard constraint the original "just add `Ord`" plan missed:

- **`Ord` cannot coexist with the frozen GDA `max` / `min`.** The 1.0 surface has
  inherent context-aware `max(&self, other, ctx) -> (Decimal, Status)` and
  `min(...)` (the spec's maxnum/minnum). `Ord` carries *provided* methods
  `max(self, other) -> Self` and `min(self, other) -> Self`. For a value
  receiver, Rust method resolution selects `Ord::max` at the by-value step before
  it reaches the inherent `&self` method, so every `d.max(other, ctx)` on a value
  resolves to `Ord::max` and fails to compile. This breaks the conformance and
  differential harnesses and, more importantly, any downstream value-receiver
  call. Adding `Ord` is therefore a *breaking* change, not an additive one.

- **The ordering must be the `totalOrder`, not numeric.** `Eq` on `Decimal` is
  structural (`1.0 != 1.00`, `-0 != +0`). `Ord` / `PartialOrd` are required to be
  consistent with it (`a == b ⟺ partial_cmp == Some(Equal)`). A numeric order
  would report `1.0 == 1.00`, violating that law. Only the IEEE 754 `totalOrder`
  (which returns `Equal` exactly on structural identity) is lawful. So the
  ADR-0045 worry ("`<` might be read as numeric") is unavoidable in a different
  form: any lawful `<` here is the total order, not the numeric one.

Given that adding `Ord` is breaking regardless, the change is taken as a major
bump to 2.0, and this ADR supersedes ADR-0045.

## Decision

**Implement `Ord` / `PartialOrd` for `Decimal` as the IEEE 754 `totalOrder`**,
both delegating to the existing private `total_cmp` (the relation
`compare_total` exposes). Not derived: a derived impl would compare the `Repr`
fields lexicographically, which is numerically meaningless. Lawful against the
structural `Eq` as argued above; total over every value, so it never panics;
gateless, because `total_cmp` uses only always-available `DecBig` operations.

**Rename the GDA `max` / `min` to `maxnum` / `minnum`.** This resolves the
`Ord::max` / `Ord::min` collision and aligns with IEEE 754-2019, which renamed
these operations to `maximumNumber` / `minimumNumber`. `max_magnitude` /
`min_magnitude` are unaffected (no `Ord` provided method shadows them) and keep
their names. The decTest dispatch keys stay `"max"` / `"min"` (spec operation
names); only the Rust methods change.

**Add `FromStr`**, `type Err = ParseDecimalError`, delegating verbatim to
`Decimal::parse_str` (exact, non-rounding; inherits the error type). Lives under
the existing `fmt`-gated `convert::parse` module.

**Add `to_f64(&self, RoundingMode) -> (f64, Status)`** under `binary-float`,
mirroring `Decimal128::to_f64`: NaN / ±∞ / ±0 pass through bit-exactly (sNaN
raises `INVALID` and yields a quiet NaN per IEEE 754-2019 §5.4.2 `convertFormat`,
qNaN quietly); a finite value is rendered to its exact decimal string via
`Display` and parsed once by `f64::from_str`. Rendering the exact decimal of
`self` (not an intermediate `Decimal128`) makes this a single
round-to-nearest-even step with no double-rounding error. `rm` informs only the
unrepresentable edges (overflow ⇒ ±∞ + `OVERFLOW | INEXACT`, underflow ⇒ ±0 +
`UNDERFLOW | INEXACT`); an in-range finite is reported `INEXACT` unconditionally,
matching the `Decimal128::to_f64` contract. Because this needs `Display`, the
`binary-float` feature now pulls `fmt` (as `interop` already does), and
`RoundingMode`, previously re-exported only under `interop`, is re-exported under
`any(interop, binary-float)`.

**Bump 1.0.1 → 2.0.0** and mark ADR-0045 superseded.

## Consequences

- **Breaking.** Downstream code calling `decimal.max(other, ctx)` /
  `decimal.min(...)` on a value must move to `maxnum` / `minnum`. This is the
  unavoidable cost of giving the type `Ord`; the name change is loud and the
  compiler error at the old call sites is immediate, not silent.
- metrology depends on `ferrodec-decimal` 2.0 with `["interop", "binary-float"]`
  and uses `ferrodec_decimal::Decimal` directly: `BTreeSet`-keyable, sortable,
  `parse`-able, and `to_f64`-convertible.
- `<` / `>` on `Decimal` is now the total order: it distinguishes cohorts and
  signed zeros and orders NaNs. This is the ADR-0045 hazard made concrete, but it
  is the only lawful choice, and it is documented at the impl, in the README
  design principles, and here. Numeric comparison remains one call away
  (`compare` / `compare_total`).
- `to_f64` is a convenience read-out, not the IEEE 754 §5.4.2 correctly-rounded
  `convertFormat`: it never double-rounds (single round from the exact decimal),
  but the unconditional `INEXACT` on in-range finites is a deliberate
  under-report of exactness, matching `Decimal128::to_f64`.
- `binary-float` now implies `fmt`. `fmt` is default-on, so the only build this
  changes is `--no-default-features --features binary-float`, which now also gets
  the string surface. Additive within the feature graph.
- No sibling workspace crate version-depends on `ferrodec-decimal`, so the major
  bump breaks nothing inside the workspace; the only affected consumer is the
  external `metrology`, which is moving to 2.0 deliberately.

## Related

- Plan: `plans/async-swinging-nygaard.md` (the approved plan assumed an additive
  1.1; the `Ord` / `max` collision discovered during implementation forced the
  2.0 reframing recorded here).
- Other ADRs: **supersedes ADR-0045** (the 1.0 settle and its no-ordering
  stance); sibling of ADR-0041 (the exact `from_float` conversion, the reverse of
  `to_f64`); mirrors the `Decimal128::to_f64` L5 contract note in the root
  crate's `src/convert/binary.rs`.
- Code: `ferrodec-decimal/src/compare.rs` (`Ord` / `PartialOrd`, `maxnum` /
  `minnum`), `ferrodec-decimal/src/convert/parse.rs` (`FromStr`),
  `ferrodec-decimal/src/to_float.rs` (`to_f64`).
