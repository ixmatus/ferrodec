# ADR-0059: The provably correctly rounded decimal128 lane: bounded escalation ladder, falsification program, and §9.2 surface completion

- **Status**: accepted (mechanism outcome recorded 2026-08-02; see §Outcome)
- **Date**: 2026-07-27

## Context

### The quantitative confession

ADR-0032 committed every §9.2 transcendental on all three formats to
the correctly rounded contract, discharged by a fixed 50 digit
`Extended` kernel whose error budget clears the empirical worst case
half ULP margins from the Arb search corpus. ADR-0033 then proved the
claim exhaustively at `Decimal32`. `Decimal64` and `Decimal128` still
cite the sampled corpus: roughly 300 random draws per function with
coefficients capped at 8 digits, against input spaces of 10^16 and
10^38 values per function.

The standard equidistribution model (muller-handbook-2018, the
hardness of rounding chapters) prices what that leaves open. A 50
digit intermediate carries about half an ulp50 of representation
error; a `Decimal128` result misrounds when the true value lies
within that error of a 34 digit rounding boundary, a window of about
10^-15 of an ULP. Over roughly 10^38 finite canonical inputs per
function the expected number of misrounding inputs is on the order of
10^22, and higher still for trig, where the argument reduction
guarantees only 9 headroom digits. The shipped per input claim is
therefore almost surely false on undiscovered inputs, and a full
oracle sampling campaign deep enough to exhibit one witness (about
10^15 evaluations through certified ball arithmetic) is not
affordable. ADR-0033's own calibration points the same way: the
sampled corpus under estimated the true `Decimal32` worst case by a
median factor of 10^5 and by 7.8 million on `acosh`.

Three thin spots are already admitted in tree. The trig argument
reduction survives 43 digits, 9 over the format's 34
(`ferrodec-transcend/src/argred.rs`, `FRAC_DIGITS`). The 38 digit
`π/2` truncation carries an analytic bound of 10^-3 ULP, looser than
the sampled `Decimal128` `cos` margin of 4.051 × 10^-4 ULP, so the
trig discharge at `Decimal128` is empirical rather than analytic,
with the 80 digit U384 path deferred "until a failing high magnitude
case surfaces" (ADR-0032, honesty amendment). `exp` near the overflow
edge keeps about 11 headroom digits through the `k · ln 10` reduction.

### The two standing rejections, engaged verbatim

ADR-0024 rejected true provably correct rounding:

> correct decimal rounding requires solving the Table Maker's
> Dilemma, which needs proven hardest-case bounds for each
> transcendental at decimal64 and decimal128 widths. No such bounds
> exist in the literature, and deriving them is a research programme,
> not an engineering slice. A claim the project cannot discharge is
> worse than a weaker claim it can.

ADR-0032 rejected Ziv adaptive precision:

> Rejected on unbounded worst case latency: there is no proven worst
> case loop count for decimal Ziv, so the bound becomes a runtime
> parameter rather than a discharged invariant. A loop without a
> proven termination bound is incompatible with the STM32U class
> embedded target and conflicts with the verification first posture
> inherited from ADR-0024.

Both rejections were correct as stated, and both are narrower than
they look.

The Ziv rejection targets an *unbounded* loop. A compile time ladder
of exactly two rungs has a static worst case, the sum of two compile
time constants; nothing about it is a runtime parameter. ADR-0040
already carved the unbounded loop in for the heap crate on the ground
that the latency argument does not bind there; this ADR completes the
picture: the latency argument never bound a bounded ladder either.
The rejection is dissolved for the ladder, not overruled for the
unbounded loop, which stays confined behind an `alloc` feature.

