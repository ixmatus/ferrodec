# ADR-0034: Empirical coverage extension to decimal32 sqrt and the Kani unreachable identity residue

- **Status**: proposed
- **Date**: 2026-05-28 (proposed)

## Context

ADR-0033 closed exhaustive correctly rounded verification for the 18
unary §9.2 transcendentals at `Decimal32`. Every canonical `Decimal32`
input was walked through a two tier certified Arb filter, and the
per function worst case half ULP margin was committed and replayed as
a default on kernel gate. The natural next question is how far that
empirical axis extends across the rest of the operation surface. A
proposal enumerated five candidate pieces: identity sweeps, exhaustive
sqrt, pinned operand binary sweeps, stratified random binary sampling,
and a consumer domain (STM32U class embedded calculator) box. This ADR
records which of those reduce entropy and which do not, and scopes the
engagement to the two that do.

The decisive distinction is the Table Maker's Dilemma. ADR-0033's
exhaustion was load bearing because a transcendental's true value can
sit arbitrarily close to a half ULP boundary with no a priori bound,
so a random sample misses arbitrarily small margins by definition. The
ADR-0033 campaign confirmed this empirically: the sampled corpus
minimum was typically `10^5` narrower than the true exhaustive worst
case, and on `acosh` it was roughly 7.8 million times off.

That pathology does not transfer to basic arithmetic. The exact result
of `add` / `sub` / `mul` / `fma` on finite decimals is a finite decimal
(a sum or product of finite decimals is exact in finite precision), and
`div`'s exact quotient is a rational whose correctly rounded value is
exactly decidable. The ties live at fixed grid points determined by the
precision difference structure, so the failure space is structurally
bounded and sampled coverage is far more representative. The cosh
equivalent "we missed by `10^6`" risk has no analogue here. Worse, the
binary input space at `Decimal32` is roughly `10^16` to `10^18` pairs,
so no binary sweep can be exhaustive: it would be harder sampling
dressed in the word "exhaustive," over a space already covered by
property tests, the fuzz harness, the General Decimal Arithmetic
conformance vectors, and the Kani special case dispatch proofs. The
three binary pieces are therefore deferred, not adopted (see Rejected
and deferred).

Two pieces survive.

**sqrt is the one follow up operation that shares ADR-0033's
structure.** sqrt is algebraic irrational, so it carries the same TMD
pathology as the transcendentals: its root can sit arbitrarily close to
a half ULP boundary, and a sample can under estimate the worst case
margin exactly as it did for the §9.2 set. sqrt is also IEEE 754-2019
§5 *mandatory*, a higher specification authority than the §9.2
*recommended* transcendentals ADR-0033 covered. Today sqrt rests on a
proptest envelope against `astro-float`, a sampled oracle. CBMC cannot
reason about the sqrt rounding loop, for the same reason the arithmetic
Kani harnesses shim out the alignment and rounding loops (ADR-0015,
ADR-0016), so no Kani proof of sqrt correct rounding exists to be
stronger than an empirical sweep. Exhaustive `Decimal32` sqrt is
therefore the top achievable verification tier for the operation, and
the ADR-0033 tooling discharges it almost unchanged.

**A trimmed identity residue survives the Kani redundancy test.** The
proposal's identity sweep mixed two classes. The first class is already
discharged totally by the Kani harnesses: the §5.7.2 / §5.4.2 canonical
predicate projection (`is_canonical` is equivalent to
`canonicalize` fixed point) and its idempotence, the BID and DPD
encode and decode round trips, the §5.10 total order antisymmetry on the
same cohort same sign finite finite domain, and the special case
dispatch arithmetic identities. A symbolic proof holds for every input;
an exhaustive loop over the `10^9` concrete `Decimal32` values is
strictly weaker, so sweeping these is pure redundancy and is dropped.

The second class is genuinely not proven, because the relevant code path
runs through the loops the Kani harnesses shim out, or over a domain the
harnesses restrict:

- `parse_str(to_string(x))` recovers the value `x`. This is the string
  round trip through `Display` formatting and parsing, a different and
  far more complex path than the bit level encode and decode round trip
  Kani proves. Today it is exercised only by the `parse` fuzz target,
  sampled, and only on `Decimal128`.
- `total_cmp(x, x)` is `Equal` across cohorts. Kani's total order domain
  is same cohort; reflexivity over different exponent representations of
  one numeric value sits outside it.
- `next_up(next_down(x))` recovers `x` (and the reverse) away from the
  format extremes.
- `add(x, 0)` obeys the General Decimal Arithmetic preferred exponent
  rule. In decimal arithmetic `x + 0` is not the trivial identity it is
  in binary: the result cohort follows the ideal exponent rule, and for
  many cohorts the operation runs the general alignment path rather than
  a zero short circuit.

