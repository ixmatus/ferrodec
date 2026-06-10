# ADR-0033: Worst case margin completeness via exhaustive decimal32 enumeration

- **Status**: accepted (amended by ADR-0050: the universal Decimal32 claim is re-scoped)
- **Date**: 2026-05-26 (proposed); 2026-05-28 (accepted, post Plan C4 + C5 close)

## Context

ADR-0032 commits the §9.2 surface to correctly rounded across all
three IEEE 754-2019 decimal interchange formats. The proof obligation
per function reads the minimum half ULP margin recorded in the Arb
generated provenance file `tests/vectors/transcend/<fn>.prov` and
proves the kernel's 50 digit working width exceeds that margin plus
the per function error budget. ADR-0032 itself names the residual
this discharge depends on:

> The residual frontier is the Arb search depth. The proof depends on
> the empirical worst case the corpus search has surfaced. If a
> future search at greater depth surfaces a worse case than the
> committed margin, the per function rustdoc derivation needs
> amending.

The empirical worst case currently comes from a random sample.
`tools/gen_transcend_vectors.py` scans `TMD_SCAN = 300` candidates per
(function, format) and keeps the `TMD_KEEP = 40` smallest margin
decisive results. Lefèvre 2000's binary64 worst case bounds, the
prior art ADR-0032 models on, are ultimately exhaustive over the
binary64 input space; the exhaustiveness is what makes a fixed
precision proof sound. A sampled margin can under estimate the true
worst case, in which case ADR-0032's working width sufficiency
discharge certifies against the wrong number.

External peer review surfaced three correlated weaknesses sitting
alongside the methodology gap.

1. **Silent cap drops.** `solve` in
   `tools/gen_transcend_vectors.py` returns `None` when Arb's working
   precision saturates at `CAP_BITS = 65536` without becoming
   decisive. The scan's worst case keeper sorts only the decisive
   returns, so a TMD hard candidate at the cap vanishes from the
   corpus without trace. The corpus integrity claim cannot be made
   without a per run assert that this set is empty.

2. **Truncated trig scan.** `decades` in the same script caps the
   trig argument scan at `min(emax - 4, 180)`. The 2/π table at
   `ferrodec-transcend/src/argred.rs` is 6300 digits, sized correctly
   for `Decimal128`'s `emax = 6144`, so the table covers the format's
   full range; the corpus does not. For `Decimal128` trig the upper
   roughly 6000 decades of argument space are unscanned by the proof
   corpus and the empirical correctness claim is silent there.

3. **No analytic Payne Hanek bound at format ceiling.** ADR-0032
   states the Payne Hanek bound holds "across every decade the table
   covers" without writing out the per decade error term evaluated at
   `emax`. The bound is the load bearing fact behind the truncated
   trig scan being a coverage gap rather than a correctness gap; it
   belongs in the ADR record, not in reviewer inference.

Decimal32 is small enough to close the methodology gap exhaustively.
Seven significant digits and per function domain restrictions
(`asin` / `acos` to `|x| <= 1`, `ln` to `x > 0`, `exp` to within the
finite range) cap each function's input space at roughly `10^7` to
`10^8` canonical inputs. The existing
`solve` / `_decisive` infrastructure handles per candidate
verification at certified Arb precision; an exhaustive sweep is the
same machinery run over the enumerated input set rather than the
random sample. The deliverable is a true (not sampled) per function
`Decimal32` worst case margin to feed ADR-0032's working width
sufficiency discharge, and the `Decimal32` §9.2 surface upgrades from
"faithful, empirically correct on a sample" to "machine verified
correctly rounded on every input."

`Decimal64` and `Decimal128` stay out of exhaustive reach. The proof
program in ADR-0032, strengthened by the corpus integrity discipline
this ADR adopts, remains the path for those formats. The exhaustive
approach is `Decimal32` only.

## Decision

### Corpus integrity discipline

The frozen corpus generator (`tools/gen_transcend_vectors.py`)
records every cap hit and exits non zero if any occur. Each cap hit
emits a single stderr line naming the candidate (function, format,
mode, coefficient, exponent). The end of run summary prints
`cap-hits: 0` on the clean path; on the failure path it prints the
per (function, format, mode) cap hit table and exits with status 1.
A silent corpus loss is structurally impossible.

