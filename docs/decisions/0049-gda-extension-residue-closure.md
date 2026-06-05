# ADR-0049: Closing the GDA decNumber extension residue (compareSignaling, nextToward, Decimal64 DPD)

- **Status**: accepted
- **Date**: 2026-06-04

## Context

ADR-0031 implemented eight General Decimal Arithmetic decNumber
extension operations (`reduce`, `divide_integer`, `logical_invert`,
`logical_and`, `logical_or`, `logical_xor`, `shift`, `rotate`) on all
three fixed-format decimal types and named exactly three items that it
deferred, "not for reasons of cost; they are deferred because they are
different decisions": `compareSignaling`, `nextToward`, and a Decimal64
DPD codec. The backlog bead `fd-bef` carried them.

These three are the last GDA-extension residue on the conformance
dispatcher side. The copy-family closure (fd-37z), the §9.6 closure
(ADR-0028), and the eight-op closure (ADR-0031) had already left only
these three plus the ADR-0005 will-not-fix rounding directives as the
remaining `Skip` sources. The frugality argument is the canonical-name
argument from the project values: a user migrating from `decNumber` or
`java.math.BigDecimal` finds `compareSignaling` and `nextToward` absent
and Decimal64 unable to exchange the DPD byte pattern, and is forced
into a workaround.

This ADR is the same deliberate relitigation of the ADR-0029
IEEE-mandatory lens that ADR-0031 performed: it admits these additive,
non-breaking extensions in the 3.x line without amending the lens. The
release-engineering claim still rests on the §9.6-closed IEEE 754-2019
mandatory surface, not on GDA-extension completeness.

## Decision

Implement all three deferred items as additive 3.x surface.

1. **`compareSignaling`** on `Decimal128`, `Decimal64`, `Decimal32`:
   `compare_signaling(self, other) -> (Option<Ordering>, Status)`. It is
   the signaling counterpart of the quiet `partial_cmp`: the same
   numeric comparison, but *every* NaN operand raises `INVALID`, where
   `partial_cmp` raises only on a signaling NaN. The signature mirrors
   `partial_cmp` (the established fixed-format idiom) rather than the
   GDA number-returning shape, so there is no NaN-as-result to
   construct and `None` carries the incomparability.

2. **`nextToward`** on all three: `next_toward(self, other) -> (Self,
   Status)`. It steps `self` one representable place toward `other`,
   reusing each format's `next_up` / `next_down` for the cohort-correct
   one-ulp step (every fixed-format operand is representable, so no
   round-to-grid is needed) with direction chosen by the numeric
   `partial_cmp`. Unlike the one-argument neighbours, which never
   signal, the directed step carries the underflow / overflow / clamp
   flags: a subnormal result raises `UNDERFLOW | INEXACT`, an infinite
   result `OVERFLOW | INEXACT`, and a zero result (the step crossed the
   subnormal gap to a signed zero at the floor exponent Etiny)
   `UNDERFLOW | INEXACT | CLAMPED`. The precision-1 degenerate that
   required a special case in the arbitrary-precision sibling (Etiny
   equals Emin, where the zero carries no flag) is unreachable here: the
   fixed formats hardcode precision at 34 / 16 / 7, so Etiny is always
   below Emin.

3. **Decimal64 DPD codec** behind a new opt-in `dpd` cargo feature:
   `Decimal64::to_dpd_bytes(self) -> [u8; 8]` and `from_dpd_bytes([u8;
   8]) -> Self`. This extends the ADR-0009 DPD interchange posture
   (decimal128) to decimal64. The declet primitive is the same IEEE
   754-2008 §3.5.2 boolean transcription, kept per crate under the
   ADR-0031 precision-local carve-out. The width-specific constants are
   5 declets (50-bit trailing significand), an 8-bit exponent
   continuation, and the 10^15 leading-digit split. `from_dpd_bytes` is
   total over all 2^64 inputs.

The verification posture per item:

