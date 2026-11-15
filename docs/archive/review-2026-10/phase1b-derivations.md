# Phase 1b: the coordinator's derivations

Written 2026-10-08 at the Phase 1 tip `864e561`, before any Wave 2
reader is launched, so that the kernel readers' versions of these
arguments can be compared against an independent one. Reproducers for
anything numeric live under `reproducers/`.

**Reading order caveat, recorded for honesty.** The plan asked for the
four budgets to be derived blind. The reduction's error contract lives
in `argred.rs` beside the catalog it feeds, and I read the four catalog
entries in the same pass as that contract. What follows is therefore a
line by line verification of each itemization against the code, plus
an independent attack on each premise the itemization rests on, not a
derivation that never saw the catalog's numbers. The attack is where
the value turned out to be.

## 1. Constants

The twenty hand curated literals in `ferrodec-transcend/src/consts.rs`
(ten at 55 digits for rung 1, ten at 115 for rung 2) were compared
against mpmath at 140 digits. Every one lies within 0.5 units of its
last digit (eleven are truncations, nine are roundings; the largest
deviation is `INV_PI_EXT_STR` at 0.95 units below the true value,
still a faithful last digit). The crate's own test re-derives the
rung 1 set through astro-float at 220 bits and asserts agreement to
`10^-50`; this is a second, independent oracle for the same claim.

The nine `ferrodec-multiword::bigconst` generators (`pi`, `2/π`,
`ln 2`, `ln 10`, `e`, `tan(π/8)`, `1/ln 2`, `1/π`, `1/ln 10`) were
evaluated at 50, 100, 333, 1,000, and 2,000 digits and compared with
`floor(c·10^(n−1−E))` from mpmath at 2,100 digits: every output is
exact (the documented bound is one unit of the last digit; the
measured deviation is zero at all 45 points). Clean.

## 2. The Newton step counts (fd-k9t), analytically

Code read: `Extended::recip` seeds from the format's own reciprocal
(`to_format` then `recip_seed`, so the seed carries exactly
`F::PRECISION` digits) and runs two steps of `x ← x(2 − bx)`;
`Extended::sqrt` seeds from the format's square root and runs two
steps of `x ← (x + b/x)/2`, each step calling `div`, which is a
fresh two step `recip`. `Extended2` uses three steps of each. The
rustdoc on both says "≥ 33 digits of initial precision", true only for
Decimal128.

For the reciprocal the relative error recurrence is exact:
`e_{k+1} = e_k²`, with `e_0 ≤ 5·10^-P` (half an ULP of the seed).

| seed P | e_0 | after 2 steps | after 3 steps | after 4 | after 5 |
|---|---|---|---|---|---|
| 34 | 5e-34 | 6e-134 | beyond width | | |
| 16 | 5e-16 | 6e-62 | 4e-123 | | |
| 7 | 5e-7 | 6e-26 | 4e-51 | 2e-101 | 2e-202 |

Rung 1 needs about `10^-50`, rung 2 about `10^-110`. Decimal128 and
Decimal64 are inside the model at both rungs (Decimal64 rung 2 by 13
orders, not the 18 a doubling heuristic suggests). Decimal32 is not:
two steps reach 25 digits at rung 1 and three reach 50 at rung 2, so
every kernel that divides, takes a reciprocal, or takes a square root
carries a `10^-25` relative intermediate at rung 1, 25 orders outside
the budget model. The square root is worse than its own recurrence
suggests because its inner `div` is itself limited to 25 digits.

