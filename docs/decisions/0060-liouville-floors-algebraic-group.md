# ADR-0060: Liouville floors for the algebraic §9.2 group: unconditional two rung claims, the exact integer adjudicator, and the powr negative result

- **Status**: accepted (Track D D3 phase gate, fd-4zo.25; the banner
  below governs the proof posture)
- **Date**: 2026-08-05

> **Not specialist verified.** The number theory in this ADR was
> derived by the supervising model and has not been reviewed by a
> professional number theorist. Its three legs of evidence are the
> derivations themselves, an exhaustive small format falsification
> probe (`tools/liouville_probe.py`), and agreement with the published
> binary format analogs (lang-muller-2001, iordache-matula-1999,
> brisebarre-muller-2007), which derive the same scaling laws by an
> independent method. External verification can be priced at any time;
> until then, every claim below carries this banner.

## Context

ADR-0059's Track D charter deferred the algebraic §9.2 group (`pown`,
`powr`, `rootn`, `compound`, `rSqrt`, `hypot`, group D3, fd-4zo.25)
behind a phase gate: derive, before any implementation, whether
Liouville type bounds from minimal polynomials give provable finite
ladder caps, making the correctly rounded claim for the group
*unconditional* in the default two rung build and giving the dynamic
rung a proven termination width. The charter's one line version
("the algebraic operations need no spike: Liouville type bounds from
minimal polynomials give unconditional caps") turns out to be true
only when made precise, and false for one operation; this ADR is the
precise version.

The operational meaning of a cap is fixed by the ladder mechanism
(ADR-0059). Rung 2 (110 digit `Extended2`) delivers unconditionally
in the default build; its delivery is correct for an input iff the
true value and the working value sit on the same side of every
rounding boundary, which holds whenever the true value's relative
distance to every boundary exceeds `B₂ · 10^−109`, where `B₂` is the
operation's audited rung 2 budget and `10^−109` bounds one working
ulp relative to the value. So the target statement per operation is a
proven floor:

> **(⋆)** every input not disposed of input side (exact and tie
> classification, anchor residual channel) has a true value whose
> relative distance to every format grid point and nearest mode
> midpoint exceeds a stated `10^−D`, with `D` explicit.

`D < 109 − log₁₀ B₂` makes the two rung claim unconditional; the
dynamic rung provably terminates at working precision
`p ≈ D + log₁₀ B + 2`.

A rounding boundary near a normal value is `M · 10^F` with `M` an
integer below `10^P` (grid) or below `10^(P+1)` and ending in 5
(midpoint), `P` the format precision. Everything below is about how
close an algebraic number can sit to such a rational. (Subnormal
region boundaries pin the quantum at `10^(etiny−1)` instead; each
operation's result range is checked against that region at its spec.)

## Decision

### The floor lemma, two engines

**Engine A (rational true values: `pown`, `compound`).** If
`y = N/10^k` is an explicit rational and `b = M·10^F` a boundary,
`y − b` is a rational whose numerator is a nonzero integer whenever
`y ≠ b` (`y = b` is the classified exact or tie case), so
`|y − b| ≥ 10^min(F,0) · 10^−max(k,0)`-scaled by the alignment; the
work per operation is bookkeeping the denominator.

**Engine B (algebraic degree `d ≥ 2`: `rSqrt`, `rootn`, `hypot`,
`powr`).** The conjugate identity replaces the mean value theorem so
every constant is explicit: for `y > 0` with `y^d` rational and any
rational `b` within a percent of `y`,

> `|y − b| = |y^d − b^d| / (y^(d−1) + y^(d−2)·b + … + b^(d−1))
>          ≥ |y^d − b^d| / (d · (1.01·y)^(d−1))`,

and Engine A applied to `y^d − b^d` supplies an integer numerator
`≥ 1` over an explicit denominator. A zero numerator means `y`
rational, which is the classified set: **completeness of the input
side exact and tie classification is a stated premise of every
floor**, exactly tripod leg 1 in its existing role. This is
Liouville's 1844 argument specialized to power of ten denominators;
lang-muller-2001 and brisebarre-muller-2007 prove the binary analogs
by an independent digit recurrence method, and every scaling law
below matches theirs (rSqrt `~3n`, q-th root `~qn`, sqrt and norm
`~2n`), which is the strongest external check available short of
specialist review.

### rSqrt, worked (the template the group follows)

Input `x = a·10^u > 0`, `a < 10^34`. Write `u = 2v + r`, `r ∈ {0,1}`:
`y = 10^−v · y₀` with `y₀ = m^−1/2`, `m = a·10^r < 10^35`, so
`y₀ ∈ (10^−17.5, 1]`. Scaling by `10^v` maps every boundary near `y`
to `M·10^G` near `y₀` (the boundary form is preserved; the input
exponent drops out — powers of ten couple multiplicatively, which is
what makes this cap uniform over the whole exponent range).

Bounding `G`: a boundary within a percent of `y₀ > 10^−17.5` exceeds
`10^−18`, and `M < 10^35` forces `G ≥ −18 − 34 = −52`; also `G ≤ −33`
so `10^−2G` is an integer. Then, using `m · y₀² = 1`:

> `|y₀² − b²| = |10^−2G − m·M²| · 10^2G / m ≥ 10^2G / m`
> `rel dist = |y₀ − b| / y₀ ≥ 10^2G / (m · y₀² · 2.01) = 10^2G / 2.01`
> `≥ 10^−104 / 2.01 ≈ 4.9·10^−105`.

**Verdict**: with a rung 2 budget `B₂ ≤ 10³` the two rung build is
unconditional with a margin above ×49; the dynamic rung terminates at
its first attempt (220 digits). The floor's near attainability is a
Pell shaped question (`m·M² = 10^104 ± 1` needs a 34 digit square
divisor of `10^104 ± 1`; open at that size, never observed, matching
lang-muller-2001's report that the binary rsqrt bound is unattained
through double precision), so the floor, not the heuristic worst case
near `10^−69`, is the binding contract.

The load bearing constant is `G ≥ −52`; an off by one flips the
verdict. It gets a boxed derivation here, the probe's small format
enumeration, and a dedicated unit test in the mechanism slice.

### Per operation floors (Decimal128; smaller formats follow the same formulas and are strictly easier)

| op | floor (relative), non classified inputs | bare two rung verdict | with adjudicator |
|---|---|---|---|
| `rSqrt` | `4.9·10^−105` uniform | unconditional (Newton kernel, `B₂ ≤ 10³`) | unconditional, margin question gone |
| `rootn`, `2 ≤ |n| = q ≤ 6` | `≥ 10^−D`, `D(q) ≤ (q+1)·34 + q + 8` (table in spec: 105 / 142 / 177 / 211 / 247) | `q = 2` only | `q ≤ 5` unconditional (`q = 6` needs `U1024`, priced at spec) |
| `pown`, `2 ≤ |n| = q` | positive `n`: `D ≤ 34q + 2`; negative: `D ≤ 34q + 36` | `n ∈ {−2, 2, 3}` (powering arm) | `n` positive `≤ 6`, negative `≥ −5` |
| `compound` | `D ≤ n·w + 36`, `w` = digit width of exact `1 + x` | marginal corner only | `n·w ≤ 196` unconditional (covers every realistic call) |
| `hypot` | anchor band `ρ = min/max ≤ 10^−18`: residual channel; kernel band: `≥ 1/(8.1·S)`, `S` the scaled integer `a₁²·10^2Δ + a₂²  < 10^174` | conditional for `S > 10^105` | unconditional across the band |
| `powr` | **no useful uniform floor** (below) | — | — |

Notes anchored to the table:

- **hypot's split.** (Implemented as a strict adjusted exponent gap
  `adj(w) − adj(z) > ⌈(P+2)/2⌉`, which PROVES `ρ < 10^−δ₀`; the gate
  is one sided conservative, and the seam is pinned by a
  both-bands-agree test.) For magnitude ratio `ρ ≤ 10^−18` the true value
  hugs `|x₁|` from above at `≤ ρ²/2 ≤ 5·10^−37` relative, below every
  boundary above `|x₁|` (`≥ 5·10^−35`) with a ×10 margin: the
  ADR-0051 residual channel decides every mode input side (side
  theorem: `hypot(x₁,x₂) > |x₁|` strictly for `x₂ ≠ 0`). The kernel
  band then has a bounded exact integer `S` and Engine B applies. The
  band edge constant `10^−18` and the `Δ ≤ 52` width bound get their
  own tests. This is the D1/D2 integer anchor lesson recurring
  exactly where the kickoff predicted (`hypot(x, tiny)` hugging
  `|x|`).
- **compound reduces to pown on the exact rational `1 + x`** (the D1
  logp1 exact sum analysis), and inherits two whole range families
  the D2 lesson demands classified input side: `1 + x` a power of ten
  (values `10^kn` across and beyond the exponent range, the
  `exp10_integer` twin, §7.4 dispositions included) and huge `x`,
  where `compound` hugs the classified `pown` value. Both are spec
  obligations recorded here.
- **Dynamic rung termination, now with widths.** Every operation
  above terminates at a proven width `p ≈ D + log₁₀ B + 2`: first
  attempt (220) for `rSqrt`, `hypot`, `rootn q ≤ 5`, and `pown` over
  `−5 ≤ n ≤ 6` (`n = −6` has `D ≤ 240`, so its proven width sits at
  the first doubling, 440 — one rung out, recorded rather than
  rounded off; the powi lane's review caught the original sentence
  claiming it at 220).
  For large operands (`rootn q ≳ 2800`, `pown`/`compound` with
  `n·w ≳ 10^5`) the proven width exceeds the rung 3 constant
  generators' 100,000 digit depth cap: a genuinely pathological input
  would panic loudly there rather than deliver a wrong rounding,
  the M8b posture, now with the parameter range stated instead of
  implied.

### The powr negative result

`powr(x, y) = e^(y·ln x)` with both operands format values: `y` in
lowest terms is `a/b` with `b` up to `~10^33`, and the algebraic
degree of `x^(a/b)` is `b` (up to the perfect power structure of
`x`). Engine B's exponent scales linearly in the degree, so there is
no useful uniform floor: the guarantee parameter is the second
operand's denominator, and it is unbounded. This is not an
implementation gap; it is the same wall that makes the
transcendentals hard, and Matveev style linear forms in logarithms
(the S5 spike, matveev-2000) are the only literature route that could
improve it. **Decision (Parnell, 2026-08-05)**: powr ships in D3
anyway, at pow's existing documented tier (Tier 1 by construction
plus Tier 2 model), on identical machinery; the claim ladder is per
operation, so the weaker tier sits honestly beside the others. The
kickoff's stop rule stops the claim upgrade, not the operation.