For this residue, exhaustive empirical evaluation is the correct tier
precisely because the higher tier will not discharge. This is the
project's stated order of preference working as intended: proof first,
and an exhaustive example sweep as the fallback when CBMC defeats the
loop. The residue sweep needs no oracle and no Arb: it is kernel self
consistency, a pure Rust loop over the canonical `Decimal32` set.

`Decimal64` and `Decimal128` stay out of exhaustive reach, exactly as in
ADR-0033. Their canonical input cardinalities (`~10^16` and `~10^36`)
defeat exhaustion for sqrt and for the identities alike.

## Decision

### Exhaustive decimal32 sqrt

`sqrt` joins the ADR-0033 exhaustive sweep. Its domain is `x >= 0`
(negative finite inputs are the NaN special value contract, not a hard
to round case, and are excluded). The two tier filter
(`tools/d32_exhaustive_sweep.py`), the certified Arb `solve` and
`_decisive` machinery (`tools/gen_transcend_vectors.py`), the worst case
provenance format, and the worst case output re-derivation
(`tools/d32_exhaustive_compute_outputs.py`) all apply unchanged; the
extension is one `FUNCS` entry (`a.sqrt()`), one non negative domain
arm, and the function's inclusion in the sweep set. sqrt carries no
`f(1) = 0` TMD hard residue: `sqrt(1) = 1` and `sqrt(0) = 0` are exact,
and perfect square coefficients give exact roots, so the expected TMD
hard set is empty.

The deliverables mirror ADR-0033 exactly:

- `tests/vectors/transcend/sqrt_d32_exhaustive.prov`, the true
  exhaustive worst case half ULP margin and the input achieving it.
- `tests/vectors/transcend/exhaustive/sqrt.txt`, the worst case input
  paired with its proven correctly rounded output, the format the test
  harness consumes.
- A default on kernel replay in
  `ferrodec-decimal32/tests/transcend_vectors_exhaustive.rs`: the
  `sqrt` worst case row rounds correctly, lifting the gate from 18 to 19
  rows.
- An MPFR cross check arm in
  `ferrodec-test-support/tests/mpfr_gate.rs`, gated behind
  `--features mpfr-gate`, confirming the worst case row independently.

The per input enumeration outputs (`~10^9` rows) never enter the
repository, as in ADR-0033.

### The Kani unreachable identity residue

A new on demand exhaustive sweep
(`ferrodec-decimal32/tests/identity_exhaustive.rs`) enumerates the
canonical `Decimal32` values, constructing each through the public
constructor, and asserts the four residue identities named in Context:
the string round trip, cross cohort total order reflexivity, the
`next_up` / `next_down` mutual inverse, and the `add(x, 0)` preferred
exponent rule. The sweep is `~10^9` iterations, multiple minutes in a
release build and far too slow for a debug `cargo test`, so it is
either `#[ignore]`d and documented as a release only explicit run, or
gated behind a dedicated feature; it is not a default CI gate.

The sweep produces no frozen vector. Identities have no worst case row
to freeze; their deliverable is the runnable test plus this ADR's record
that the sweep ran clean over the full canonical `Decimal32` set, with
the input count, date, and commit. The identities the Kani harnesses
already prove totally are deliberately excluded and enumerated above so
the scope boundary is explicit.

## Consequences

- The `Decimal32` machine verified correctly rounded claim extends from
  the §9.2 recommended transcendentals to the §5 mandatory sqrt. The
  sqrt rustdoc on `ferrodec-decimal32` tightens from the proptest
  envelope citation to "ADR-0034 exhaustive `Decimal32` sweep proves
  margin X on every canonical input," and the default on kernel gate
  grows from 18 to 19 rows.

- The exhaustive `Decimal32` unary surface that admits the proof shape
  is then complete: the 18 §9.2 transcendentals plus §5 sqrt.
  `roundToIntegral`, the only other §5 unary operation, is
  combinatorially trivial and needs no sweep.

- The identity sweep closes the small exhaustive gap on the four
  `Decimal32` paths the Kani harnesses cannot reach, with no oracle
  dependency. The README verification section gains a bullet for the
  ADR-0034 gates.

- The named exposure in the README disclosure narrows again on
  `Decimal32`: the residual rounding error on a boundary case the
  empirical search did not surface now excludes sqrt as well as the
  §9.2 set. Per the standing disclosure invariants, the named failure
  mode edit requires per edit approval and is not made here.

- The binary arithmetic surface is explicitly out of scope and recorded
  as such, so the question is not relitigated from zero later. The
  consumer domain framing is deferred, not killed (see below).

- Latency and dependency posture are unchanged. The sqrt sweep runs
  offline, never a Cargo dependency, never in CI; the identity sweep is
  on demand. The kernel does not change.