The ADR-0024 rejection targets a claim that *requires* hardest case
bounds. The ladder requires none: its rungs are sound for every input
whose true value is not pathologically near a boundary, and the
classification leg (below) disposes of every input whose true value
is *on* a boundary. Hardest case knowledge, if it ever exists for
decimal128, would only calibrate the top rung's width. What remains
of ADR-0024's ground is exactly its literature claim, and this lane
deliberately accepts the research programme that claim declined:
decimal128 hardest case bounds are unpublished territory
(slz-worst-cases reaches binary64 sized domains and no further;
lefevre-stehle-zimmermann-d64-exp is the lone published decimal
worst case table, for one function at decimal64), and the lane's
research spike and campaign corpora are scoped as multi slice work
with stop losses, which is what "a research programme, not an
engineering slice" prescribes. ADRs are revisable, not law; this one
relitigates with new mechanism, not with a rereading.

### Why now

Nobody has a correctly rounded decimal128 elementary function
library: CORE-MATH (core-math) and RLIBM (rlibm) are binary only, and
IEEE 754-2019 §9.2 recommends exactly this surface. The claim is
already shipped; what this lane adds is a mechanism under which the
claim is true by construction rather than statistically false, an
evidence program that could have falsified it and did not (or did,
and then repaired it), and the missing §9.2 operations so the surface
is complete against the clause. Downstream, SMIL's calculator keys
each return the one correct answer, provably.

## Decision

### The tripod

Correct rounding by construction rests on three legs; each handles a
case the other two cannot.

1. **Input side exact and tie classification.** Inputs whose true
   value sits exactly on a rounding boundary (representable at the
   format width, or exactly on a nearest mode midpoint) are
   classified and short circuited before the kernel. No finite
   precision test can decide these; they must be proved. A nearest
   mode tie is exact representability at `PRECISION + 1` digits with
   final digit 5, so the existing exactness machinery (ADR-0047)
   generalizes by one digit of width and packs the exact coefficient
   through the format rounder, which resolves the tie, the directed
   sides, and `INEXACT` correctly by construction. The post hoc
   proofs in `exact.rs` whose soundness leans on the kernel already
   being correctly rounded are replaced with input side derivations:
   an integer cube root test for `cbrt`, and for `pow` the decimal
   analog of the Lauter and Lefèvre boundary analysis
   (lauter-lefevre-pow-boundary): with `y = ±a/b` in lowest terms and
   `x = 2^α · 5^β · m` with `m` coprime to 10, `x^y` is exactly
   representable only if `b | α`, `b | β`, and `m = t^b` for integer
   `t`, all decidable in fixed width integers without factoring. The
   transcendental functions admit no nonzero exact cases and no ties
   at representable inputs (Lindemann Weierstrass and Niven, cited
   per function in rustdoc); the enumerable ties are named and
   committed as vectors: `exp2(-49)` and `exp2(-50)` at `Decimal128`,
   `exp2(-23)` at `Decimal64`.

2. **Anchor residual theorems (ADR-0051, unchanged).** Results that
   hug a format grid point asymptotically (distances like 10^-6000
   relative) are decided by the side theorem through `sticks_to` and
   `to_format_with_residual`. No finite rung separates these; the
   seam stays load bearing and runs before the ladder's predicate.