### The exact integer adjudicator (tripod leg 1 completed)

For every operation above except powr, "which side of the one
candidate boundary" is decidable exactly in bounded integer
arithmetic, because the true value satisfies a known integer relation
of bounded size: sign(`y − b`) = sign(`10^−2G − m·M²`) for rSqrt
(one `U384` compare), sign(`4S·10^· − (2M+1)²·10^·`) for hypot
(`U768`), the `q`-th power comparisons for `rootn`/`pown`/`compound`
within the width table above. **Decision (Parnell's principle call,
2026-08-05: "the principled, sophisticated, and correct thing")**:
the rung 2 delivery for the D3 operations runs the escalation
predicate, and on an ambiguous verdict adjudicates exactly instead of
delivering blind or panicking. The engineering shape: replacing "the
hash almost surely has no collision, with a written probability" by a
content compare on the (essentially never taken) collision path.

Semantics: rung 2's working error (`≤ B₂` units `≪` half a quantum)
locates the single candidate boundary; the adjudicator computes the
exact side; delivery follows that side with `INEXACT` (the true value
is off boundary by classification completeness). Escalation stays a
deterministic function of the input; `ladder_audit` for these
operations becomes "the adjudicator ran and decided", vacuous panics
removed by construction. In `unbounded-ladder` builds the adjudicator
makes the rung 3 entry unnecessary for these operations (the fixed
rungs plus the adjudicator already decide everything), but the Ziv
path stays wired for uniformity and its proven 220 digit first
attempt termination is pinned by test. Adjudication runs only behind
a rung 2 ambiguity (`~10^−66` of calls at the widest budget), so its
cost is irrelevant; its correctness tests are not, and the
`S = k² + k` and `k² + 1` constructions (near attaining families,
confirmed by probe) supply real planted inputs for them. Naming of
the module and functions is settled at the D3 naming checkpoint;
names are identity bearing.

