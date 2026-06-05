# ADR-0048: §7.4 CLAMPED fidelity, and the BID structural residual

- **Status**: accepted
- **Date**: 2026-06-04

## Context

IEEE 754-2019 §7.4 / the General Decimal Arithmetic specification define CLAMPED as
an informational flag: it is raised when a result's value is exact but its preferred
(ideal) quantum exponent had to be constrained into the format's representable range.
No value is ever wrong because of CLAMPED; only a quantum-aware consumer reads it.

ferrodec under-raised CLAMPED, and worse, all three fixed-format decTest conformance
comparators masked it out. About 421 decTest cases that expect CLAMPED passed
vacuously: the headline "0-fail against decTest" claim carried a hidden asterisk for
this flag. ADR-0035 named the masking a deliberately deferred oracle blind spot, and
the bead fd-61r recorded the gap as the path to full GDA-testsuite coverage.

Investigating it surfaced a structural limit. The arithmetic ops already thread the
GDA ideal quantum as `q_preferred`, so most clamp sites are detectable. But when an
operand's own exponent exceeds the format quantum range, BID has no wide working
exponent to hold it: the operand is normalised into a padded cohort at parse
(`1E+384` is stored as `1000000000000000E+369`), and the pre-clamp exponent is gone.
The decNumber reference keeps that exponent and raises CLAMPED from the operation;
ferrodec cannot, because in BID the stored operand already reads as the clamped
cohort. So `divide(9e6144, 1)`, `add(1E+384, 1E+384)`, and the like cannot raise
CLAMPED in this representation. Full §7.4 CLAMPED fidelity is not achievable in BID.

## Decision

1. **The precise §7.4 rule.** Raise CLAMPED if and only if the delivered result is
   value-exact at its ideal quantum, but that ideal quantum was moved into the
   representable range by the **exponent-range** limit: a high-end pad (ideal beyond
   Qmax, coefficient padded with trailing zeros down to Qmax) or a low-end / zero
   clamp (a zero whose ideal quantum is below Qmin, delivered at Qmin / Etiny). The
   discriminator against precision-driven rounding is that the clamp fires only on
   the range limit in the rounding kernel, never on the precision limit. add / sub
   cancellation never clamps (the ideal `min(qa, qb)` of two in-range operands is in
   range). quantize is out of scope (GDA raises Invalid, not Clamped; the decTest
   Clamped vectors for it are commented out).

2. **Locate the decision in the shared rounding kernel.** Each format's
   `finalise_finite` raises CLAMPED on the non-zero pad branch and, via a threaded
   `q_ideal` (the operation's preferred quantum captured before the Step 1 subnormal
   drop), on a zero result whose ideal quantum fell outside `[Qmin, Qmax]` (including
   a subnormal underflow that rounds to zero, whose delivered exponent has already
   been pulled to Qmin). Decimal128's few inline zero short-circuits that bypass the
   kernel (multiply by zero, 0 over finite divide, scaleb of zero) raise it inline.
   A literal whose quantum exceeds the range now raises CLAMPED at parse, since parse
   routes through the same kernel, which covers the toSci / Base and the DPD Encode
   cases.

3. **Harden the oracle.** All three conformance runners now compare CLAMPED
   (separately from the five IEEE flags). The headline 0-fail claim is no longer
   vacuous for CLAMPED on the detectable surface.

4. **Accept and document the BID structural residual.** The pre-clamped-operand
   cases cannot be raised. The runners detect them by re-parsing operands (an operand
   that itself raises CLAMPED at parse is pre-clamped) and skip those cases rather
   than failing, tallying them as a named structural-CLAMPED category. The residual
   is 55 cases on the standard corpus: Decimal128 20 (dqDivide 4, dqFMA 7,
   dqRemainderNear 9), Decimal64 35 (ddAdd 5, ddDivide 5, ddFMA 7, ddRemainder 9,
   ddRemainderNear 9), Decimal32 0. The per-file pass-count pins (ADR-0010) record
   the residual exactly: a structural case that regressed to a fail, or one that
   started passing, both move the pinned count and fail the build.

## Consequences

The decTest oracle now honestly compares CLAMPED across all three fixed formats,
raised at every clamp site BID can detect, on both the standard and the
`--features dpd` corpus. A silent mask has been replaced by real passes plus a
visible, counted, documented skip. There is no correctness change: every divergence
was flag-only, the values were always correct, so all value-based tests are
unchanged.

The structural residual is permanent and intrinsic to the BID encoding, not a defect
to be fixed later; raising those cases would require a working representation wider
than the storage format (decNumber's model), which is a different library. The skip
predicate keys on the genuine cause (an operand clamped at parse), so it cannot
silently absorb a regression in a fixable case: such a case would move from pass to
skip and trip its per-file pin. KNOWN_ISSUES.md carries the structural-CLAMPED
category and its counts.

This is an informational-flag and verification-honesty improvement, not a numerical
one. It needs no version bump; the conformance and kernel changes are observable only
to a consumer that inspects the §7.4 flag.

## Related

- Other ADRs: extends ADR-0035 (Decimal128 parity train and conformance oracle
  hardening, which named the CLAMPED mask as deferred); relates to ADR-0010 (per-file
  pin discipline) and ADR-0009 (DPD interchange behind the `dpd` feature).
- Bead: `fd-61r`.
- Commits: `31cf613` (Decimal128), `dbc5a30` (Decimal64 / Decimal32).