3. **The bounded escalation ladder.** For everything else:

   - **Rung 1** is today's tuned 50 digit `Extended` path, byte
     identical on every call that does not escalate.
   - **The predicate.** After the anchor check and before rounding,
     `Extended::near_rounding_boundary::<F>(budget)` tests whether
     the intermediate lies within the function's total error budget
     (in ulp50 units) of either boundary family: the format grid
     point or the nearest mode midpoint, in every rounding mode. The
     predicate mirrors the drop position arithmetic of the format
     rounder exactly and is mode independent, so escalation is a
     deterministic function of `(f, x)`; this also decides the
     tininess side at the subnormal edge whenever the value is
     decided. Testing both families roughly doubles an escalation
     rate of 10^-14, which costs nothing and buys determinism.
   - **Rung 2** recomputes at 110 digits: `Extended2` with a `U384`
     coefficient, a new `U768` for products, three Newton steps, wide
     constants, and `reduce_wide` for trig (143 fraction digits, 110
     surviving; the shared 6300 digit `2/π` table extends to 6400
     digits; a 115 digit `π/2`). This delivers the deferred 80 digit
     U384 `π/2` path as the escalation target while rung 1 stays
     untouched; rung 1's trig truncation looseness moves into rung
     1's honest budget instead of into a claim. Worst case latency is
     the sum of the two rungs, roughly 6 to 9 times today's cost, a
     compile time constant. Kernel bodies become generic over a
     `pub(crate) trait ExtNum` so one algorithm serves both rungs;
     the `Extended` implementation delegates to the existing methods
     verbatim.
   - **Rung 3, feature gated.** Behind an `alloc` feature (name
     settled with Parnell at its slice; naming is identity bearing),
     the ladder tops out in an unbounded Ziv loop doubling from about
     220 digits on a heap backed working type. Termination for non
     boundary truths is a theorem given leg 1's completeness; builds
     with the feature carry the unconditional claim. The default
     build stays `no_std`, alloc free, statically bounded.

### The claim ladder

Three tiers, stated in rustdoc and the README in this order:

- **Tier 0, unconditional.** Every result lies within the top rung's
  quantified bracket of the true value. Strictly stronger than the
  ADR-0024 faithful contract.
- **Tier 1, by construction.** Correctly rounded, conditional on two
  auditable premises: the per function error budgets are sound
  (analytic, itemized per amplification term, padded by a factor of
  ten; the padding is nearly free because the escalation rate is
  linear in the budget), and the classification of leg 1 is complete.
- **Tier 2, model.** Under the equidistribution model the expected
  number of Tier 1 exceptions over the full `Decimal128` domain is
  about 10^-36 per function, against about 10^22 for the kernel this
  ADR replaces. Builds with the unbounded rung have no exception set.

Budgets are sound, not tight. The ADR-0050 failure was the error
model, not the margins; budgets are therefore rederived per function
from op counts with the known amplification terms (the `k · ln 10`
edge, the `y · ln x` coupling, argument reduction survival) itemized
in rustdoc, and a campaign harness audits each budget empirically
over the historically falsifying bands. An unsound budget reopens
exposure only on that function; this containment is real but
partial, and the audit exists because of it.

### The verification program

Falsification first, all local compute until the probe results
justify more (settled fork).

- **S1, the falsification probe, against the current kernel before
  the mechanism merges.** A `publish = false` workspace member
  `ferrodec-campaign` drives the shipped kernel's public `Extended`
  intermediates over the three thin spots (high decade trig with
  full 34 digit coefficients, the exp family overflow and underflow
  edges, the `pow` edge strip), about 10^9 samples per function
  overnight. The kernel is its own filter: only intermediates within
  10^-6 ULP of a boundary go to Arb certification, which makes the
  campaign affordable; a mandatory unconditional substream of 10^5
  to 10^6 fully certified random samples per function guards against
  the correlated failure shape where the filter and the kernel share
  a defect. A found misround is committed with provenance, fires the
  U384 tripwire, and goes to Parnell before any disclosure edit; a
  clean run at depth becomes a rule of three bound and the margin
  histograms. Either outcome is the deliverable.
- **S2, deep margin corpora.** The same driver at overnight scale on
  the priority set, landing the hardest certified rows as committed
  corpora with exact per bucket pins, a manifest (seeds, thresholds,
  git SHA), and the disclosure wording diff in the same change.
- **External anchors.** The published decimal64 `exp` worst cases
  (lefevre-stehle-zimmermann-d64-exp) are recertified through our
  own Arb pipeline and committed as the one externally grounded
  worst case corpus in any decimal format; Cowlishaw's transcendental
  decTest vectors, currently wired only to `ferrodec-decimal`, gain a
  `Decimal128` replay gate for the precision 34 subset.
- **S4, the oracle floor.** The ADR-0051 sub 10^-100 directed
  corrections get their raised precision Arb certification pass, and
  the nearest only pins upgrade to all modes.