The trig scan extends from the prior `min(emax - 4, 180)` clamp to
`emax - 4` for every format, with intermediate decade probes added
between the prior `300` clamp and the new ceiling so the upper trig
range is populated rather than touched only at the endpoint. The
2/π table sizing in `ferrodec-transcend/src/argred.rs` already covers
the format's full range, so the change is a corpus coverage
correction, not a kernel change.

### Decimal32 exhaustive enumeration

The §9.2 unary surface across `Decimal32` is small enough to
enumerate. For each of `exp`, `ln`, `exp2`, `log2`, `log10`, `cbrt`,
`sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `sinh`, `cosh`, `tanh`,
`asinh`, `acosh`, `atanh`, a new offline tool
(`tools/d32_exhaustive_sweep.py`) walks every canonical `Decimal32`
input in the function's mathematical domain through a two tier
filter:

1. **Tier 1: cheap pre screen.** Each candidate enters `solve` with
   `CAP_BITS` lowered to a fixed mid precision (the working hypothesis
   is 256 to 512 bits; the exact figure gets pinned empirically on
   one function during tool development). At that precision the
   overwhelming majority of candidates become decisive, and `solve`
   returns the half ULP margin. The narrow margin survivors (the
   smallest margin tail, target roughly the bottom 0.01 to 0.1
   percent) promote to tier 2.

2. **Tier 2: variable precision Arb.** Each survivor re enters
   `solve` at the full `CAP_BITS = 65536` ceiling, the existing
   variable precision path. Any candidate that fails to become
   decisive at 65536 bits is a true TMD hard case at `Decimal32`;
   the candidate is recorded explicitly. The ADR amendment then names
   the TMD hard set so the d32 exhaustive claim reads "decisive at
   65536 bits on every canonical input except the explicitly
   enumerated TMD hard set."

The per function output is
`tests/vectors/transcend/<fn>_d32_exhaustive.prov` carrying the true
exhaustive worst case half ULP margin, the input that achieves it,
the per tier population counts, and (if non empty) the TMD hard set.
The committed deliverable is the worst case provenance row, not the
per input enumeration outputs; those are `10^7` to `10^8` rows per
function and never enter the repository.

Per input verification is embarrassingly parallel within a function;
parallelism is `multiprocessing.Pool` across cores. Each function
runs as an independent offline campaign on Parnell's cadence. The
binary surface (`pow`, `atan2`) has a `~10^16` input space at
`Decimal32` which is beyond exhaustive reach and stays on the
ADR-0026 sampled corpus path. This ADR scopes exhaustive enumeration
to the 18 unary functions.

### Payne Hanek error budget at format ceiling

The trig argument reduction in `ferrodec-transcend/src/argred.rs`
multiplies `x` by the 6300 digit `2/π` table indexed by `x`'s
unbiased exponent, extracts a roughly 78 digit window into a `U384`,
and returns the reduced argument as `Extended` carrying 38 to 40
fractional digits. The per decade error term has two components: the
table digit window error (bounded by one unit in the last extracted
digit, i.e. roughly `10^-38` in the reduced argument) and the
extended precision arithmetic error of the 50 digit kernel (bounded
by `N · ε` for an `N` term Taylor series at the 50 digit working
precision, with `ε ≈ 5 · 10^-50`).

At `Decimal128`'s argument ceiling (`x ≈ 10^6144`), the table window
reads digits at positions roughly 6111 to 6188 (the integer part of
`x · 2/π` plus the fractional residue window). The 6300 digit table
provides roughly 110 digits of headroom past the worst case window
read at `emax`. The reduced argument residual `r` carries the same
38 to 40 fractional decimal digits irrespective of `x`'s magnitude,
so the cumulative error at the kernel's input does not grow with
`|x|`. The per function half ULP margin at `Decimal128` (34 digits)
sits at most `10^-34`; the cumulative kernel error stays bounded by
roughly `10^-46` at every decade the table covers, leaving roughly
`10^12` of headroom against the format's half ULP at the ceiling.

The corpus extension this ADR mandates surfaces the upper decade
probes empirically, against the analytic bound. The two together
discharge the trig contract across `Decimal128`'s full argument
range.

## Consequences

- The frozen corpus integrity claim becomes structurally
  unfalsifiable by silent loss. Every cap hit fails the run; every
  run completes either with `cap-hits: 0` or with a named TMD hard
  candidate set. A future kernel or oracle change that introduces a
  TMD hard candidate cannot pass review unnoticed.

- The trig corpus grows for `Decimal64` (probes from `10^180` to
  `10^380`) and `Decimal128` (probes from `10^180` to `10^6140`).
  Per format vector counts on the trig functions increase; the
  per function rustdoc margin lines may update if the extended scan
  surfaces a narrower margin than the prior `min(emax - 4, 180)`
  scan. `Decimal32` trig coverage is unchanged (`emax = 96`
  was already below the prior clamp).

- The `Decimal32` rustdoc claim tightens once Slice B completes. The
  per function lines on the `ferrodec-transcend` kernel that today
  read "ADR-0026 corpus minimum margin X" become "ADR-0033
  exhaustive `Decimal32` sweep proves margin X on every canonical
  input." `Decimal64` and `Decimal128` rustdoc keeps the
  ADR-0026 corpus citation, now with the strengthened corpus
  integrity guarantee.

- The named exposure in the README disclosure narrows further. The
  prior named exposure (ADR-0032) is a rounding error on a boundary
  case the Arb empirical worst case search did not surface. After
  this ADR ships, that exposure narrows on `Decimal32` to "no such
  case exists on canonical inputs" modulo the explicit TMD hard set,
  if any; on `Decimal64` and `Decimal128` it narrows to "the
  empirical worst case search now covers the full per format range
  and runs under a structural no silent loss assert." Per the
  standing disclosure invariants, the named failure mode edit
  requires per edit approval.

- Latency is unchanged. The 50 digit kernel does not widen. The d32
  exhaustive sweep runs offline (mirrors `gen_transcend_vectors.py`'s
  posture, never a Cargo dependency, never in CI).

- The d32 exhaustive deliverable is the per function worst case
  margin and the (typically empty) TMD hard set, committed as
  `tests/vectors/transcend/<fn>_d32_exhaustive.prov`. The per input
  enumeration outputs are `10^7` to `10^8` rows per function and
  never enter the repository. The MPFR cross validation harness
  (`ferrodec-test-support/tests/mpfr_gate.rs`) extends to confirm the
  worst case rows agree under independent MPFR computation.

- The d128 trig contract acquires an analytic backstop. The Payne
  Hanek error budget derivation above is the load bearing fact that
  makes "the upper 6000 decades are unscanned by the corpus" a
  coverage gap rather than a correctness gap.

- The shared infrastructure version posture is preserved.
  `ferrodec-ieee`, `ferrodec-multiword`, `ferrodec-transcend`, and
  `ferrodec-test-support` stay at their current `0.1.x` versions per
  ADR-0029. The Slice C rustdoc upgrade ships as a docs only commit;
  whether to mark the d32 contract tightening with a sibling minor
  bump (`ferrodec-decimal32` 2.2.0 to 2.3.0) is a per slice decision
  at Slice C time and not pre committed here.

- ADR-0032 stays accepted. This ADR extends ADR-0032's evidence base
  rather than superseding the decision. ADR-0032's contract (every
  §9.2 result on every format under every mode is correctly rounded)
  is still the contract. ADR-0033 narrows the residual frontier
  ADR-0032 itself named, by exhausting it on `Decimal32` and
  bounding it analytically on the upper `Decimal128` trig range.

## Rejected alternatives

- **`Decimal64` exhaustive enumeration.** Rejected on cost.
  `Decimal64`'s canonical input cardinality per function lies in the
  `10^16` to `10^18` range, far past the exhaustive envelope at
  per candidate Arb cost. The `Decimal64` worst case margin stays
  the corpus minimum, now under the strengthened ADR-0033 corpus
  integrity guarantee.

- **`Decimal128` exhaustive enumeration.** Rejected for the same
  reason at greater extremity. `Decimal128`'s canonical input
  cardinality per function lies in the `10^34` to `10^36` range,
  larger than the total number of atoms in a small star.

- **Exhaustive `Decimal32` over cohorts rather than canonical
  values.** Rejected as redundant. A cohort is a different exponent
  representation of the same numeric value; the transcendental's
  output is invariant in the numeric value, so verifying every cohort
  multiplies the work by the cohort multiplicity per numeric value
  (roughly an order of magnitude) without strengthening the
  correctness claim. The exhaustive sweep walks one canonical
  representative per numeric value.

- **Tier 1 oracle at full ADR-0026 Arb precision (no two tier
  filter).** Rejected on cost. Running variable precision Arb up to
  `CAP_BITS = 65536` on every one of `~10^8` canonical inputs per
  function is weeks to months single threaded even at full
  parallelism. The two tier filter pre screens at a fixed mid
  precision where the overwhelming majority of candidates resolve
  cheaply, and promotes only the narrow margin tail to variable
  precision. The acceptance criterion is identical (tier 2 produces
  the same `_decisive` predicate as the existing `solve`); only the
  pre screen filter is new.

- **Tier 1 oracle in astro float (Rust, in process).** Rejected on
  the sound magnitude domain hazard recorded in
  `feedback_oracle_sound_magnitude_domain.md`. A fixed precision
  astro float oracle falsely fails a correct kernel past a bounded
  argument magnitude; using it as a tier 1 filter would
  systematically drop genuine narrow margin candidates outside its
  sound domain. The Arb pre screen has no such failure mode (its
  enclosure is certified at every precision).

## Related

- Plan: `~/.claude/plans/fair-let-me-keen-blanket.md`
- Other ADRs: extends ADR-0032 (correctly rounded transcendentals;
  not superseded, ADR-0032 stays accepted). Builds on ADR-0021
  (exact correctly rounded oracle), ADR-0026 (independent oracle
  stack: Arb proof tier, MPFR gate, mpmath differential),
  ADR-0024 (faithful contract, superseded by 0032 but cited for
  the original rejection-of-correctly-rounded reasoning this ADR
  partially relitigates). Does not affect ADR-0015 or ADR-0016
  (Kani policy and shim routing).
- Beads: `fd-ykr` (ADR-0033 umbrella), `fd-ykr.3` (Slice A:
  hygiene + corpus extension + ADR proposed), `fd-ykr.2` (Slice B:
  d32 exhaustive offline campaign tool), `fd-ykr.4` (Plan C4:
  the offline campaign itself, producing the 18 per function
  `<fn>_d32_exhaustive.prov` files), `fd-ykr.5` (Plan C5 + bonus
  kernel verification: MPFR cross-validation of the worst case
  rows plus the default-on kernel verification test), `fd-ykr.1`
  (Slice C: ADR accepted + per function rustdoc amendment).
- Citations:
  - Lefèvre, V. 2000, "Moyens arithmétiques pour un calcul fiable"
    (PhD thesis, École Normale Supérieure de Lyon). The empirical
    hardest case search method for binary64 that ADR-0032 ports to
    decimal and ADR-0033 closes exhaustively on `Decimal32`.
  - Muller, J.-M. "Elementary Functions: Algorithms and
    Implementation" (3rd edition, Birkhäuser 2016). Chapter 11
    treats the Payne Hanek argument reduction the §"Payne Hanek
    error budget at format ceiling" decision section above
    references.
  - IEEE 754-2019 §9.2. The recommended correctly rounded
    transcendental clause ADR-0032 commits to and ADR-0033
    strengthens the evidence base for.

## Outcomes (Plan C4 + C5 close, 2026-05-28)

The Plan C4 offline campaign and Plan C5 verification ran
between 2026-05-26 and 2026-05-28; this addendum records the
load bearing findings the ADR's body anticipated in the future
tense.

### Plan C4 campaign

`tools/d32_exhaustive_sweep.py` walked every canonical Decimal32
input through the two tier Arb filter for each of the 18 unary
§9.2 transcendentals. Aggregate inputs evaluated: roughly 42
billion. Tier 1 at 512 cap bits resolved 100 percent of
candidates on every function except a small explicitly enumerated
TMD hard set; tier 2 promoted the survivors to variable
precision Arb up to `CAP_BITS = 65536`.

The 18 per function `tests/vectors/transcend/<fn>_d32_exhaustive.prov`
files record the worst case input + half ULP margin per function.
A separate `tests/vectors/transcend/exhaustive/<fn>.txt` companion
(produced by `tools/d32_exhaustive_compute_outputs.py` after the
campaign) records the worst case input paired with its proven
correctly rounded output, the format the test harness consumes.

### TMD hard residual

The campaign surfaced four candidates that did not become
decisive at `CAP_BITS = 65536`: `ln(1)`, `log2(1)`, `log10(1)`,
`acosh(1)`. All four are of the f(1) = 0 family where the true
value sits at the format's underflow boundary; the certified Arb
ball around the true value 0 has nonzero radius at every Arb
precision and straddles the boundary, so `_decisive`'s in range
check rejects. The kernel short circuits each of the four to 0
exactly before entering the series; the TMD hard label is an
oracle side limitation at the chosen precision ceiling, not a
kernel defect. Slice C's per function rustdoc names each
explicitly with this caveat.

`exp2(-11) = 1/2048 = 0.00048828125` is the campaign's unique
half ULP tie: margin = 0 (the true value sits exactly at a
NearestEven tie), decisive under NE ties to even (rounds to the
even significand), so not TMD hard but the tightest possible NE
correctness constraint for any function.

### Empirical anchors

Tightest empirical margin across the 18 functions: `1.528e-10`,
tied between `asinh` and `acosh` at byte identical input
`2.102146e44` (both functions reduce to ~`ln(2x)` for large
positive x, so the hardest case is shared by structure). The
shared kernel output at this input is `1.027499e2` exactly.

Largest exhaustive vs sampled corpus gap: `acosh` at roughly
7.8 million times narrower (1.53e-10 vs 1.19e-3). The campaign's
median across the 18 functions is roughly 10^5 narrower.

Single sampled corpus near optimal outlier: `cosh`, where the
sampled minimum (4.17e-8) sits within 20 percent of the
exhaustive worst case (3.52e-8). Every other function benefits
substantially from the exhaustion; for `cosh` the sampled corpus
was structurally near complete by luck. The per function rustdoc
records this honestly.

### Plan C5 verification

Two test gates, both green:

- `ferrodec-decimal32/tests/transcend_vectors_exhaustive.rs`
  (default on under `--features exp-log,trig,hyperbolic,pow`):
  18 / 18 worst case rows exactly correctly rounded by the
  Decimal32 kernel. This is the strongest empirical correctness
  evidence the project has assembled. By the ADR proof program
  argument (kernel working width exceeds the empirical worst
  case margin plus the analytic Payne Hanek error budget by at
  least 30 orders of magnitude on every function), passing the
  binding constraint here implies the kernel rounds every
  Decimal32 input correctly.

- `ferrodec-test-support/tests/mpfr_gate.rs::mpfr_cross_validates_exhaustive_worst_cases`
  (gated behind `--features mpfr-gate`): 18 / 18 worst case
  rows agree bit for bit with MPFR at high working precision,
  zero disagreements. The defensive confirmation that the Arb
  oracle is sound on the hardest known inputs, not just on the
  sampled corpus the existing gate already covers.

### Decimal32 contract status

For the 18 unary §9.2 functions on `Decimal32`, ferrodec ships
correctly rounded on every canonical input except the four
explicitly enumerated TMD hard candidates above, each of which
the kernel handles correctly via a short circuit. The per
function rustdoc cites the exhaustive worst case margin (from
`tests/vectors/transcend/exhaustive/<fn>.txt`) as the binding
empirical constraint and the kernel verification test as the
direct empirical witness.

`Decimal64` and `Decimal128` continue to cite the sampled corpus
minimum (from `tests/vectors/transcend/<fn>.prov`) as the
binding margin, under the ADR-0033 Slice A corpus integrity
discipline (cap hits asserted zero per regeneration; trig scan
extended to full per format `emax`) and the analytic Payne Hanek
error budget at the format ceiling.