- `compareSignaling` and `nextToward` are exact, dispatch-only ops in
  the ADR-0021 exact-oracle domain, so they add no Kani harness
  (ADR-0031's precedent for its eight ops). Decimal64 and Decimal128 are
  pinned against the vendored `ddCompareSig` (557 of 559) / `dqCompareSig`
  (559) and `ddNextToward` (302 of 304) / `dqNextToward` (304) decTest
  files (the dq counterparts vendored from the suite-2.62 archive under
  ADR-0042 integrity). Decimal32 has no upstream `ds*` vectors for either
  op, so it rests on hand-derived boundary unit tests plus the
  cross-format-identical code path the larger formats exercise (the
  ADR-0031 Decimal32 posture). The Decimal64 / Decimal128 conformance
  runners compare the underflow / overflow / clamp flags exactly, which
  validates the `nextToward` flag rule directly.

- The Decimal64 DPD codec ports the three ADR-0009 Kani harnesses
  (`declet_decode_total`, `from_dpd_bytes_total`, `dpd_roundtrip_specials`,
  all verified) plus module and property tests. Conformance runs the two
  wholly-DPD-hex files: `ddEncode` all 376 pass, `ddCanonical` 190 of 230.

The surface is additive and non-breaking, a SemVer-minor bump on all
three: `ferrodec` 3.2.0 to 3.3.0, `ferrodec-decimal64` 3.2.0 to 3.3.0,
`ferrodec-decimal32` 3.2.0 to 3.3.0.

## Consequences

The GDA-extension dispatcher gap collapses to zero. Together with the
prior closures, the only remaining conformance `Skip` sources are the
ADR-0005 non-IEEE rounding directives and two intrinsic
BID-structural residuals, both documented and counted:

- The `ddCanonical` 40 skips are non-canonical-declet preservation cases
  (the `copy` family and NaN non-canonical-payload cases). A BID-backed
  codec cannot satisfy them: `from_dpd_bytes` canonicalizes the declets
  on decode and `to_dpd_bytes` always emits the canonical form, so a
  non-canonical expected pattern is unreachable. This is the same
  residual as the decimal128 dqCanonical 90 / 154 split (ADR-0009) and
  the same intrinsic-limit posture as the fd-61r CLAMPED residual
  (ADR-0048). The runner detects it with a re-encode-equality predicate
  (an expected `#hex` that re-encodes to a different pattern is skipped)
  and tallies it as a structural category, pinned per ADR-0010.

The new surface is six method signatures (`compare_signaling` and
`next_toward` on each of the three formats) plus the Decimal64 `dpd`
feature with two methods. All are plain inherent `pub fn`, non-breaking,
no new trait impls. The `compareSignaling` and `nextToward` methods are
ungated core surface; the DPD codec is behind the `dpd` feature so the
embedded code-size floor is preserved when it is off.

This ADR is additive only. It does not amend the ADR-0029 breaking-change
set, and it extends the ADR-0009 DPD-interchange decision to a new width
rather than superseding it.

## Related

- General Decimal Arithmetic specification, Mike Cowlishaw, at
  speleotrove.com/decimal, the source of the operation semantics and the
  conformance vectors.
- ADR-0009: the decimal128 DPD interchange this extends to decimal64.
- ADR-0031: the eight-op GDA extension closure that deferred these three
  items and set the precision-local helper carve-out and the Decimal32
  hand-plus-cross-format verification posture.
- ADR-0041: the arbitrary-precision `ferrodec-decimal` `compare_signal`
  and `next_toward`, the libmpdec-validated semantics reference (not
  reusable code; the fixed formats are BID).
- ADR-0042: the hash-pinned vendored-fixture integrity discipline the
  newly vendored `dqCompareSig` / `dqNextToward` files follow.
- ADR-0048: the fd-61r CLAMPED BID-structural residual, the same
  intrinsic-limit posture as the ddCanonical non-canonical-declet skips.
- ADR-0010: the per-file exact-match conformance pins every new dispatch
  arm and the DPD path record.
- Beads: `fd-bef` (the umbrella) and `fd-bef.1` .. `fd-bef.5`.
