# ADR-0036: Decimal128 from_f64 / from_f32 signature unification (breaking)

- **Status**: accepted
- **Date**: 2026-05-29

## Context

The binary-float conversion constructors diverge across the three
formats. The siblings expose

```rust
Decimal64::from_f64(x: f64, rm: RoundingMode) -> (Decimal64, Status)
```

which gives the caller rounding control and reports the IEEE 754
exception status (chiefly INEXACT, the common case for a binary double
that has no exact decimal form). The parent exposes the weaker

```rust
Decimal128::from_f64(value: f64) -> Decimal128
```

which hardcodes round-to-nearest-even and discards the status, and
`from_f32(value: f32) -> Decimal128` likewise. A caller porting between
the formats hits the divergence immediately, and on the parent has no
way to choose a rounding direction or learn that the conversion was
inexact.

This is the API-symmetry item from the 2026-05-29 review (the parity
train, ADR-0035). Unlike the other parity fixes, converging the
signatures is a breaking change to a shipped 2.x surface.

## Decision

Converge the parent on the siblings' richer signature:

```rust
Decimal128::from_f64(value: f64, rm: RoundingMode) -> (Decimal128, Status)
Decimal128::from_f32(value: f32, rm: RoundingMode) -> (Decimal128, Status)
```

`rm` drives the decimal-string round-trip's rounding; the returned
status carries INEXACT (and any overflow/underflow at the format
edges). NaN / ±∞ / ±0 still pass through, now paired with `Status::OK`.

Downstream adjustments on the parent:

- `TryFrom<f64>` / `TryFrom<f32>` keep their infallible-after-finite
  contract and pick `RoundingMode::NearestEven`, discarding the status
  (matching the sibling `TryFrom` impls), so the trait surface is
  unchanged.
- `FromPrimitive::from_f64` / `from_f32` delegate to the new inherent
  signature with `NearestEven` and drop the status, as before.

This is a **major-version** change for the `ferrodec` crate: it lands
with the rest of the ADR-0035 parity train, which also tightens
observable NaN sign/payload and zero `Display` behavior, so the release
that carries this branch is ferrodec 3.0. The version bump itself is
left to release engineering; this ADR records that the bump is required.

## Consequences

The three formats now share one binary-float conversion shape, so
generic code and cross-format ports stop special-casing the parent, and
parent callers gain rounding control and the INEXACT signal the siblings
already provided. The cost is the break: every caller of the parent
`from_f64` / `from_f32` that wanted the bare value must add the
rounding-mode argument and take `.0`. The `TryFrom` and `FromPrimitive`
paths absorb the change internally, so the common conversion entry
points are unaffected in behavior.

Provenance is clean: the new signature is the siblings' existing,
tested shape; the parent body keeps its decimal-string round-trip and
only threads `rm` and the status through.

## Related

- Parity train: ADR-0035 (this is its one hard-breaking API item).
- Other ADRs: ADR-0029 (the 2.0 breaking-change set, the prior major
  cycle); ADR-0014 (Display harmonization, also observable on this
  branch).
- Issues: `fd-92w.11` (API symmetry).
