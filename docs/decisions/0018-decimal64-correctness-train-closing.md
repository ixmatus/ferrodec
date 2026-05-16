# ADR-0018: decimal64 H tier correctness train closing

- **Status**: accepted
- **Date**: 2026-05-15
- **Supersedes**: ADR-0017

## Context

ADR-0017 carved a dedicated decimal64 correctness slice out of the
1.15 cycle. Slice D had wired the conformance dispatcher far enough
to expose three H class correctness bugs in `Decimal64` that mirror
`Decimal128`'s pre 1.13 H tier shapes (finite operand addition
magnitude loss, wrong rounding direction at the 16 digit precision
boundary, and a `pack_finite` precondition overflow in FMA). The
1.13.x decimal128 fixes had never propagated to the decimal64 port
because the dispatcher gap masked the absence. ADR-0017 deferred the
dispatcher buildout, recorded the three bugs in
`ferrodec-decimal64/KNOWN_ISSUES.md` with decTest reproducers, and
specified that decimal64 should get the same six agent review
(ADR-0010 methodology) the decimal128 surface received rather than
case by case triage.

This ADR records the slice that closed the train and supersedes
ADR-0017.

## Decision

The decimal64 correctness slice ran to completion and ships as
`ferrodec-decimal64` 1.4.0:

1. **Phase 1 review.** Six general purpose agents swept the decimal64
   op surface under the ADR-0010 rubric and the provenance and
   security discipline (no Intel decimal library or IBM decNumber
   recall; parser threat model; transcendental bound independence
   audit). Output: 9 H, 14 M, 17 L unique findings.

2. **H tier closed.** H1 through H9 landed one concern per commit:
   the H3 typed `BiasedExp` / `Coefficient` newtypes (illegal biased
   exponents made unrepresentable rather than debug asserted), H1
   addition magnitude loss, H2 effective subtract residue borrow, H4
   FMA preferred quantum threading, H5 dynamic `rem` alignment bound,
   H6 quantize on zero, H7 the breaking `to_f64` signature change
   (signaling NaN raises `INVALID`), H8 parser counter saturation,
   H9 `Status::CLAMPED` emission at the in operation clamp sites.

3. **M tier closed.** M1 through M15: the conversion correctness set
   (`to_f32` direct path, exact integer conversions, `from_f64`
   signaling bit pattern), the transcendental documentation set
   (parser threat model, saturation and argument reduction
   envelopes), the five ADR-0016 special case only Kani shim groups
   for the transcendental cluster, and an `astro-float` property
   oracle.

4. **L tier closed.** L1 through L13 and L15 through L17, grouped one
   commit per area: dead assertion and invariant documentation, the
   `rem` naming and unused rounding mode, the `Inf / 0` status
   rationale, dead arms and single decode, the parse cast
   compile time invariant, the zero engineering rendering fix, the
   opaque error message, and the stable `Debug` surface. L14 (a
   richer parse error enum) is deferred to v2.0 as a breaking
   change.

5. **Conformance dispatch wired, and a residual found.** Wiring the
   `add` / `subtract` / `multiply` / `divide` / `fma` arms surfaced
   fd-d47: a residual H2 class boundary defect distinct from the H2
   fix. The add and FMA alignment used a static 22 digit window that
   truncated the lower operand prematurely when the dominant
   coefficient was small. It was root caused and fixed with a
   dynamic per side shift bound keyed on the actual digit count (the
   same shape as the H5 `rem.rs` fix), so the subtraction stays
   exact whenever it fits in `u128`. The full `dd*` corpus then ran
   with zero failures, and the per file pass counts are guarded
   exact match per ADR-0010.

6. **decimal32 stays separate.** Its thinner `KNOWN_ISSUES.md` is a
   distinct follow up slice; folding it in would have doubled the
   moving parts for no shared benefit.

## Consequences

**Wins.**

- `Decimal64` now conforms to the General Decimal Arithmetic spec
  for the arithmetic operations users actually call:
  `ddAdd` 973, `ddSubtract` 514, `ddMultiply` 444, `ddDivide` 702,
  `ddFMA` 1318, `ddBase` 708, the full corpus at zero failures, with
  the counts guarded per file so a silent regression cannot hide.
- The H3 invariant moved from a debug assertion into the type
  system, so an out of range biased exponent is now a compile error
  at the construction site rather than undefined release behaviour.
- The transcendental cluster has Kani no panic and special case
  propagation proofs plus an arbitrary precision property oracle,
  so its correctness is captured in proofs and tests rather than
  rediscovered.
- fd-d47 is the system working as designed: the conformance
  dispatch, added as the regression guard for the H fixes,
  immediately caught a residual the H tier had not covered, and it
  was fixed before the arm landed under the zero failure ceiling.

**Costs.**

- `to_f64`'s breaking signature change forces a one line migration
  on every caller. The honest signature (a rounding mode in, a
  status out) is worth the churn; it is called out in the 1.4.0
  CHANGELOG.
- The non arithmetic `dd*` operations (comparison, the quantum
  family, the bitwise and copy family, DPD interchange) still route
  to skip in the dispatcher. The methods are exercised by unit and
  property tests; the gap is dispatcher coverage and is documented
  as a limitation, not a correctness claim.

**Drift.**

- `ferrodec-decimal64/KNOWN_ISSUES.md` is the source of truth for
  the closed bugs and the two remaining documented limitations (the
  GDA ideal exponent Clamped case and the dispatcher coverage gap).
  ADR-0017's status moves to superseded by this ADR.

## Related

- ADR-0010: testing strategy after the six agent correctness review
  (the methodology applied here).
- ADR-0016: Kani harness shim routing rule (the special case only
  shim shape used for M10 through M14).
- ADR-0017: the superseded decimal64 conformance coverage gap ADR.
- `ferrodec-decimal64/KNOWN_ISSUES.md`: the closed bug audit trail
  and the two remaining documented limitations.
- `ferrodec-decimal64/CHANGELOG.md`: the 1.4.0 entry.
- Plan and findings: the decimal64 correctness slice plan and the
  six agent findings document archived under the workspace decision
  records.
