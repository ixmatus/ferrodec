# Lane plan: provably correctly rounded Decimal128 transcendentals

Lane charter and execution plan for the arc that becomes ADR-0059. Archived in
tree so the intent is reconstructable from the repository alone. This is a
lane (multiple slices, spikes, and stop losses), not a slice.

Settled forks (Parnell, 2026-07-25): the mechanism is a bounded two rung
escalation ladder plus a feature gated unbounded top rung; compute stays
laptop only until probe results justify cloud spend; the research spike runs
now with the publication decision deferred until the corpus exists; all four
scope riders are in (Decimal64 rides along, decTest transcendental replay
against Decimal128, Decimal32 adopts the ladder uniformly, and the full §9.2
surface completion joins the lane).

## Context

IEEE 754-2019 §9.2 recommends correctly rounded elementary functions; no
decimal128 library achieves this provably (CORE-MATH and RLIBM are binary
only; decimal128 hardest to round cases are unpublished). ferrodec already
ships the claim (README Accuracy section, ADR-0032) on a fixed 50 digit
kernel (`ferrodec-transcend/src/extended.rs`, `EXT_PRECISION`), proven
exhaustively at Decimal32 only (ADR-0033/0034); Decimal64 and Decimal128 rest
on a 300 draw sampled corpus whose coefficients cap at 8 digits. Under the
equidistribution model the 50 digit kernel statistically misrounds about
10^22 of the roughly 10^38 Decimal128 inputs per function; the shipped claim
is almost surely false on undiscovered inputs and unfalsifiable by affordable
full oracle sampling. The admitted thin spots: trig argument reduction keeps
only 9 headroom digits (`argred.rs`, `FRAC_DIGITS`), the 38 digit π/2
analytic bound is looser than the sampled cos margin (the U384 path is
deferred on a named tripwire), and exp keeps about 11 headroom digits at the
overflow edge.

This lane refounds the claim as correct by construction and deliberately
accepts the research programme ADR-0024 declined; the charter ADR relitigates
that rejection explicitly (ADRs are revisable, not law). Downstream, SMIL
becomes the first calculator whose every function key provably returns the
one correct answer, which motivates the full surface rider.

Prior constraints honored: Decimal128 exhaustion stays rejected (ADR-0033);
the unbounded Ziv rejection (ADR-0032) is dissolved for the default build by
a static two rung bound and confined to an opt in `alloc` feature for the
unbounded rung; ADR-0050's lesson (the error model fails, not just the
margins) drives kernel versus oracle testing at scale.

## Track B: the mechanism (tripod plus ladder)

Correct rounding by construction rests on a tripod; each leg handles what the
other two cannot.

1. **Input side exact and tie classification** handles boundary equality
   truths.
2. **ADR-0051 anchor residual theorems** handle asymptotic grid hugging
   (distances like 10^-6000 relative, which no finite rung separates);
   `sticks_to` and `to_format_with_residual` stay load bearing and run before
   the predicate.
3. **The escalation ladder** handles generic near boundary cases:
   - Rung 1 is today's tuned 50 digit `Extended` path, byte identical on all
     calls that do not escalate (roughly 1 − 10^-14 of calls; about 1 − 4e-3
     for Decimal128 trig until the budgets tighten).
   - A mode independent predicate `Extended::near_rounding_boundary::<F>`
     mirrors the drop position arithmetic of `src/ops/round.rs` and tests
     both boundary families (grid point and midpoint) in every mode, so
     escalation is a deterministic function of the input alone; this also
     closes the subnormal edge tininess flag hazard.
   - Rung 2 is `Extended2 { coef: U384 }` at 110 working digits with a new
     `U768` for products, plus `reduce_wide` (`FRAC2 = 143`, 110 surviving
     digits; the shared 2/π table extends 6300 → 6400 digits; a 115 digit
     π/2 constant). This delivers the deferred U384 π/2 path. The worst case
     is T1 + T2, roughly 6 to 9 times today's latency, a compile time
     constant.
   - Rung 3 (feature gated, `alloc`) is unbounded Ziv doubling from about 220
     digits on a heap backed working type (`DecBig` substrate;
     `ExtNum::precision()` becomes a method so the fixed types constant
     fold). Termination is a theorem for non boundary truths given
     classification completeness; feature builds carry the unconditional
     claim modulo budget soundness. The feature name is decided with Parnell
     at ADR time (naming is identity bearing). The default build without
     `alloc` claims Tiers 1 and 2 below.