- **S3, escalation aware tests, after the mechanism lands.** A
  planted corpus that provably forces rung 2; escalation depth
  telemetry behind a test feature with exact per bucket pins; a
  `force-escalate` configuration that routes the entire existing
  corpus through rung 2 and demands byte identity with rung 1's
  verdicts; a `ladder-audit` configuration that panics on top rung
  residual ambiguity; and the repository's first non push CI
  workflow (manual dispatch plus weekly) carrying the differential,
  MPFR gate, force escalate, and campaign smoke lanes.

### The research spike and the surface completion

A timeboxed spike (one to two weeks) asks whether published effective
transcendence measures yield finite provable ladder caps: the
explicit irrationality measure of π
(salikhov-zudilin-pi-irrationality) for huge argument trig, Matveev's
explicit linear forms in logarithms (matveev-2000) for the exp, log,
`pow`, and `atan2` boundary families, and Shidlovskii's E function
measures (shidlovskii-transcendence) for the entire function class.
The deliverable is a memo with reference entries, negative results
written, and a mandatory not specialist verified banner; no ADR cites
a derived cap as a theorem without priced external verification. The
algebraic operations need no spike: Liouville type bounds from
minimal polynomials give unconditional caps.

The 17 recommended §9.2 operations ferrodec lacks (`logp1`,
`log2p1`, `log10p1`; `expm1`, `exp2m1`, `exp10`, `exp10m1`; `pown`,
`powr`, `rootn`, `compound`, `rSqrt`, `hypot`; `sinPi` through
`atan2Pi`) join the lane in four groups in that order, implemented on
the generic kernels so each inherits the ladder from day one. The π
scaled family goes last: it is the largest unknown, and a stall there
strands nothing.

### Execution discipline

The refactor sequence is gated so the tuned kernel cannot be
perturbed silently: the commit introducing the `ExtNum` trait changes
zero call sites, and the commit swapping kernel bodies to generic
code must pass the full corpus, anchor bands, the `Decimal32`
exhaustive suite, and the MPFR gate with zero diffs before any rung 2
code exists. A byte identity failure at that gate stops the lane for
a write up; it is not a fix forward gate. Versioning follows the
minor precedent (ADR-0032's 2.1.0, the 3.4.0 value repairs), with the
enumerable tie value changes named in the CHANGELOG; the final call
stays with the release conversation. The README disclosure's named
failure mode is edited only with per edit approval, invariants
verbatim.

## Consequences

- The three most plausible ways this is wrong, inverted up front.
  First, the genericization quietly perturbs rung 1, the one code
  path every existing witness attests; the zero diff M3/M4 gates are
  structural, and the lane stops on failure. Second, a budget
  constant is unsound, which reopens today's exposure on that
  function; the rederivation, tenfold padding, and empirical budget
  audit exist because transcribing today's rustdoc prose would
  repeat ADR-0050. Third, the escalation path rots unexecuted at a
  10^-14 firing rate; the planted corpus, the force escalate byte
  identity differential, and the weekly workflow are the standing
  counters, and the telemetry pins catch threshold drift in either
  direction.
- Latency: the common path is byte identical and bench guarded
  neutral; worst case is a compile time 6 to 9 times; `Decimal128`
  trig averages a few percent slower until its budget tightens,
  and that number is printed honestly in the mechanism slice's
  CHANGELOG entry alongside the thumbv6m size delta.
- The named exposure in the README narrows from "boundary inputs the
  sampled search did not surface" to the Tier 1 premises (a budget
  audit failure or a classification gap) plus the Tier 2 model
  number; the edit is approval gated as always.
- The sampled corpus stops being the binding evidence for
  `Decimal64` and `Decimal128`; the campaign corpora, the external
  anchors, and the ladder's construction replace it. The
  `sticks_to` doc comment's citation of the ADR-0033 empirical
  margin floor gets regrounded or amended by the S2 results.