### Kernel architecture constraints (mandated by the floors)

- `rSqrt` must be a direct Newton kernel (seeded reciprocal square
  root, or `recip` ∘ `sqrt` composition on `ExtNum`), budget target
  `≤ 500`. The `exp(−½·ln x)` route carries the `|ln x| ≤ 14151`
  amplification (`B₂ ~ 10^8`, the CBRT/POW scale) and its threshold
  `~10^−101` cannot clear the `4.9·10^−105` floor: architecture is
  forced, not preferred.
- `pown` needs a binary powering arm at working precision for small
  `|n|` (the bare unconditional range and the common case; also
  faster), with the `exp(n·ln x)` route for large `|n|`.
- `rootn` and `compound` may use the `exp/ln` route (their
  unconditional range comes from the adjudicator, not from margins),
  with the powering arm an optimization question, not a correctness
  one.

### Tier language (the payoff, stated)

For the README and rustdoc, joining the ADR-0059 ladder:

> For `pown` (`−5 ≤ n ≤ 6`), `rootn` (`2 ≤ |n| ≤ 5`), `compound`
> (`n · width(1+x) ≤ 196`), `rSqrt`, and `hypot`, correct rounding in
> the default build is **unconditional**: input side classification,
> the anchor side theorems, the Liouville floors of ADR-0060 against
> the audited budgets, and the exact integer adjudicator on the
> residual path together leave no exception set, in any build.
> Outside those operand ranges the operations carry the Tier 1 by
> construction and Tier 2 model claims, now with a proven dynamic
> rung termination width in `unbounded-ladder` builds (loud panic
> honesty past the generator depth cap). `powr` carries `pow`'s tier
> statement verbatim; its claim cannot be upgraded by minimal
> polynomial bounds at all (this ADR's negative result).

The exact operand constants above are the spec time numbers to
verify; they move only downward (conservative) if the itemized
budgets land wider than targeted.

## Consequences

- The three most plausible ways this is wrong, inverted up front.
  First, a constant bookkeeping error (a `G` off by one flips a
  verdict): countered by the exhaustive small format probe promoted
  to `tools/liouville_probe.py`, boxed derivations for the load
  bearing constants, per format enumeration unit tests in the
  mechanism slice, and the banner's standing offer of external
  verification. Second, the adjudicator rots unexercised (the
  sinh/cosh saturation lesson's shape): countered by planted near
  boundary corpora from the near attaining families and a test lane
  that routes deliveries through it unconditionally. Third, a future
  budget tightening treats the observed minima (`~10^−69`) as the
  binding quantity instead of the floors: this ADR states the floors
  are the contract; the observed minima are diagnostics.
- ADR-0059's charter sentence "the algebraic operations need no
  spike: Liouville type bounds from minimal polynomials give
  unconditional caps" is amended by this ADR: true for `rSqrt` and
  bounded integral operands, parameterized for `rootn`/`pown`/
  `compound`, false for `powr`. The S5 spike's scope explicitly
  gains powr as its only possible upgrade path.
- The adjudicator extends tripod leg 1 (input side decidability) from
  "exact and tie values" to "boundary side of near boundary values"
  for the algebraic group. It adds a pure integer module on a cold
  path; `no_std`, fixed width, no factoring, same as the existing
  classifier machinery.
- The phase gate outcome feeds the D3 specs directly: kernel
  architecture constraints, per operation operand ranges for the
  unconditional claim, the two hypot band constants, the compound
  whole range families, and the budget targets (`rSqrt ≤ 500`,
  powering arms `≤ 10³`) are binding on the implementation
  delegation.
- Verification posture is explicit: derivations (this ADR), probe
  (exhaustive at P=4 for all operations, P=5 for rSqrt at 10^6
  inputs, floors held everywhere, exactness criteria cross checked on
  every hit), literature (scaling laws match the binary analogs
  derived independently). What none of the three legs can see: a
  derivation error whose small format analog is also wrong in the
  same way and whose binary analog differs structurally; that is the
  residual risk the banner names.

## Related

- ADR-0059 (the lane charter this gate belongs to; its Track D
  section and claim ladder), ADR-0051 (anchor residual channel, the
  hypot far band and compound anchor deliveries), ADR-0047/M7
  (exactness machinery the classifiers extend).
- Plan: `plans/2026-07-25-correctly-rounded-d128-transcendentals-lane.md`
  ("Track D status and the D3/D4 kickoff terms").
- Beads: fd-4zo.25 (D3); the S5 spike bead gains the powr pointer.
- References (registry slugs): lang-muller-2001,
  iordache-matula-1999, brisebarre-muller-2007, niven-irrational-numbers,
  lauter-lefevre-pow-boundary, matveev-2000, ieee-754-2019.
- Instrument: `tools/liouville_probe.py` (the falsification probe;
  rerun instructions in its header).

## Landing note (fd-jxk, 2026-08-09)

The exact integer adjudicator specified above is implemented, wired,
and exercised; this note records where each piece landed and the two
deltas against the text above.

- **Mechanism.** The rung 2 escalation predicate now returns the
  candidate boundary's identity (`candidate_boundary` on each rung,
  the bool predicate its adapter, so the two cannot drift;
  `ladder::Boundary` carries `(coef, exp, Grid | Midpoint)`). The five
  kernels deliver through `ladder::round_adjudicated`: a rung 2
  ambiguity inside the adjudicable range is decided by the per
  operation `adjudicate::<op>_side` functions and delivered through
  the ADR-0051 residual channel anchored at the boundary; outside the
  range each build keeps its pre adjudicator behavior. Every decision
  path carries its completeness proof at the site, and an `Equal`
  comparison (the true value ON a boundary, which classification
  completeness excludes) panics loudly.
- **Semantics, as decided at the fd-jxk design checkpoint.**
  Adjudication is rung 2 semantics in every build: in
  `unbounded-ladder` builds an in-range ambiguity adjudicates instead
  of entering Ziv, so delivered bytes are build invariant on that path
  by construction; the Ziv rung stays wired for uniformity and the out
  of range remainder, and the `force_rung3` lane still exercises the
  220 digit first attempt termination. `ladder_audit` for the five
  operations panics only when the adjudicator *declined* — "the
  adjudicator ran and decided" replaces "no ambiguity observed", the
  vacuous panics removed by construction.
- **Delta 1: `U1024` (design checkpoint fork 3).** The width table's
  `q = 6` rows landed rather than being held back: `ferrodec-multiword`
  gained `U1024` (308 digits, compare oriented surface), so the
  unconditional operand ranges are `pown −6 ≤ n ≤ 6` and `rootn
  2 ≤ |n| ≤ 6` — one wider on the negative `pown` side and at
  `|n| = 6` than the tier language block above, which was written
  against the `U768`-only widths. The widest landed comparands:
  `rootn n = −6` aligns `a·C⁶` at ~244 digits, `pown n = −6` aligns
  `C·a⁶` at ~239.
- **Delta 2: the `D(q)` closed form vs the table.** The rootn row's
  formula `D(q) ≤ (q+1)·34 + q + 8` gives 112/147/182/217/252 while
  the tabulated spec numbers read 105/142/177/211/247: the closed
  form is uniformly loose by 5 to 7. The table binds; both agree on
  every width verdict (`U768` through `q = 5`, `U1024` at `q = 6`),
  so no decision moves.
- **Anti rot (Consequences inversion #2).** The `force_adjudicate`
  battery lane, run together with `force_escalate` in a default
  build, replaces rung 2's budgeted verdict with an unbudgeted
  nearest boundary locate: every corpus row of the five operations
  then delivers *through* the adjudicator wherever its range gates
  accept, and the full pinned corpus is the byte identity reference.
  The planted near attaining families (`S = k² + 1`, `S = k² + k`)
  are exercised both as kernel inputs (the hypot suites replay under
  the lane) and directly against the deciders at their sharpest
  (distinguishing `−0.2`, `+25·10^−34`, and `−1` in `y²` at
  `k = 10^16`).
- **Verification at landing.** Corpus replay byte identical on all
  three formats with the adjudicator wired (it decided nothing the
  normal path delivered); the force_adjudicate lane green on all
  three formats; the transcend suite green in both feature
  configurations; the per rung boundary identity pins and the
  decider unit tests new.