Which claims this touches: rung 1 decides the whole Decimal32 input
space (the escalation threshold sits below the coefficient lattice, a
derivation the telemetry gate pins). For the 18 unary functions and
`sqrt`, the ADR-0033 exhaustive tables put every input at least about
`10^-8` ULP (about `10^-15` relative) from a boundary, ten orders
above the `10^-25` intermediate error, so those shipped claims close a
posteriori. The D1 to D4 groups and the pi family at Decimal32 have no
such table, and there the "correctly rounded by construction" premise
is false in the strict sense. Which kernels use Newton ops: `atan`
(stage 1 reciprocal, stage 2 division), `asin` and `acos` (square
root and division), `tan` (division), `tanh` and the inverse
hyperbolics (division, square root), `rootn`, `rsqrt`, `hypot`
(square root), `pow`'s final inversion on one path. `compound`,
`exp`, `ln` and the `logp1` family use none (series with `div_u32`
only), so they are untouched. The expected number of misrounds this
could produce over all Decimal32 inputs of the affected functions is
about `10^-8` (an input must sit within `10^-18` ULP of a boundary),
so the gap is a soundness defect in the argument, not a witnessed
wrong value.

Closure, in the record only: derive the step count from the seed and
the target, as `extended_dyn::newton_steps` already does (it targets
twice the working precision and gives 2 and 3 steps at P = 34, 3 and 4
at P = 16, 4 and 5 at P = 7 for rungs 1 and 2); the fixed loops in
`extended.rs` and `extended2.rs` take the derived count. The sqrt's
inner division then widens automatically. One pin: a Decimal32
reciprocal of a near boundary case at rung 1 agreeing with mpmath to
50 digits. The derivation discharges the analysis the bead asks for;
the AWS replay is not needed for the fix, only for a claim that the
pre fix shipped values were all correct, which nothing needs once the
corpora replay green after the change. Severity for the remediation
epic: P1.

## 3. The adjudicator, read independently

`adjudicate.rs` in full, with a defect brief, before reader A2 exists.

- `cmp_aligned`: `hi_i = e_i + digits(m_i)` puts `v_i` in
  `[10^(hi_i−1), 10^hi_i)`, so separated decades decide without
  arithmetic; same decade shifts the larger exponent side by
  `|e1 − e2| = |digits(m2) − digits(m1)|`, landing exactly on the
  other's width. Negative exponents and i64 products are handled
  uniformly. The `as u32` cast on the shift is bounded by the width
  argument (≤ 244). Sound.
- Widths against `U1024` (308 digits), with `C < 10^35` (a midpoint)
  and stripped coefficients `< 10^34`: `rsqrt` `c·C² < 10^104`;
  `hypot` `C² ≤ 10^70` against `S < 2·10^170`; `powi` at `n = −6`
  `C·a^6 < 10^239`; `rootn` at `n = −6` `a·C^6 < 10^244`; `compound`
  `C·N'^q < 10^231` by the `|n|·w ≤ 196` gate. The table reproduces.
- Each relation: `rsqrt` `sign(1 − c·C²·10^(u+2E))`, `hypot`
  `sign(S·10^2q − C²·10^2E)`, `powi` both signs of `n`, `rootn` both
  signs, `compound` with `1 + x = N'·10^s` built from the stripped
  operand on `U768`. Each is the monotone square or power of the
  defining identity, multiplied through by a positive quantity, and
  the exponent bookkeeping in the calls matches the formulas. Sound.
- `compound_side`'s gates: `d > 198` and `digits(c) + e > 198` return
  `None`, which the doc proves loses no in range case (`w ≥ 197`
  already exceeds the `|n|·w ≤ 196` range for every `n ≠ 0`); the
  negative side guard `digits(c) > d` keeps `x ≤ −1` out and the
  subtraction from wrapping. Sound.
- `Equal` panics. Reachable from the decider level (the tests prove
  it); reachable from the public API only through a classifier gap.
  Design note for the report, not prejudged: on `Equal` the true value
  IS the boundary, so the correct delivery is `b` under the tie rule
  (a grid boundary is exact and not INEXACT; a midpoint rounds by the
  mode and is INEXACT), which the guarded path cannot express because
  it ORs INEXACT unconditionally. On an embedded target a panic is a
  reset, a wrong flag is silent. The sounder posture is probably the
  panic under `debug_assertions` and `ladder_audit`, and in release a
  documented degraded delivery of `b` with the right flag. Parnell's
  call; the report will carry both sides.