Kernel bodies become generic over a `pub(crate) trait ExtNum` (associated
constants for series caps; three Newton steps at rung 2; the `Extended` impl
delegates verbatim, following the house precedent of the `DecimalFormat`
genericization proven byte identical). The series loops already terminate on
`next_sum == sum`, so they widen for free.

**The claim ladder** (ADR and README wording):

- Tier 0, unconditional: every result lies within the top rung bracket of the
  true value, strictly stronger than faithful.
- Tier 1, by construction: correctly rounded, conditional on per function
  budget soundness (analytic, itemized, padded by a factor of ten; sound not
  tight is cheap because the escalation rate is linear in the budget) and on
  classification completeness.
- Tier 2, model: expected Tier 1 exceptions about 10^-36 per function over
  the full Decimal128 domain (against about 10^22 for the kernel as shipped).
  Builds with the unbounded rung have no exception set at all.

**Exact and tie classification completion** (`exact.rs`): a nearest mode tie
is exactly "representable at p + 1 digits with final digit 5", so the width
gates generalize to `F::PRECISION + 1` and pack through the format rounder,
which resolves ties, directed sides, and INEXACT correctly for free. The post
hoc circular proofs (documented in the module header) are replaced input
side: `cbrt_exact_input` (integer cube root; provably no ties) and
`pow_exact_input` (decimal analog of Lauter and Lefèvre: `x^(a/b)` is exact
iff `b | α`, `b | β`, and `m = t^b`, decidable in `U256` without factoring).
Named tie vectors: `exp2(-49)` and `exp2(-50)` at Decimal128, `exp2(-23)` at
Decimal64. The transcendental functions have no ties (Lindemann–Weierstrass
and Niven, cited per function in rustdoc). Constants delivered directly (the
π/2 and π quadrants in `inverse_trig.rs`) get a one time offline boundary
distance certification in the `consts.rs` tests rather than the runtime
predicate.

## Track A: verification program (all local compute)

