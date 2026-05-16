# ADR-0019: decimal32 correctness train closing

- **Status**: accepted
- **Date**: 2026-05-16

## Context

ADR-0017 and ADR-0018 deferred decimal32 to its own correctness
slice after the decimal64 train, on the reasoning that decimal32's
`KNOWN_ISSUES.md` was mostly coverage gap with no confirmed
correctness defect: the conformance dispatcher had only ever
exercised `tosci` and `apply`, so nothing had probed the arithmetic
paths. The decimal64 slice (ADR-0018) closed its H tier and reached
full `dd*` conformance, which made a new oracle available: every
finite `Decimal32` is exactly representable in `Decimal64`, so the
now conformance validated `Decimal64` operation, rounded back to
seven digits, is an exact check on the `Decimal32` operation for
`mul` and `rem` and a strong screen for `add`, `sub`, and `div`.

This ADR records the decimal32 slice that ran to completion and
ships as `ferrodec-decimal32` 1.4.0.

## Decision

1. **Cross-check oracle, then a six agent review.** A `Decimal64`
   cross-check harness was stood up first. It immediately refuted
   the planning era hypothesis: the static alignment windows in
   `addsub.rs` and `rem.rs` are not sufficient for the narrow
   format. A six agent review (ADR-0010 methodology) over the whole
   decimal32 op surface, under the provenance and security
   discipline, produced eight H, six M, and four L findings.

2. **H tier closed.** H1 the addsub static window and asymmetric
   zero magnitude loss (one root, fixed with a dynamic per side
   shift over a u128 register mirroring the in crate `fma.rs`, plus
   an explicit zero operand fast path); H2 the `rem` static
   `MAX_SAFE_SHIFT`; H3 the typed `BiasedExp` and `Coefficient`
   newtypes that lift the `pack_finite` preconditions into the type
   system plus the FMA preferred quantum clamp; H4 the FMA effective
   subtract borrow and extend; H5 the quantize zero short circuit;
   H6 the breaking `to_f64` signature; H7 the inherent `to_f32`;
   H8 the `parse_str` adversarial counter cap, a security fix. H1
   through H5 mirror the decimal64 fd-d47, H5, H3, fd-d47 mirror,
   and H6 shapes; H6 through H8 mirror the decimal64 H7, M4, and H8
   shapes.

3. **Two findings were evidence first, not assumed.** The fd-d47
   power of ten regime was audited explicitly rather than assumed:
   targeted probes proved decimal32's addsub borrow and extend has
   an `is_power_of_10` branch that handles the leading digit drop
   correctly, so H1 carries no fd-d47 residual, and the probes are
   now permanent guards. The H2 `rem` pin from Phase 0 was found to
   be spec correct, not a defect: its integer quotient has about
   sixteen digits, so the `Invalid_operation` is GDA correct, and
   the cross-check oracle had been unsound for `rem` when the
   quotient has eight to sixteen digits. The oracle was corrected to
   the GDA rule, then a sound witness (`rem(1E+13, 9999999)`)
   confirmed the genuine static window defect, which was then fixed.
   This is the conformance and oracle machinery working as designed,
   the same lesson ADR-0018 recorded for decimal64 fd-d47.

4. **M and L tiers closed.** The conversion correctness set (M2
   `scaleb` envelope, M3 `from_f64` signaling bit, M4 the exact
   integer conversion surface), the five Kani special case shim
   groups (M5: `exp`/`ln`, `trig`, `hyper`, `pow`/`cbrt`, the
   quantum family, ported under ADR-0016 with no CBMC budget skip,
   bringing decimal32 proof coverage level with decimal64), the
   cross-check formalized as the permanent net plus a round-trip
   guard (M6, L2), and the drift cleanup (L1 zero engineering
   rendering, L3 audited safe invariants, L4 the rem proof note).

5. **DPD interchange added (N+1).** The Phase 0 premise that
   `dsEncode.decTest` was BID was wrong: the file header reads
   "Selected DPD codes". Rather than defer, a `dpd` gated DPD
   interchange codec was built: the format independent declet
   primitive is pure IEEE 754-2008 §3.5.2 boolean equations with no
   lookup tables, and `Decimal32::to_dpd_bytes` / `from_dpd_bytes`
   carry the 32-bit interchange framing. BID stays the arithmetic
   storage encoding (ADR-0001); DPD is a byte level adapter, off by
   default, `no_std` clean (ADR-0009 posture, now applied to
   decimal32). With `dpd` on, `dsEncode` passes 250 of 268 (up from
   2), `dsBase` unchanged at 698, the per file counts exact match
   and feature conditional (ADR-0010).

6. **Minor version, breaking note.** `to_f64`'s signature change is
   a hard break on a 1.x crate. The release is 1.4.0, a minor bump
   with the break called out prominently in the CHANGELOG, mirroring
   the deliberate decimal64 1.4.0 decision (ADR-0018): the honest
   signature is worth the churn, the honest accounting lives in the
   CHANGELOG.

## Consequences

**Wins.**

- `Decimal32` now conforms on the arithmetic paths the cross-check
  exercises (add, subtract, multiply, divide, and the GDA correct
  remainder, seven blocks, zero ignored), the H3 invariant is in the
  type system rather than a release elided debug assertion, the
  transcendental and quantum clusters have Kani special case proofs
  level with decimal64, and DPD interchange unlocks 248 more
  conformance cases.
- The slice carried the decimal64 lessons forward concretely: the
  fd-d47 power of ten regime was checked with evidence and found
  sound; the rem oracle unsoundness was caught and corrected before
  it could mask or fabricate a defect.

**Costs.**

- `to_f64`'s breaking signature forces a one line migration on every
  caller. Called out in the 1.4.0 CHANGELOG.
- 18 `dsEncode` and a set of `dsBase` cases still skip: `parse_str`
  does not apply the IEEE 754-2019 §7.4 preferred exponent clamp.
  This is a documented cross crate quantization policy decision, not
  a codec or arithmetic defect.
- Transcendentals still route through `f64` / `libm` (faithfully
  rounded, not correctly rounded), the documented v1.0 baseline; the
  `Extended` kernel route is a 1.16 era follow up.

**Drift.**

- `ferrodec-decimal32/KNOWN_ISSUES.md` is the source of truth for
  the closed audit trail and the two remaining documented
  limitations.
- A correctness defect in the released parent `ferrodec` 1.15.0
  Decimal128 FMA (`property_fma_oracle`) was discovered incidentally
  during this slice's workspace test runs, at large biased
  exponents. It is not caused by the decimal32 work (the parent
  crate was untouched) and is explicitly out of scope here. It is
  filed for separate triage; the decimal32 per commit gate was
  scoped to tolerate that single known parent failure while keeping
  every decimal32 and other workspace suite green.

## Related

- ADR-0001: BID over DPD (DPD stays a byte level adapter).
- ADR-0009: DPD interchange behind the `dpd` feature (the posture
  applied to decimal32 here).
- ADR-0010: testing strategy after the six agent review (the
  methodology applied here).
- ADR-0016: Kani harness shim routing (the rule for the M5 port).
- ADR-0017 / ADR-0018: the decimal64 conformance gap and the
  decimal64 train, which deferred decimal32 to this slice.
- `ferrodec-decimal32/CHANGELOG.md`: the 1.4.0 entry.
- `ferrodec-decimal32/KNOWN_ISSUES.md`: the closed audit trail and
  the remaining limitations.
- Plan and findings: the decimal32 correctness slice plan and the
  six agent findings document archived under the decision records.