- The shared infrastructure version posture is preserved per ADR-0029.
  Whether to mark the `Decimal32` sqrt contract tightening with a
  sibling minor bump (`ferrodec-decimal32` 2.2.0 to 2.3.0) is a per
  slice decision at sqrt gate landing time and is not pre committed
  here; the default is a docs and tests only commit with no version
  bump, matching ADR-0033 Slice C.

- ADR-0033 stays accepted. This ADR extends its evidence base to a new
  operation and a new identity surface rather than superseding any
  decision.

## Rejected and deferred

- **Pinned operand binary sweeps, stratified random binary sampling,
  and the consumer domain (STM32U class) box.** Deferred on the no TMD
  argument in Context: basic arithmetic has an exactly decidable
  correctly rounded oracle and a structurally bounded failure space, so
  exhaustion adds little over the existing property, fuzz, conformance,
  and Kani coverage; and the `~10^16` to `~10^18` binary pair space
  cannot be exhausted, so any binary sweep is sampling, not exhaustion.
  The consumer domain box has a further subtlety: within a precision
  capped box (for example four significant digits), `add` / `sub` /
  `mul` results land in eight or fewer digits, which the format
  represents exactly, so no rounding decision is exercised, and the
  box's marginal coverage collapses to `div` within a realistic range.
  One boundary biased stratified pass per operation remains a defensible
  future check if a concrete concern about a concentrated bug family in
  a narrow input region arises; absent that concern it is not adopted.
  These are deferred rather than killed: the consumer domain framing for
  the STM32U target is recorded here for a future engagement.

- **Extending the consumer domain box to `Decimal64` or `Decimal128`.**
  Deferred with the binary pieces. A precision capped box is the only
  exhaustive flavored evidence those formats could ever carry, which is
  the one genuine argument for it, but within such a box the wider
  formats represent the `add` / `sub` / `mul` results exactly, so the
  evidence concentrates on `div` and sqrt and is thin relative to the
  kernel replay work. Revisit only alongside a binary engagement.

- **Sweeping the Kani proven identities.** Rejected as redundant. The
  canonical predicate projection and idempotence, the BID and DPD
  encode and decode round trips, the same cohort total order
  antisymmetry, and the special case dispatch arithmetic identities are
  discharged symbolically for every input by the existing Kani
  harnesses; an exhaustive loop over the concrete `Decimal32` values is
  strictly weaker and adds nothing.

- **`Decimal64` or `Decimal128` exhaustive sqrt.** Rejected on the same
  cardinality grounds ADR-0033 records for the transcendentals: `10^16`
  to `10^36` canonical inputs per function is far past the exhaustive
  envelope. `Decimal64` and `Decimal128` sqrt keep their proptest
  envelope.

## Related

- Plan: `~/.claude/plans/i-want-to-scope-twinkling-clarke.md`
- Other ADRs: extends ADR-0033 (worst case margin completeness via
  exhaustive `Decimal32` enumeration; not superseded). Builds on
  ADR-0026 (independent oracle stack: Arb proof tier, MPFR gate),
  ADR-0021 (exact correctly rounded oracle), and ADR-0032 (correctly
  rounded transcendentals contract). The identity redundancy argument
  rests on ADR-0015 and ADR-0016 (Kani scope policy and shim routing):
  the arithmetic harnesses shim the alignment and rounding loops, which
  is exactly why the residue identities are unproven and the sweep is
  the right fallback.
- Beads: `fd-lem` (ADR-0034 umbrella), `fd-i9c` (Slice 1: ADR proposed
  plus sqrt tooling extension), `fd-k6s` (Slice 2: sqrt exhaustive
  campaign, default on kernel gate, MPFR cross check, ADR accepted),
  `fd-5nq` (Slice 3: identity residue exhaustive sweep).
- Citations:
  - IEEE 754-2019 §5.4.1. The mandatory `squareRoot` operation this ADR
    verifies exhaustively at `Decimal32`, correctly rounded by the
    standard's requirement, a stronger obligation than the §9.2
    recommended transcendentals.
  - Lefèvre, V. 2000, "Moyens arithmétiques pour un calcul fiable"
    (PhD thesis, École Normale Supérieure de Lyon). The exhaustive
    hardest case search method ADR-0033 ports to decimal and this ADR
    reuses for sqrt.
  - Muller, J.-M. "Elementary Functions: Algorithms and
    Implementation" (3rd edition, Birkhäuser 2016). The Table Maker's
    Dilemma treatment that distinguishes the algebraic and
    transcendental cases (which admit no a priori margin bound) from
    basic arithmetic (whose exact result is finitely computable), the
    distinction that scopes this ADR.