- Two items for reader A2, not verifiable from this file alone: that
  `exact::hypot_aligned_sum` returns `(S, q)` with `y² = S·10^(2q)`
  exactly (the decider's exponent arithmetic assumes it), and that the
  kernels map `Side::Above` to the correct residual direction for a
  negative result (odd `n` in `powi` and `rootn`), since the deciders
  compare magnitudes.

No defect found in the module.

## 4. The four budgets, verified line by line

Unit: the catalog charges one unit per working rounding (each binary
operation rounds half even at 50 digits, so at most `5·10^-50`
relative) and counts an error of `E` relative as `E·10^50` units,
which is the conservative direction because a unit in the 50th digit
is at least `10^-50` relative. Verified against `Extended::add` and
`mul` (exact `U384` product or aligned sum, then one half even
rounding) and against the predicate's widening to exactly 50 digits.

### sin (rung 1 budget 1.5e14, rung 2 10,000)

Code path: `reduce_trig` (window of 115 digits of `2/π`, 76 fractional
digits of `y` kept, residual `r = y · π/2_38`), then Taylor at
`|r| ≤ π/4`, then the octant select.

- `π/2` truncation: `PI_OVER_TWO_COEF_38` is below `π/2` by
  `6.28·10^-38` relative (computed from the literal). `r` inherits it
  exactly. The map into `sin` is `|r·cot r| ≤ 1` on the reduced range,
  into `cos` `|r·tan r| ≤ 0.79`, into `tan` `|2r/sin 2r| ≤ 1.57`. So
  `≤ 6.3e12` units for `sin`; the catalog's "≤ 10^13" holds.
- Window truncation: the catalog says 43 surviving digits "at the
  deepest cancellation (76 − 33 leading zeros worst case)". **This
  premise is false.** A continued fraction search over every
  Decimal128 exponent (`reproducers/P1B-1/cancellation_depth.py`,
  six seconds) gives the true minimum of `||x·2/π||` over the input
  set as `1.317·10^-38`, 37.88 digits of cancellation, at
  `x = 9308532438209917461067659354862169E4639`; every one of the
  6,146 exponents has a worst input deeper than 33 digits (5,415 at
  34, 670 at 35, 58 at 36, 3 at 37). The Diophantine expectation for
  about `10^38` inputs is exactly this. Decimal64's worst is 19.4
  digits, Decimal32's 9.9. So the guaranteed surviving width is 38
  digits at rung 1 (`10^-38` relative, `10^12` units, not `10^7`) and
  105 at rung 2 (`10^-105` relative, about `7.7·10^4` rung 2 units,
  not "about 1").
- Taylor at cap 120 (360 units) and recomposition (10): overcounts,
  fine. One unit for the residual's own rounding into 50 digits is
  unlisted and absorbed.
- Rung 1 total with the corrected window term: about `1.1·10^13`;
  the budget `1.5·10^14` still carries a 13× pad. Rung 2 total:
  about `7.7·10^4`; the stated budget of `10^4` is BELOW the unpadded
  sum for the deepest inputs. Inputs affected: those within about
  `1.1·10^-37` of a multiple of `π/2`, expected count about fourteen
  across all of Decimal128. For them, in default builds the top rung
  delivers unconditionally with a true error up to eight times the
  budget `ladder_audit` checks, and in `unbounded-ladder` builds a
  boundary between `10^4` and `7.7·10^4` units away would not
  escalate. The probability any of them misrounds is about `10^-72`
  per input, so no wrong value is expected and none is witnessed: the
  delivered `sin` and `cos` on the worst input are correctly rounded
  in all five modes (scratch crate output in the reproducer; the
  measured rung 1 residual error is `5.7·10^12` units, inside the
  model). `cos` and `tan` share the window term and the same
  correction.
- Finding P1B-1, proposed P1: the stated premise is unsound and the
  rung 2 trig budgets are mis-sized by about an order for a handful
  of inputs. Closure: `argred.rs` (doc premise and, if the rung 2
  window should keep its one unit term, `FRAC2_DIGITS` from 143 to at
  least 150), `ladder.rs` `SIN`/`COS`/`TAN` rung 2 constants restated
  from the derived depth, the probe committed as the derivation, and
  the worst input planted as a pinned row in all five modes. The
  consequence for the Tier 2 statement is a wording one: the model's
  rate is quoted per budget, and the budget must be the true bound.

### atan (rung 1 10,000, rung 2 20,000)

Code path verified: stage 1 reciprocal for `|x| > 1` (`recip`, 15
units in the catalog's model, exact Newton at P ≥ 16), stage 2 shift
`(t − 1)/(t + 1)` (the subtraction is exact for a 50 digit `t` near 1,
the division about 17 units relative, mapped through `atan` at a
small argument as relative, then added to `π/4` whose own literal is
good to `10^-55`), Taylor whose per term error grows linearly while
terms shrink as `0.17^k`, and the `π/2 − result` closing subtraction
against a result at least `π/4`. Sum about 650 units; the 10,000
budget carries a 15× pad. The `sticks_to` anchor runs before the
guard. At Decimal32 the stage 1 reciprocal is the fd-k9t 25 digit
intermediate; the exhaustive table closes `atan` there. Verified.
Campaign coverage: none (S2 did not sweep `atan`); the budget rests
on this derivation plus six planted rows. Reader A5 should probe the
stage 2 threshold band and the `|x|` just above 1 band against Arb.

### compound (rung 1 1.5e8, rung 2 3e8)

Code path verified: `logp1_extended_core` (direct series below half,
`1 ⊕ x` then the `ln` core above), the `n · log1p(x)` product, then
`exp_from_extended_body_adjudicated` which runs the shared exp
reduction and Taylor and delivers through the adjudicator. The
itemization's main term is right: a relative error of 951 units in
`v = n·log1p(x)` becomes absolute through `|v| ≤` the exp gate, and
`exp` maps absolute argument error one to one into relative result
error: `≤ 1.35·10^7` units. Two imprecisions, both absorbed by the
pad: (a) the gate magnitude on the negative side is the underflow
limit (about 14,222 at Decimal128), not 14,151, a 0.5% difference;
(b) the itemization lists `EXP`'s Taylor (180) but omits `EXP`'s own
reduction term `r = v − k·ln 10` (22,200 units in `EXP`'s
itemization), which this path also incurs; it is 0.16% of the sum.
`POW` shares the formula and should be checked for the same omission
by reader A5. No Newton operation on this path, so Decimal32 is
inside the model. Verified; one P3 documentation finding (P1B-2).

### log10p1 (rung 1 15,000, rung 2 25,000)

`logp1`'s items (series at cap 250, the wide band absorption mapped
through `1/t` against the `|ln t| ≥ 0.405` floor, the `ln` decade
items whose constant multiply error is relative to a result the
constant term dominates) plus one constant multiply by `1/ln 10`
(literal good to `10^-55`, one rounding). The tiny input case needs
no anchor: the series' stopping rule drops the `u²/2` term only when
it is below half a unit of `u`, so the dropped tail is inside the
rounding. Sum about 952 units; the pad holds. Verified.

## 5. Items handed to the Wave 2 readers

- A3: the catalog's "at most 33 leading zeros" premise and the
  corrected depth (38); whether any other budget cites it.
- A4: `reduce_wide`'s 143 digit window against the derived depth; the
  dynamic reader's window formula against the same bound at `p`.
- A5: `POW`'s itemization for the omitted reduction term; `atan`
  probes in the two reduction bands; the `EXP` gate magnitude.
- A2: the two adjudicator items in §3.