- ADR-0032's §Rejected alternatives Ziv paragraph is amended by this
  ADR (bounded ladder adopted; unbounded loop stays rejected outside
  the `alloc` feature); its proof posture section is superseded for
  `Decimal64`/`Decimal128` by the tripod. ADR-0024's narrowed
  rejection is relitigated as recorded above. ADR-0040 is untouched:
  its crate keeps its own Ziv strategy and its boundary statement
  remains true.
- The lane accepts research risk with stop losses: the spike is
  timeboxed with the memo as deliverable, `pow` classification
  carries its own timebox, and a probe find is a deliverable, not a
  failure.

## Outcome (recorded 2026-08-02; mechanism arc M1 through M8b)

The lane's falsification program and mechanism arc are complete. This
section records what happened against the charter above: the probe
verdict, the deviations worth naming, the measured costs that replace
the charter's estimates, and the honest boundary of the proof. The
witness corpus, the mechanism, this record, and the README edit land
together in one signed merge; nothing below is public until then.

### The probe verdict

S1 falsified the shipped claim, as the equidistribution model
predicted it would. The probe produced 1 819 Arb certified misround
witnesses against the shipped Decimal128 trig kernel (sin 643,
cos 570, tan 606): high decade inputs with full 34 digit
coefficients, exactly the band the sampled corpus never reached. The
witnesses are committed with provenance and replay as a pinned
regression gate with exact per file counts; under the ladder every
row rounds correctly, and the force escalate and force rung 3
configurations replay the corpus byte identically through each upper
rung. The exp family and pow edge probes surfaced no misrounds, but
the classification leg repair (M7) fixed live directed mode defects
the probe was not aimed at: cbrt on perfect cubes and pow on exact
rational powers returned neighbor values with spurious INEXACT at
TowardZero and TowardNegative, and the enumerable exp2 ties resolved
by kernel noise rather than by the tie rule.

### Deviations from the charter worth naming

- **Trig escalation is common, not rare.** The charter's 10^-14
  escalation shape assumed budgets near the working error. Rung 1's
  honest trig budget instead carries the 38 digit π/2 truncation as
  its dominant item (the fd-aqs.10 analytic bound, 10^13 predicate
  units), so about 3% of full range random Decimal128 trig calls
  escalate, about 6% for tan. This is the charter's own "rung 1's
  trig truncation looseness moves into rung 1's honest budget"
  clause, quantified; the tightening target is the rung 1 reduction
  bound, not the pad.
- **The unbounded rung's support is computed, not stored.** Stored
  constants cannot follow the Ziv doubling, so rung 3 computes π,
  2/π windows, the logarithm family, and the reciprocals at the
  requested depth, each generator carrying a derived error bound and
  oracle pins; the working type is a Copy handle into a per attempt
  arena (the ExtNum seam's exemplar receiver carries the arena and
  the precision); the runtime reduction reads its 2/π window at
  depth q + p + 70, reproducing the fixed rung constants exactly at
  p = 110. The plan document's "Implementation resolutions" section
  records each fork and why.
- **The program's instruments caught a real mechanism defect.** The
  sinh and cosh saturation proxies fed the guarded delivery instead
  of the format rounder, contradicting the module doc's unguarded by
  design list. A proxy's one digit coefficient sits exactly on a
  working grid point at every precision, so every saturating call
  paid a silent rung 2 re run, a ladder audit build panicked on
  saturating Decimal64 and Decimal32 inputs, and the unbounded rung
  turned the waste into an unbounded widening loop, which is how the
  widened M8b gate battery surfaced it. The audit lane had only ever
  run on Decimal128, whose saturation region random samplers barely
  reach; it now runs on all three formats, and the gate script's
  pipeline masking defect found in the same investigation is fixed.
  This is the Consequences section's third inversion realized in
  miniature, caught by exactly the instruments it prescribed.

### Measured costs (replacing the charter's estimates)

