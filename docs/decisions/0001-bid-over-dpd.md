# ADR-0001: BID-128 over DPD-128

- **Status**: accepted
- **Date**: 2025-09-01

## Context

IEEE 754:2019 defines two 128-bit decimal interchange formats: BID (Binary Integer Decimal) and DPD (Densely Packed Decimal). They round-trip into the same set of values; the difference is how the coefficient packs into the 128-bit container. BID stores the coefficient as a binary integer in the trailing significand field; DPD packs it 10 bits per 3 decimal digits ("declets").

Library implementations historically split: Intel's reference uses BID, IBM's uses DPD. Both are valid IEEE encodings. ferrodec had to pick one for its `Decimal128` newtype and arithmetic kernels.

## Decision

Use BID-128 exclusively. The `Decimal128` value is a `repr(transparent)` `u128` whose bits follow the BID layout. There is no DPD support and no plan to add one.

## Consequences

**Wins:**

- All arithmetic kernels are pure binary integer operations on `u128` (and the U256/U384/U512 multiword primitives in `src/multiword/`). No declet pack/unpack tables, no 10-bit-aligned bit-twiddling.
- The 113-bit coefficient field gives a clean `u128` envelope for the integer coefficient, simplifying every overflow check in the rounding pipeline.
- Embedded targets (the original goal: STM32U-class Cortex-M0+) avoid lookup tables that DPD would require for every digit-level operation. Code size on `thumbv6m-none-eabi` stays under 50 KB at the `--features=fmt` floor.

**Costs:**

- Interop with libraries that prefer DPD requires byte-pattern conversion. ferrodec doesn't ship that adapter; users wanting cross-format exchange handle it externally.
- Some IEEE 754 spec language is easier to read in DPD terms (the "declet" abstraction). The BID arithmetic is more direct in code but less directly mappable to spec text in some places.

**Why this isn't reconsidered:**

DPD wins for human-readable bit patterns and per-digit operations (each declet stores 3 decimal digits). ferrodec's API never exposes the bit pattern at the digit level — `Decimal128` is treated as a numeric value. The BID benefit (clean integer arithmetic) is realized at every arithmetic call; the DPD benefit (per-digit access) would only matter at parse / display, which run rarely. The trade favors BID for ferrodec's workload shape.

## Related

- `src/bid.rs` — encoding constants and pack / classify helpers.
- `docs/PLAN.md` — original v1 design doc records this as a fixed early choice.

## Update (2026-05-08)

The "ferrodec doesn't ship that adapter; users wanting cross-format exchange handle it externally" sentence in *Consequences → Costs* is **superseded by [ADR-0009](0009-dpd-interchange.md)**. ferrodec now ships `Decimal128::to_dpd_bytes` / `from_dpd_bytes` behind the opt-in `dpd` cargo feature for IEEE 754-2019 DPD byte-pattern interchange.

The core decision in this ADR — BID-128 as the storage and arithmetic encoding — is unchanged. ADR-0009 is a byte-level adapter on top, not a re-litigation. The "Why this isn't reconsidered" paragraph still applies: arithmetic stays BID, the codec is feature-gated so the embedded floor pays nothing for it by default, and the deeper alternatives (DPD-as-storage, parallel `Decimal128Dpd` newtype) were re-examined in the ADR-0009 plan and rejected.
