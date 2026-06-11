---
slug: glossary
category: glossary
citation: "ferrodec glossary: the decimal-754 vocabulary, defined against the GDA specification and IEEE 754-2019."
canonical: n/a
doi: n/a
archived: n/a
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "repo (MIT OR Apache-2.0)"
vendor-status: n/a
rot-risk: n/a
provenance: primary
consumers:
  - README.md
  - docs/testing.md
  - KNOWN_ISSUES.md
verification:
  - ferrodec-test-support/tests/references_integrity.rs
notes: "One terminology registry so every document and a future manual use the same words the same way. Definitions are paraphrased against cowlishaw-gda-arith and ieee-754-2019 (cited by slug, not repeated); the entry adds the ferrodec-specific reading where one exists."
---

# Glossary: the decimal-754 vocabulary

Sources: [cowlishaw-gda-arith](cowlishaw-gda-arith.md) for the GDA
terms, [ieee-754-2019](ieee-754-2019.md) for the standard's terms.
Each definition notes where ferrodec's reading has teeth.

**cohort.** The set of representations sharing one numeric value at
different exponents (`1E+1`, `10E+0`, `100E-1`). Decimal formats,
unlike binary, do not normalize: which cohort member an operation
returns is specified (see *preferred exponent*), and ferrodec's
conformance comparisons are cohort-exact, not merely value-exact.

**quantum.** The value of one unit in the last place of a
representation, `10^exponent`. `sameQuantum` and `quantize` are
operations over quanta, not values.

**preferred (ideal) exponent.** The exponent the standard or GDA
specifies for each operation's result when the value is exactly
representable (for add: `min(Q(x), Q(y))`). The cohort discipline
above is what makes this testable.

**clamped.** The informational condition (flag `CLAMPED`) raised when
a result's preferred quantum falls outside the representable range
and the exponent is adjusted to fit. See the registry-status-flags
entry and ADR-0048 for the BID structural residual.

**subnormal.** A nonzero value below the minimum positive normal
magnitude `10^Emin`; representable at reduced effective precision.
The UNDERFLOW flag detects tininess before or after rounding per the
standard's choice; ferrodec follows decTest (before rounding, the
fd-99f lesson).

**ulp.** Unit in the last place at a given precision and exponent.
Margins in the transcendental proofs are stated in fractions of an
ulp of the destination format.

**correctly rounded.** The returned result equals the infinitely
precise result rounded once to the destination under the active
rounding direction. The §9.2 surface holds this contract on all
three formats (ADR-0032).

**faithfully rounded.** Within one ulp of the infinitely precise
result: one of the two neighbors. Strictly weaker than correctly
rounded; the astro-float oracle asserts this bound as a hard-defect
catcher (the superseded ADR-0024 contract).

**quiet NaN / signaling NaN / payload.** NaNs carry a diagnostic
payload in the trailing significand. Quiet NaNs propagate; signaling
NaNs raise INVALID in most consumers (the class operation is the
quiet exception). Propagation is first-NaN-wins with sNaN priority
quirks the conformance comparator checks sign and payload for
(fd-92w.1).

**BID / DPD.** The two IEEE 754-2008+ decimal interchange encodings:
Binary Integer Decimal (coefficient as a binary integer; ferrodec's
arithmetic representation, ADR-0001) and Densely Packed Decimal
(declet-coded coefficient; the `dpd` feature's interchange surface,
ADR-0009).

**canonical encoding.** The unique encoding the standard designates
within each cohort member's bit patterns; non-canonical coefficients
decode as zero per §3.5.2 canonicalization, and `is_canonical` /
`canonical` expose the predicate and the canonicalizer.

**context.** The GDA evaluation environment: working precision,
exponent range (Emax, Emin), rounding, clamp. The fixed formats bake
their context into the type; `ferrodec-decimal` carries it as a
value, which is the deepest API difference between the two layers.

**trailing significand.** The encoded coefficient field of an
interchange format (the part after the combination field), holding
the coefficient digits or the NaN payload.

**round-half-even.** `NearestEven` tie breaking (banker's rounding),
the IEEE default. decTest's `half_up` directive maps to
`NearestAway` (a vendored-harness lesson worth naming because the
two are easy to conflate).

**flags versus traps.** IEEE 754 alternate exception handling (traps)
is not implemented anywhere in the family; every operation returns
`(result, Status)` and the caller accumulates (ADR-0002).