- Typical input predicate tax (criterion, measured at M8): sin and
  cos about +1.5%, pow +0.7%, the exp and log family +5.6 to +6.3%.
- Full range random Decimal128 trig (20 000 deterministic inputs
  spanning the whole exponent range, host, release): sin 116 to
  152 µs per call (+31%), cos 116 to 153 µs (+32%), tan 120 to
  197 µs (+64%). Consistent with the measured escalation rates times
  a roughly tenfold rung 2 cost in the deep window regime. The
  charter's Consequences said "a few percent"; these numbers replace
  it, accepted on the correctness first ground that a correct
  algorithm can be tightened later while an incorrect one causes
  damage now.
- The `unbounded-ladder` feature adds no measurable cost when the
  third rung is not entered (within 0.3% run noise on the same
  sweep); entry probability is the Tier 2 model figure.
- Worst case latency for default builds remains the compile time sum
  of the two fixed rungs, roughly 6 to 9 times rung 1.
- thumbv6m size, measured as the family rlib `.text` plus `.rodata`
  totals with LTO disabled for measurability (pre link, so an upper
  bound on the linked delta): the default `transcendentals` build
  grows from 324 KB to 408 KB (+83 KB, +26%), which is the rung 2
  mirror kernel, U768, and the wide reduction; the `unbounded-ladder`
  build compiles for the target at 595 KB but requires an allocator
  to be useful.

### Where the proof stands

The tier claims hold as chartered, with the boundary between proof
kinds now explicit. Analytic, in rustdoc at the site: the per
function budget itemizations, the reduction truncation discharges
(rung 2 and rung 3 analytic, rung 1 trig empirical by its honest
budget), the runtime generator bounds, and the window discard
congruence that makes the generator's unit error unamplifiable.
Empirical, in committed harnesses: the S1 band budget audit (rung 1
error under a tenth of budget over the falsifying bands), the byte
identity differentials, the cross substrate differential at 110
digits, and the oracle pins. Model, stated as model: the Tier 2
residual rate, and the transfer of the budget itemizations to
arbitrary p, which is pinned against the fixed catalog at p = 110
and argued, not proved, beyond it. Two permanent asymmetries are
accepted and named: the Ziv doubling arm is reachable only at the
Tier 2 rate, so its only executable witness is synthetic; and
termination of the unbounded rung is a theorem only given the
classification leg's completeness, with the constant generators'
depth cap turning any pathology into a loud panic rather than a
wrong delivery.

### Still open in the lane

S2 deep margin corpora, S3 planted corpus, telemetry pins, and the
weekly CI workflow, S4 anchor floor certification, the S5
transcendence measure spike, the Track D surface groups, and the S6
close out (verification map, testing.md frontier, KNOWN_ISSUES
posture). The README disclosure edit is drafted under per edit
approval and lands only with the atomic merge.

## Related

- Plan: `plans/2026-07-25-correctly-rounded-d128-transcendentals-lane.md`
- Beads: epic `fd-4zo`; children `fd-4zo.1` through `fd-4zo.28`.
- Other ADRs: amends ADR-0032 (Ziv rejection paragraph; d64/d128
  proof posture); relitigates the ADR-0024 narrowed rejection; builds
  on ADR-0026 (oracle stack), ADR-0033/0034 (exhaustive d32 and the
  TMD scoping lens), ADR-0047 (exactness machinery), ADR-0050 (error
  model repair), ADR-0051 (anchor residual seam); coexists with
  ADR-0040 (heap crate Ziv).
- References (registry slugs): lefevre-2000, muller-handbook-2018,
  muller-elementary-functions, ziv-1991, crlibm, core-math, rlibm,
  slz-worst-cases, lefevre-stehle-zimmermann-d64-exp,
  lauter-lefevre-pow-boundary, matveev-2000,
  shidlovskii-transcendence, salikhov-zudilin-pi-irrationality,
  ieee-754-2019, payne-hanek-1983, arb-flint, mpfr.