- **S1, the falsification probe.** Runs first, against the current kernel,
  before the mechanism merges. A new workspace member `ferrodec-campaign/`
  (`publish = false`, outside default members); no production changes are
  needed because `Extended`'s fields and the `*_extended` entry points are
  already public. The boundary distance primitive in Rust is lockstep tested
  against the Python margins through the shared `round_half_even` case table
  (the `_selftest` discipline). Targets: high decade trig (10^15 to 10^6140,
  full 34 digit coefficients, never sampled before), the exp family overflow
  and underflow edges, and the pow edge strip; about 10^9 samples per
  function overnight on the laptop. The filter escalates to Arb when either
  boundary distance falls below 1e-6 ULP; survivors are certified by a new
  `tools/campaign_certify.py` reusing `gtv.solve` and `_decisive` (cap hits
  are genuine TMD hard finds and get the ADR-0033 discipline). A mandatory
  unconditional substream sends 10^5 to 10^6 random samples per function
  through full Arb regardless of margin; it is the counter to the correlated
  failure shape (the filter shares the kernel's code paths) and is never
  trimmed for budget. Mirror versus production rounded equality is asserted
  on every sample. Determinism comes from a counter mode PRNG keyed on
  (campaign, function, format, shard, index): a checkpoint is one integer,
  resume is O(1), aggregation is idempotent.
  On a find: commit the witness with Arb provenance; the U384 tripwire fires
  (the rung 2 argument reduction slice becomes required); surface to Parnell
  before choosing the disclosure path (policy on a shipped claim is his).
  On no find at 10^10: a rule of three bound below 3e-10 per input in the
  probed strata, margin versus decade histograms committed, and the ADR
  records the result. Abort criteria: disagreement above 1e-3 in the
  unconditional substream means a harness bug (stop, fix, restart);
  calibration off by more than ten times means rescope before spending.
- **S2, the local deep campaign (Decimal64 and Decimal128).** The same driver
  at overnight scale (10^9 to 10^10 per function on the priority set: sin,
  cos, tan, exp, ln, pow, log10, sinh, cosh at Decimal128; exp, sin, cos at
  Decimal64). The hardest rows (at most 50 per function and format) land as a
  committed corpus under `tests/vectors/transcend/campaign/` with a
  `MANIFEST.json` (campaign id, N, threshold, seeds, git SHA; citable later),
  a `load_campaign` loader and exact `EXPECTED_BUCKETS_CAMPAIGN_*` pins in
  `frozen.rs`, and replay tests at root and Decimal64. SHA256SUMS land in the
  same commit. The disclosure wording diff ships in the same PR as the
  corpus. The campaign also regrounds or stresses the `sticks_to` doc's
  empirical margin citation; that comment is updated either way. Cloud
  deepening (the previously scoped $100 and $1k tiers, with the runner
  scripts kept in tree under `tools/campaign_aws/` this time) stays a
  deferred, gate approved option once S1 calibrates the cost model.
- **The LSZ external anchor.** Lefèvre, Stehlé, and Zimmermann's published
  decimal64 exp worst cases are the only externally certified worst case
  table in any decimal format. License check first (pointer only expected;
  the worst case values themselves are mathematical facts); every row is
  recertified through our own Arb pipeline (`tools/gen_lsz_d64_exp.py`); the
  corpus lands at `tests/vectors/transcend/external/` with a Decimal64 replay
  test. The comparison of our sampled Decimal64 exp minimum against the LSZ
  true worst case becomes the second calibration datum beside ADR-0034's
  Decimal32 one. Highest information per dollar in the program; zero compute.
- **decTest transcendental replay against Decimal128.** The vendored exp, ln,
  log10, and power vectors are wired only to the GDA crate today; the p = 34
  compatible subset gets a root crate conformance gate with exact per file
  pins.
- **S4, oracle floor closure.** `tools/certify_anchor_floor.py` certifies the
  ADR-0051 sub 1e-100 directed corrections with Arb (the existing 65536 bit
  cap has sixty times headroom), upgrades the nearest only anchor band pins
  to all modes, and closes the note with an ADR-0051 addendum. If a certified
  line disagrees with the residual seam, that is a real defect: the fix
  precedes the pin and a failure class reference entry is written.
- **S3, escalation aware tests (after the mechanism lands).** A planted rung
  2 forcing corpus (S1 and S2 survivors plus synthetic near boundary
  constructions by boundary inversion and last digit local scan, each forward
  certified by Arb and asserted canonical); escalation depth telemetry (a
  `telemetry` feature, atomics, off by default) with exact per bucket pins
  over the planted and existing corpora (a drift tripwire in both
  directions); a `force-escalate` cfg that runs the entire existing corpus
  through rung 2 demanding byte identity with the rung 1 verdicts (the anti
  rot differential); a `ladder-audit` cfg that panics on top rung residual
  ambiguity. A new `.github/workflows/verification.yml` (workflow_dispatch
  plus a weekly cron) carries force rung 2, differential, mpfr gate, and
  campaign smoke lanes; push CI stays untouched.

## Track C: research spike

S5 is timeboxed to one or two weeks; the deliverable is a feasibility memo
plus reference entries, with negative results written and a "not specialist
verified" banner mandatory (consult pricing is a later fork). Order: first,
huge argument trig via the explicit irrationality measure of π (Salikhov
2008; Zeilberger and Zudilin 2020), which yields a finite cap near 10^4
digits on exactly the S1 thin spot; second, Matveev's explicit linear forms
in logarithms for the exp, ln, pow, and atan2 boundaries (atan reduces to
logarithms of Gaussian rationals; verify that reduction carefully); third,
Shidlovskii E function measures, where the likely honest finding is effective
but not explicit, and that gets written as the negative result. The algebraic
operations in the surface rider (hypot, rSqrt, pown, rootn, compound, sqrt,
cbrt) need no spike: Liouville type bounds from minimal polynomials give
cheap unconditional ladder caps, and that lemma folds into the surface
track's ADR. Publication (repository docs, arXiv note, or ARITH) is decided
after the S2 corpus exists; the manifest discipline means no rerunning either
way.

## Track D: §9.2 surface completion (after M4 lands)

Seventeen missing recommended operations, implemented on the generic `ExtNum`
kernels so each inherits the ladder from day one. Order: logp1, log2p1,
log10p1 first (the internal `log1p_extended` already exists); then expm1,
exp2m1, exp10, exp10m1 (cancellation aware near 0, anchor residual
treatment); then the algebraic group pown, powr, rootn, compound, rSqrt,
hypot (exactness fully decidable, provable caps per Track C); last the π
scaled family sinPi through atan2Pi (rich exact case tables via Niven;
argument reduction is exact rational, no Payne–Hanek needed). Every operation
ships with kernel, classification, corpus vectors, property tests, a rustdoc
Accuracy block, and a disclosure row. Four groups in that order; each group
is one merge.

## Slice and merge sequence

Unsigned commits on each branch; one signed merge per slice; prompt before
every YubiKey touch.

0. **S0 charter**: ADR-0059 (lane charter plus mechanism, engaging the
   ADR-0024 and ADR-0032 rejections verbatim, the tripod, the claim tiers,
   and the quantitative confession as the opening); reference accretion
   (slugs: `lefevre-stehle-zimmermann-d64-exp`, `slz-worst-cases`,
   `core-math`, `rlibm`, `lauter-lefevre-pow-boundary`, `matveev-2000`,
   `shidlovskii-transcendence`, `salikhov-zudilin-pi-irrationality`; a table
   maker's dilemma glossary entry; Wayback archive each source at citation
   time); the beads decomposition of this plan.
1. **S1 probe arc** (four beads: crate plus primitive plus lockstep test;
   sampler CLI plus calibration; certifier plus substream; probe run plus
   corpus plus the outcome dependent ADR twin). S4 (one bead) and the LSZ
   anchor plus decTest replay (two beads) run in parallel; all are
   independent of the mechanism.
2. **Mechanism arc** M1 through M9, commit sized: M1 `U768` in
   `ferrodec-multiword`; M2 the predicate plus tests; M3 the `ExtNum` trait
   with zero call site diffs; M4 kernel bodies generic, gated on byte
   identity across the full corpus, anchor bands, the Decimal32 exhaustive
   suite, and the MPFR gate (zero diffs or stop the lane); M5 `Extended2`;
   M6 `reduce_wide` plus the table extension; M7 the `exact.rs` completion
   plus tie vectors; M8 wiring the ladder with per function budget constants
   (rederived, itemized, padded tenfold) plus the `force-escalate` and
   `ladder-audit` cfgs; M8b the unbounded rung feature; M9 ADR finalization,
   rustdoc tier language, the README disclosure edit (per edit approval with
   a diff preview), bench before and after, the thumbv6m size delta, and the
   version bumps.
3. **S2 local campaign plus S3 escalation tests** (after the mechanism).
4. **Track D surface groups one through four** (interleavable with 3).
5. **S5 spike** (any time; the natural slot is alongside overnight campaign
   runs).
6. **S6 close**: the verification map debt row, the testing.md frontier
   rewrite, the KNOWN_ISSUES posture, and memory plus roadmap updates.

Versioning (the release conversation decides; precedent is minor): ferrodec
4.0.0 → 4.1.0 with the siblings in lockstep, ferrodec-transcend 0.2.0 →
0.3.0, ferrodec-multiword 0.1.0 → 0.2.0. The CHANGELOG names the tie value
changes and any S1 witnesses explicitly. Track D additions are minor too.

## Critical files

- `ferrodec-transcend/src/extended.rs` (predicate; `ExtNum` seam;
  `Extended2` mirror source)
- `ferrodec-transcend/src/exact.rs` (input side cbrt and pow; the p + 1 tie
  generalization)
- `ferrodec-transcend/src/argred.rs` (`reduce_wide`; 6400 digit table; wide
  π/2)
- `ferrodec-transcend/src/{exp,ln,sincos,inverse_trig,hyperbolic,pow,cbrt}.rs`
  (generic bodies plus 22 predicate call sites; Track D modules beside them)
- `ferrodec-multiword/src/` (new `u768.rs` mirroring `u512.rs`)
- `src/ops/round.rs` (read only: the contract the predicate mirrors)
- `ferrodec-campaign/` (new), `tools/campaign_certify.py`,
  `tools/campaign_aggregate.py`, `tools/certify_anchor_floor.py`,
  `tools/gen_lsz_d64_exp.py`, `tools/gen_planted_hardcases.py`
- `ferrodec-test-support/src/frozen.rs` (`load_campaign` and `load_planted`
  plus bucket pins)
- `docs/decisions/0059-*.md` plus per track ADRs; `docs/references/` (eight
  new entries); `.github/workflows/verification.yml` (new)

## Lane wide verification gates

- Every slice keeps the full existing suites green (`--features
  transcendentals` on the three sibling crates plus root; corpus, anchor,
  exhaustive, and decTest pins; fmt, clippy, and rustdoc at `-D warnings`).
- The M4 byte identity gate is a stop the lane gate, not a fix forward gate.
- New pins are exact per bucket counts, never floors; corpus bytes, SHA256SUMS,
  and pins land in the same commit; the cap hit exits nonzero discipline
  applies everywhere.
- A bench guard keeps rung 1 latency neutral (criterion before and after, per
  the ADR-0032 precedent); the honest average latency number for trig is
  printed in the ADR.
- The force escalate byte identity differential, the planted corpus
  escalation pins, and the weekly verification workflow keep the ladder from
  rotting.
- Oracle gotchas honored: `copy_abs` plus `localcontext` for CPython decimal;
  libmpdec is correctly rounded under HALF_EVEN only; `mp.libmp.to_str` past
  30 digits; astro-float's sound magnitude domain (never a tier 1 filter).

## Stop losses and named risks

- M4 byte identity failure: stop, write up, replan; do not push through.
- Budget unsoundness (the ADR-0050 shape): budgets are rederived per function
  with the amplification terms itemized, padded tenfold, and audited by a
  campaign harness asserting the rung 1 error stays under a tenth of the
  budget over the historically falsifying bands. An unsound budget reopens
  exposure only on that function; the ADR says so plainly.
- The S1 silent miss shape (the filter shares the argument reduction path):
  countered only by the unconditional substream, which is a protected line
  item.
- The pow classification and the S5 spike carry their own timeboxes; the
  negative write up is the deliverable.
- The π scaled family is the largest unknown in the surface rider; its group
  goes last so a stall strands nothing.

## M8b design resolutions (settled with Parnell, 2026-07-31)

Two forks settled and one constraint discovered during M8b scoping;
recorded here so the implementation session inherits decisions, not
open questions.

**Feature name: `unbounded-ladder`** (Parnell's call; identity
bearing). Pulls in `alloc`; the README tier sentence reads "builds
with `unbounded-ladder` have no exception set."

**Constants: computed at runtime** (Parnell's call). The stored
support cannot follow the Ziv doubling: the nine working constants
exist at 55 and 115 digits only, and the 2/π table (6 408 digits)
covers a 220-digit rung-3 reduction at every exponent (6 111 + 220 +
~41 ≈ 6 372) but not the second doubling at high exponents. Capping
the rung would contradict the merged ADR's "no exception set"
language, so the rung computes its own support in `DecBig`:

- π by a Machin-type formula (`16·atan(1/5) − 4·atan(1/239)`, arctan
  as scaled-integer Taylor with an explicit tail bound); 2/π windows
  by dividing computed π to depth `q + p + ~45`.
- `ln 2 = 2·atanh(1/3)`, `ln 3 = 2·atanh(1/2)`,
  `ln 10 = 2·ln 3 + 2·atanh(1/19)`; `e` by `Σ 1/k!`;
  `tan(π/8) = √2 − 1` via `DecBig::isqrt`; the reciprocal constants
  by Newton against the computed originals.
- Every generator carries its own truncation-plus-rounding bound,
  folded into the rung's budget formula `budget(p)` (the M8
  itemizations re-evaluated at precision `p`, plus the generator
  items), and is pinned against stored mpmath oracle strings at
  several depths (the M5 constants discipline). Generator speed is
  irrelevant: rung 3 entry is ~10^-71 per call; only correctness
  matters, exercised by a `force_rung3`-style test lane.

**The seam constraint (discovered, resolution chosen).** Rust
monomorphization cannot instantiate unboundedly many precisions from
const generics, so dynamic precision must be a runtime value — and
the current `ExtNum` constructor surface (`ONE` / `HALF` as consts,
`from_format` / `parse_str` / the nine constant fns as statics) has
no slot to carry it into a kernel body. A scoped precision global
was considered and rejected: ambient state in a pure kernel, a
rung-3 race in principle, and thumbv6m has no CAS to lock it with.
Resolution: the constant and constructor surface becomes
*exemplar-relative* — instance methods whose receiver supplies only
the precision context. The fixed rungs ignore the receiver and
constant-fold, so the refactor is provably behavior-neutral for
rungs 1 and 2 and lands first as its own M4-style byte-identity
commit; the Ziv driver then seeds each attempt with
`from_format_at(x, p)` and doubles `p` until the predicate clears.
`ExtNum::precision()` becomes `precision(&self)` in the same change
(the plan's original note, now load bearing). Series caps become
precision-derived on the dynamic rung.

Execution order for the M8b session: (1) exemplar seam, byte-identity
gate; (2) `DecBig` constant generators plus oracle pins; (3) the
dynamic working type and its ops; (4) the runtime reduction;
(5) feature plumbing, ladder wiring, `force_rung3` lane, budget(p);
(6) CHANGELOG and the tier-language touch-points M9 finalizes.

**Implementation resolutions (recorded 2026-08-02, steps 3 through 5;
the ADR M9 finalizes inherits these).**

- *The `Copy` constraint.* `ExtNum` requires `Copy` (kernel bodies
  consume a working value more than once) and `DecBig` is `Vec`
  backed. Resolution: the working value is a `Copy` handle into an
  arena the Ziv driver owns per attempt (`DynArena`, a
  `RefCell<Vec<DecBig>>` plus the precision; handles carry an index,
  an exponent, and a sign). The exemplar receiver carries the arena
  reference and the precision, which is exactly the runtime context
  the seam was built to thread. Values are immutable once interned;
  no unsafe anywhere. A scoped global was already rejected in the
  seam decision above; this is its arena-shaped completion.
- *Reciprocal constants by division, not Newton.* The sketch above
  says Newton for `1/ln 10` and `1/ln 2`; the landed generators
  divide into the computed originals instead (the `two_over_pi`
  pattern). Newton needs a seed, which is either a stored literal
  (against the runtime-computed decision) or an inner division
  anyway, and the division's error argument is three lines in the
  exact shape the module already carries.
- *Window depth.* The sketch's `q + p + ~45` is `q + p + 70` by the
  exact rung 2 mirror derivation (`FRAC = p + 33` for full survival
  under the worst 33-digit cancellation, plus the 33 + 4 coefficient
  overlap and carry-guard tail); the widths reproduce rung 2's
  constants exactly at `p = 110`.
- *Budgets as formulas.* `Budget` gained `dynamic: fn(u32) -> u128`,
  the fixed catalog's itemizations re-evaluated at `p` (series items
  scale with the precision-derived caps, constant items are
  precision-independent, Newton charges a flat 60, the runtime
  reduction contributes under 2 units at any `p`); every formula
  lands within a factor of five (observed ±30%) of its rung 2
  constant at `p = 110`, pinned by a unit test.
- *Ladder shape.* `Extended2::ESCALATES` is
  `cfg!(feature = "unbounded-ladder")`; the test-lane cfgs key on a
  new `ExtNum::RUNG` discriminant so `force_escalate` (rung 1 only)
  and the new `force_rung3` (rungs 1 and 2) keep their meanings in
  every build. `ladder_audit` is vacuous by construction under the
  feature (nothing delivers unconditionally) and keeps its meaning in
  default builds. The wrappers converted to one `ladder_run!` macro
  whose feature-off expansion is the pre-M8b two-closure call.
