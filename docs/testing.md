# Verifying ferrodec: the transcendental testing surface

This document explains how the ferrodec family establishes confidence
in its transcendental functions, what each layer of the testing
surface actually proves, and where the residual frontier lies. It is
written for a user deciding whether to trust the numbers and for a
future maintainer extending the suite. The decisions behind these
layers are recorded as ADRs in the repository (notably ADR-0010,
ADR-0021, ADR-0024, ADR-0025, ADR-0026, ADR-0032); this document is
the conceptual map those records assume.

Two facts shape everything below. First, the §9.2 transcendental
surface (`exp`, `ln`, `exp2`, `log2`, `log10`, `cbrt`, the trig and
inverse trig family with `atan2`, the hyperbolic and inverse
hyperbolic family, and `pow`) is correctly rounded on all three
formats: the returned value is the single nearest representable
result at every IEEE 754-2019 rounding direction, ties to even at
`NearestEven` and the directed grid point at the four directed
modes. The mechanism is a 50 digit `Extended` working precision
kernel whose error bound exceeds the smallest empirical Arb worst
case half ULP margin (`4.167e-8` for `cosh` at Decimal32) by more
than thirty orders of magnitude on every format; ADR-0032 records
the per function derivation and supersedes ADR-0024's earlier
faithful (≤ 1 ULP) contract. The corpus test
(`tests/transcend_vectors.rs` and the sibling mirrors) is the
standing empirical witness, with MPFR cross confirmation behind the
optional `mpfr-gate` feature (ADR-0026).

Second, the three sibling crates (`ferrodec` for Decimal128,
`ferrodec-decimal64`, `ferrodec-decimal32`) compute every
transcendental on one shared Extended precision kernel,
`ferrodec-transcend`. That sharing is frugal and convivial, and it
creates the single most important hazard the testing surface exists
to contain. The ADR-0032 correctness proof holds on the
implementation as written; the testing surface exists to catch every
class of defect that could quietly invalidate that proof, including
ones that would propagate coherently across the family.

## The correlated failure surface

### The kernel derives functions from a few primitives

`ferrodec-transcend` does not implement every function independently.
It computes a small set of primitives directly, each by argument
reduction followed by a series, and expresses everything else as a
composition of those primitives. The dependency graph, read out of
`ferrodec-transcend/src`, is this:

**Primitives** (no other kernel function in their data path):

- `exp`: reduce by `ln 10`, then a Taylor series.
- `ln`: decade decomposition plus a halve and double reduction, then a
  Taylor series.
- `sin` and `cos`: Payne and Hanek argument reduction against a wide
  fixed table, then Taylor series.
- `atan`: an inversion step and a `pi/4` shift, then a Taylor series.

**Derived** (computed as a composition of the primitives):

- `exp2` is `exp(x * ln 2)`.
- `log2` is `ln(x) * (1 / ln 2)`; `log10` is `ln(x) * (1 / ln 10)`.
- `cbrt` is `exp(ln|x| / 3)`.
- `pow` is `exp(y * ln|x|)`.
- `sinh` and `cosh` are `(e^x +/- e^-x) / 2`; `tanh` is `sinh / cosh`.
- `asinh`, `acosh`, and `atanh` are their logarithmic forms (the
  cancellation free log1p variants in the small argument and near 1
  bands; ADR-0050).
- `tan` is `sin / cos`.
- `asin`, `acos`, `atan2` route through `atan`.

`exp` and `ln` carry the widest blast radius: eleven derived functions
read through one or both of them.

### A primitive defect propagates coherently

The consequence governs the whole testing strategy. A defect in a
primitive does not stay local. It enters every function derived from
that primitive with the same sign and a related magnitude, because the
derivative literally calls the defective primitive. A systematic half
unit bias in the Extended `exp` core, for example, moves `exp2`,
`cbrt`, `pow`, `sinh`, `cosh`, and `tanh` together, since each reads
through `exp`; a bias in the `ln` core moves the other cluster,
`log2`, `log10`, and again `cbrt` and `pow`. The failures are not
independent samples; they are one failure observed through many
windows.

This is why the testing surface cannot be assembled from convenient
checks. Two kinds of check are worthless against a primitive defect,
and recognising them is the load bearing knowledge:

1. **A check whose two sides both flow through the suspect primitive
   cannot fail when that primitive is wrong.** The error enters both
   sides and cancels. ADR-0025 hit this directly: an earlier
   metamorphic identity set included relations such as
   `log_b(x) * ln(b) ~= ln(x)`, `tanh ~= sinh / cosh`,
   `exp2 == pow(2, x)`, and the `asinh` and `atanh` logarithmic forms.
   Each compares the shared kernel against itself. They look like
   coverage and prove nothing. ADR-0025 audited the kernel and removed
   them, recording the dropped identities so they are not reintroduced.

2. **A single fixed precision oracle is not a sufficient mitigation
   either.** The faithful rounding oracle (astro-float, ADR-0021) is an
   independent computation, but it is one implementation at one fixed
   working precision. Past a bounded argument magnitude its enclosure
   no longer has the digits to bracket the format result, so the
   property suites skip out of that domain by construction (the
   `coef.ilog10() + exp > 15` guard for `sin` and `cos`, and the
   magnitude scaled precision in the large argument suite). In the
   skipped decades that oracle says nothing, and a bias confined to
   those decades would pass every check that depends on it.

### Only a structurally independent implementation mitigates it

A defect in a shared primitive is visible only to a check that does
not share the primitive's structure. Concretely, a useful check must
route through neither the Extended kernel nor the one fixed precision
oracle. This is a first class acceptance criterion in ADR-0026: every
verification layer added for the transcendentals is justified as
structurally independent of the thing it is meant to exercise, or it
does not count as evidence.

Three references satisfy that criterion and were added on exactly this
reasoning (ADR-0026; their characterized power and blind spots are
catalogued in the next section):

- **mpmath**, an independent arbitrary precision implementation,
  reached through a local subprocess. It shares neither the kernel nor
  astro-float, so it breaks the correlation across the whole special
  function surface, cheaply and broadly. It is the least rigorous of
  the three: adaptive precision, not certified.
- **Arb (FLINT)**, used offline to generate a frozen corpus of hard to
  round vectors with a decimal Table Maker's Dilemma search. Arb
  computes certified ball enclosures, so where an enclosure does not
  straddle a decimal half unit boundary the correctly rounded value is
  established, not sampled. The corpus is committed as data; Arb never
  enters the build.
- **MPFR (rug)**, the industrial gold standard, behind a dev only gate,
  used to recompute the frozen corpus independently. Two independent
  gold references agreeing is the strongest acceptance criterion short
  of a discharged proof, so the corpus accept rule is: the Arb
  enclosure is decisive and MPFR agrees.

### The honest residual frontier

State the limit plainly so it is not rediscovered or overclaimed.
The result the testing surface supports is correctly rounded §9.2
transcendentals on all three formats, proven by the 50 digit
Extended kernel's headroom (more than thirty orders of magnitude
over the smallest empirical Arb worst case half ULP margin) and
corroborated at every committed Arb worst case vector with MPFR
agreement. The per function headroom derivation lives in ADR-0032
§Decision. The headroom is a proof envelope on the implementation
as written; it is not a closed proof over the continuum, because
the empirical worst case bound was found by a finite Arb search and
not derived from proven hardest case bounds per transcendental per
decimal width (which do not exist in the literature for every §9.2
function).

The residual frontier is therefore narrowed, not eliminated. The
exposure is a hard to round case the empirical worst case search did
not surface, sitting inside the kernel's >30 orders of magnitude
headroom (so the relative rounding error to the true result is
bounded by far less than half an ULP at every input within the
proof envelope) but not provably absent at every input across the
continuum. The frontier is the same exposure the README disclosure
names: rounding errors on §9.2 transcendental boundary inputs the
Arb empirical worst case search did not happen to surface. A future
contributor pushes the frontier by extending the corpus toward it
and rerunning the worst case search; it is not a defect known to
exist.

