# Verifying ferrodec: the transcendental testing surface

This document explains how the ferrodec family establishes confidence
in its transcendental functions, what each layer of the testing
surface actually proves, and where the residual frontier lies. It is
written for a user deciding whether to trust the numbers and for a
future maintainer extending the suite. The decisions behind these
layers are recorded as ADRs in the repository (notably ADR-0010,
ADR-0021, ADR-0024, ADR-0025, ADR-0026); this document is the
conceptual map those records assume.

Two facts shape everything below. First, ferrodec promises *faithful*
rounding for the transcendentals (the returned value is one of the two
representable values adjacent to the true result, at most one unit in
the last place from it), not correct rounding. Correct decimal
rounding of every transcendental is the Table Maker's Dilemma and is a
research programme, not an engineering deliverable; ADR-0021 and
ADR-0024 record why the weaker, dischargeable claim is the honest one.
Second, the three sibling crates (`ferrodec` for Decimal128,
`ferrodec-decimal64`, `ferrodec-decimal32`) compute every
transcendental on one shared Extended precision kernel,
`ferrodec-transcend`. That sharing is frugal and convivial, and it
creates the single most important hazard the testing surface exists to
contain.

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
- `asinh`, `acosh`, and `atanh` are their logarithmic forms.
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

State the limit plainly so it is not rediscovered or overclaimed. The
result the testing surface supports is strongly corroborated faithful
rounding plus a frozen worst case vector set whose correctly rounded
values are established at the specific committed arguments. It is not a
coverage proof: a finite committed corpus, even one found by a worst
case search, does not prove correct rounding over the continuum. It is
not proven correct rounding of the functions: that needs proven
hardest case bounds per transcendental per decimal width, which do not
exist in the literature.

The residual frontier is therefore narrowed, not eliminated. A
systematic sub condition number bias in the shared Extended `exp` or
`ln` core, confined to argument decades that lie outside every frozen
vector and outside the astro-float sound domain, is corroborated
against by three structurally independent oracles but is not provably
excluded. Named concretely: the exposure is a rounding bias on `exp`
or `ln` boundary arguments that the committed corpus did not happen to
include and that the differential sweeps did not happen to land on.
ADR-0026 records this in the same terms; it is the frontier a future
contributor would push by extending the corpus toward it, not a defect
known to exist.