The frontier has produced one realized defect class so far, and the
record belongs here rather than in a changelog: the 2026-06-09
review found the proof envelope's error model itself failing in the
near anchor bands (absorption at 0 and 1 turned the kernel's
relative error absolute, outside every layer's coverage). ADR-0050
records the reformulations that repaired it and the committed band
corpus that now pins the class; ADR-0051 closed the residue
(directed mode results grid stuck on an anchor) by carrying a
signed residual across the rounding seam, with the band corpus
extended over the small argument trigonometric, hyperbolic, and
exponential families as the standing witness.

## The layered oracle stack

No single layer is sufficient; the confidence comes from layers whose
blind spots do not coincide. ADR-0010 records the overall strategy
after the six agent correctness review; ADR-0025 records the
metamorphic layer and the tautology audit. For each layer below, the
power is what it actually establishes and the blind spot is what it
provably does not see. The blind spots are the load bearing knowledge:
a layer is only useful in combination with one whose blind spot it
covers.

### 1. decTest conformance vectors

`tests/conformance.rs` replays IBM's General Decimal Arithmetic
testcases through the family.

- **Power.** Specification exact for the operations it dispatches:
  arithmetic, comparison and quantum, round to integral, and the
  string and apply surfaces, at the format precision, with the
  expected per file counts pinned so a silent regression in one file
  cannot hide behind a gain in another.
- **Blind spot.** No transcendentals at all (the suite has none). The
  bitwise families (ADR-0031) and the copy family, truncating
  remainder, round to integral, plus, and min and max magnitude
  (fd-aqs.11) now dispatch and pass; the residual dispatcher gaps are
  narrow, chiefly `toEng` rendering (engineering notation the fixed
  formats do not expose) and the `canonical` operation, plus the DPD
  interchange files that only run under the `dpd` feature. Those are a
  dispatcher coverage gap, not an implementation gap. The non IEEE
  rounding directives `half_down` and `05up` are a deliberate will not
  fix (ADR-0005).

### 2. The exact integer oracle

The arithmetic correctness oracle (ADR-0021) computes the correctly
rounded result with exact integer arithmetic and no tolerance.

- **Power.** Bit exact: it asserts the single correct value for
  `add`, `subtract`, `multiply`, `fma`, `divide`, and `squareRoot`
  per rounding direction, and is itself pinned against the
  specification's own vectors with no ferrodec arithmetic in the loop.
  A half unit systematic bias cannot pass it, unlike a one unit
  envelope.
- **Blind spot.** Arithmetic only. It says nothing about the
  transcendentals.

### 3. The faithful astro-float oracle

The transcendental property suites bracket each result against
astro-float, a pure Rust arbitrary precision implementation
(ADR-0021).

- **Power.** Independent of the kernel; asserts the faithful (≤ 1
  ULP) contract per rounding direction across the supported domain.
  Under the ADR-0032 correctly rounded production contract this
  layer validates the strictly weaker invariant that any correctly
  rounded kernel result must also satisfy: a result outside the 1
  ULP envelope is a hard kernel defect. The layer was added before
  ADR-0032 tightened the contract and survives the tightening
  intact, since faithful is the weaker property a correctly rounded
  result automatically meets. Pure Rust, so it runs in the default
  test set and in CI.
- **Blind spot.** One implementation at one fixed working precision.
  It has a sound magnitude domain: past a bounded argument magnitude
  it loses the digits to bracket the result and the suites skip out of
  that domain by construction (the `sin` and `cos` magnitude guard,
  the magnitude scaled large argument suite; fd-3cd, fd-dfs). In the
  skipped decades it is silent. It validates faithful, not correctly
  rounded; the corpus layer (7) and the MPFR gate (8) provide the
  bit for bit witness to the tighter contract.

### 4. Metamorphic identities

Algebraic relations that hold for the exact functions regardless of
magnitude (`tests/property_metamorphic.rs`, ADR-0025), the backstop in
the decades the fixed oracle skips.

- **Power.** Needs no oracle, so it keeps teeth at any magnitude.
  Category A relations cross compute the same quantity through two
  mutually independent kernels and carry a tight band; Category B
  inverse round trips carry an analytic condition number derived
  bound.
- **Blind spot.** Identities whose two sides share a kernel helper are
  tautological and were removed (ADR-0025 lists them so they are not
  reintroduced). The surviving condition amplified bounds are not
  correctly rounded tight, so a sub condition number systematic bias
  can still hide; Category C cancellation checks are explicitly weak
  and small argument only. It corroborates the faithful oracle where
  it cannot reach; it does not replace it where it can.

### 5. The cross-precision value oracle

Finite `Decimal64` values widen losslessly to `Decimal128`; the
operation runs in the wider format and narrows back
(`tests/d128_crosscheck.rs`, the D64 to D128 direction).

- **Power.** Catches a value divergence between the two precisions
  using the wider format as a same family reference.
- **Blind spot.** Value, not cohort: the §7.4 preferred exponent
  policy can legitimately differ between precisions (fd-61r), so the
  comparison is the cohort insensitive IEEE compare, not bit equality.
  The `rem` reference is `Decimal128::rem_trunc`, because
  `Decimal64::rem` is GDA truncating while `Decimal128::rem` is IEEE
  nearest, a deliberate sibling API asymmetry (fd-pvu). Status flag
  interaction with that cohort policy is not strictly cross checked.

### 6. The local differential (libmpdec and mpmath)

`tests/differential.rs`, behind the `differential` feature, ships a
batch to a Python subprocess (`tools/diff_oracle.py`).

- **Power.** Two independent decimal aware references that share
  neither the kernel nor astro-float, so they break the correlation.
  CPython's `decimal` is libmpdec, correctly rounded decimal native
  with no magnitude limit; it cross checks the exact arithmetic and
  the `exp`, `ln`, `log10`, `pow` family, and the `exp` and `ln`
  sweeps deliberately reach the decades the fixed oracle skips. mpmath
  covers the special function surface `decimal` lacks (`exp2`, `log2`,
  `cbrt`, the trigonometric, inverse, and hyperbolic families,
  `atan2`), with the decimal rounding performed on our side so no
  double rounding enters.
- **Blind spot.** Local only by construction: the `differential`
  feature is off by default and absent from CI, so a default
  `cargo test` and the CI matrix never spawn Python; a nightly lane is
  a deferred follow up, not wired. It cross checks value, not cohort
  (the fd-61r reason again). `sqrt` is compared under nearest even
  only, because libmpdec's `Decimal.sqrt` ignores the context
  rounding mode while ferrodec's is correctly rounded per direction;
  the directed modes there are libmpdec's limitation, not a ferrodec
  defect. Overflow and underflow signals are not strictly compared,
  for the same cohort and context interaction. mpmath is adaptive
  precision and not certified, so it gives breadth, not proof; an
  mpmath response of `NaN`, `Infinity`, or a missing interpreter
  counts as a diagnostic skip, never a gate.

### 7. The Arb frozen vector corpus

`tests/transcend_vectors.rs` (and the sibling mirrors) check the
kernel against `tests/vectors/transcend/`, a committed corpus
generated offline with Arb and a decimal Table Maker's Dilemma worst
case search (ADR-0026).

- **Power.** Proof, not sample: Arb's certified ball enclosure, where
  it does not straddle a decimal half unit, establishes the correctly
  rounded value, so each vector is a checked fact. There is no oracle
  and no C binding in this test's path; it parses committed text, so
  it is default on and runs in standard CI. It is the strongest layer
  for the transcendentals and the only one that reaches inside the
  oracle skip decades with a proof.
- **Blind spot.** A finite corpus, even one found by a worst case
  search, is not a coverage proof over the continuum. Regenerating it
  needs the offline `python-flint` tool; the committed data does not.
  Its decade coverage also stops short of the additive anchors: the
  2026-06-09 review found the kernel's relative error model
  collapsing to absolute in the neighbourhoods of 0 and 1
  (mis-rounding `ln`, `log10`, `log2`, `atanh`, `asinh`, `asin`,
  `acos`, and `pow` there, by up to `3e8` ULP at `Decimal128`), and
  no vector in this corpus reached the affected bands.

A second committed corpus closes that blind spot. The near anchor
band corpus (`tests/vectors/transcend/anchor_bands/`,
`tests/transcend_anchor_bands.rs` and the sibling mirrors, ADR-0050)
pins the hazard decades around 0 and 1 with arbitrary coefficient
inputs at all three format precisions, generated by
`tools/gen_anchor_band_vectors.py`: mpmath at 160 dps under a
boundary margin guard, with libmpdec cross confirming `ln`, `log10`,
and `pow` at `NearestEven`, and exact per (function, mode) count
pins so a silent corpus trim cannot hide. Its power is the class the
Arb corpus could not see; its blind spots are the same finitude as
the Arb layer's, plus a scoping one stated in the generator:
directed mode lines appear wherever the oracle can certify the side
(the ADR-0051 residual seam decides the grid stuck cases in the
kernel), and only corrections below the generator's 10^-100 oracle
floor stay pinned at the nearest modes alone.

### 8. The MPFR dev gate

`ferrodec-test-support/tests/mpfr_gate.rs`, behind the `mpfr-gate`
feature, recomputes the entire frozen corpus with MPFR through `rug`
(ADR-0026).

- **Power.** An independent industrial gold standard. Two independent
  gold references agreeing is the strongest acceptance criterion short
  of a discharged proof, so this closes the corpus accept rule: the
  Arb enclosure is decisive and MPFR agrees. MPFR's ternary flag is
  reported as the instrument that confirms the corpus's correctly
  rounded value bit for bit, the strongest empirical attestation
  short of a proof.
- **Blind spot.** C binding and LGPL, so it is dev only and gated: off
  by default, never built in CI, never in the no_std build, the same
  containment granted to astro-float. It corroborates the frozen
  corpus; it is not a continuous gate.

### The frozen corpus does not retire decTest

A natural question, once the Arb corpus proves correctly rounded
values: are the decTest conformance vectors still useful? They are,
and the two barely overlap. They sit on three different axes, and the
frozen corpus subsumes the conformance vectors on none of them.

- **Operations.** The vendored decTest corpus is the General Decimal
  Arithmetic surface (arithmetic, comparison, quantum, round to
  integral, copy and bitwise, encode and decode, class, total order)
  and has no transcendentals. The frozen corpus is only the
  transcendentals. The function sets are disjoint: the frozen corpus
  fills decTest's transcendental blind spot, and decTest covers the
  entire surface the frozen corpus never touches.
- **Property.** The frozen corpus proves a value: the correctly
  rounded magnitude of `f(x)`, deliberately value not cohort, with no
  status flags and no special values. decTest tests specification
  conformance: the value and the cohort member (the §7.4 preferred
  exponent), the exact status flag set, NaN payload, signaling NaN,
  signed zero and infinity propagation, exponent clamping, and the non
  IEEE rounding directives. That behavioural dimension is most of what
  the standard mandates, and a value oracle cannot see it. It is the
  same reason a binary reference is rejected for the arithmetic
  differential but accepted for transcendentals (ADR-0025, ADR-0026).
- **Authority.** decTest is the canonical conformance suite authored
  by the specification's author. For a library whose completeness
  argument is measured against an external standard, that external
  attestation is load bearing; a corpus generated in house, however
  rigorous, cannot stand in for "passes the standard's own
  testcases."

The two are complementary by construction, the same disjoint blind
spot principle as the rest of the stack: the frozen corpus
strengthened the weakest layer, the transcendental value inside the
oracle skip region, and changed nothing about the
arithmetic conformance layer, where decTest remains the irreplaceable
authority.

### Reading the stack as a whole

The arithmetic surface is closed tightly by the exact integer oracle
and decTest. The transcendental surface is closed by a different
construction: a per function correctness proof (the 50 digit Extended
kernel's worst case error bound, ADR-0032), corroborated by
overlapping empirical layers whose blind spots are disjoint. The
faithful astro-float oracle is exact within its magnitude domain but
silent past it; the metamorphic identities keep teeth past it but
only to a condition scaled bound; the local differential adds two
structurally independent references but only when a developer runs
it; the Arb corpus turns specific worst case arguments into checked
facts inside the astro-float skip decades; and MPFR independently
confirms those facts bit for bit. The honest composite, restated
from Part I, is correctly rounded §9.2 transcendentals on all three
formats with the proof envelope as the load bearing claim, the
empirical corroborators as the standing defense, and the residual
frontier (a hard to round case the worst case search did not surface)
named rather than hidden. Formal proof (Kani) and the fuzz harness
exist for the special value, encode and decode, and total order
surfaces; they are panic and invariant guards, not transcendental
value oracles, and are documented in the README's Verification
section.

