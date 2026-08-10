# D4 design pass brief: the pi scaled family (fd-4zo.26)

Status: brief for the blocking design pass with Parnell. Nothing here
is implemented; the decisions taken at the pass become the D4 ADR and
the binding per operation specs, on the fd-jxk checkpoint's process
precedent. Facts below are checked against the tree or a primary
source and say so; derivation sketches are labeled as sketches and
carry a spec time proof obligation. Operations covered: `sinPi`,
`cosPi`, `tanPi`, `asinPi`, `acosPi`, `atanPi`, `atan2Pi`
(IEEE 754-2019 §9.2).

## 1. Resolved facts (checked, not recalled)

- **The MPFR gate maps natively.** The plan flagged "compose
  `sin(pi·x)` or map `mpfr_sinpi`" as a real fork. It is closed by
  inspection: the lockfile vendors `gmp-mpfr-sys 1.7.1`, whose
  bundled MPFR is 4.2.2 (`mpfr-4.2.2-c/VERSION`), and `rug 1.30.0`
  exposes all seven operations natively with explicit rounding
  variants (`sin_pi_round` and siblings, verified in the crate
  source). The gate keeps the exact reduction end to end; no
  composition arm exists and none should be written.
- **Argument reduction is exact decimal arithmetic.** `sinPi(x)`
  reduces by `x mod 2` on the operand's own decimal digits: no pi
  constant, no Payne and Hanek window, no truncation item. The S1
  thin spot that dominates the trig budgets (the 10^13 unit
  reduction term on `SIN.rung1`) has no analog here. Consequence
  checked against the format: every format value whose quantum is
  `>= 1` is an integer, so the whole magnitude range past the
  precision width classifies exactly and the working domain is
  `|x| < 10^P`.
- **The D1 parity table has no in tree home.** The plan references a
  `tanPi` parity extraction made during D1; it was never promoted.
  The spec re-extracts §9.2.1 row by row from the standard at spec
  time (the D3 discipline), rather than trusting the reference.

## 2. Exact and tie classification (sketches; each line is a spec time proof obligation)

Every format value is a rational `p/10^k`. Niven's theorem
(registry: `niven-irrational-numbers`) gives the complete rational
value inventory for the forward family, and the decimal
representability of the special abscissas does the rest. The decimal
twist does real work here: `1/6` and `1/3` terminate in no decimal
format, so entire columns of the textbook table are unreachable from
format inputs.

| op | exact inputs (sketch) | values | ties |
|---|---|---|---|
| `sinPi` | integers; half integers | `±0` (parity sign rules); `±1` | none |
| `cosPi` | integers; half integers | `±1`; `+0` (§9.2.1 fixes the sign) | none |
| `tanPi` | integers; quarter integers; half integers | `±0`; `±1`; `±∞` with `DIV_BY_ZERO` (parity table) | none |
| `asinPi` | `0`, `±1/2`, `±1` | `±0`, inexact `±1/6`, `±1/2` | none |
| `acosPi` | `0`, `±1/2`, `±1` | `1/2` exact, inexact `1/3`/`2/3`, `+0`/`1` | none |
| `atanPi` | `0`, `±1` | `±0`, `±1/4` exact | none |
| `atan2Pi` | axes and `|y| = |x|` diagonals | multiples of `1/4` and `1/2`, all exact | none |

Load bearing observations to prove at spec time:

- `sinPi` and `cosPi` gain **no** exact `±1/2` family: it needs
  `x = k ± 1/6` (sin) or `k ± 1/3` (cos), which no format value is.
  The classifier completeness proof is therefore two residue checks
  (integer, half integer) plus Niven for everything else. `tanPi`'s
  quarter integers ARE representable (`0.25`, `0.75` and their
  translates), so it alone gains the `±1` family.
- `asinPi(±1/2) = ±1/6` and `acosPi(±1/2) = 1/3` are the
  non-terminating rational class: never exact, never a tie
  (`rsqrt`'s `1/q` argument, reused verbatim), but the INPUTS are
  representable and common, so the corpus must carry them.
- **No ties anywhere** (sketch): a nearest mode midpoint is
  rational; by Niven the forward family's rational values are only
  `{0, ±1/2, ±1}` and each is representable (grid, not midpoint);
  the inverse family's rational values are the finite tables above.
  If this proof closes, `ladder_audit` is vacuous for the family by
  construction and the corpus generator's tie machinery has nothing
  to do. A cleaner exactness story than any group shipped so far.

## 3. The anchor disposition (the pass's central fork)

The plan's warning was "the anchor lessons recur with force". The
sketch geometry per function, with my recommended disposition:

| family | geometry (sketch) | recommended disposition |
|---|---|---|
| `cosPi` near integers | hugs `±1` at distance `(πδ)²/2`, `δ` the exact fractional part; unbounded hug as `δ` shrinks | ADR-0051 residual channel at the `±1` anchor; side theorem `cos < 1` strictly off the exact set (and `> −1` mirrored) |
| `tanPi` near quarter integers | hugs `±1` at distance `≈ 2πδ`, linear; sub ulp once `δ` is below `~10^−34` | ADR-0051 residual channel at `±1`; side from `δ`'s sign and the quadrant parity |
| `atanPi` at large `\|x\|` | hugs `±1/2` at distance `1/(π\|x\|)`, unbounded | ADR-0051 residual at `±1/2`; side theorem `\|atanPi\| < 1/2` |
| `acosPi` at tiny `\|x\|` | hugs `1/2` at distance `x/π`, unbounded | ADR-0051 residual at `1/2`; side is `x`'s sign |
| `atan2Pi` | the `atanPi` hugs recur per quadrant at `±1/2`, `±1`, and the axes' exact rows absorb the rest | residual channels mirroring `atanPi`, quadrant reflected |
| `sinPi` at tiny `x`, and near integers | value `≈ ±πδ`: slope `π`, no grid anchor (the `log2p1` precedent: a slope `≠ 1` value is generic) | plain ladder; NOT an anchor family |
| `asinPi` at tiny `x` | value `≈ x/π`: slope `1/π`, no anchor | plain ladder |
| `tanPi` near half integers | value `≈ −1/(πδ)`: large and generic, and it provably NEVER overflows — representing `n + 1/2 + δ` forces `δ ≥ 10^(adj − P + 1)`, capping the pole value near `10^P` scale (`~3.2×10^33` at Decimal128), decades inside the format ceiling | plain ladder, no overflow gate at all; the cap is a spec time proof, and its absence from the kernel is deliberate, not an omission |

The inversion to keep in view: the D3 review's repeated finding was
anchor families discovered late (`compound`'s huge `x`, `hypot`'s
band). Here the candidate list is short and enumerable up front
because the exact reduction pins every neighborhood to an exact
`δ`; the pass should bless the list as CLOSED, and the specs then
prove each row's margin table the way `hypot`'s band constants were
proven. A family found missing later is a spec defect, not a
surprise.

## 4. The claim tier (an honest negative result up front)

`sin(πp/q)` is algebraic, but of degree that grows with `φ(2q)`;
at `q = 10^34` the minimal polynomial degree is astronomically past
any fixed width integer comparison. The ADR-0060 adjudicator route
is therefore CLOSED for this family, the powr shape recurring: no
useful uniform floor, no unconditional tier beyond the classified
and anchored sets. The honest claim is the trig tier language WITH
two upgrades and no reduction caveat:

- Tier 1 by construction and the Tier 2 model, with budgets roughly
  three decimal orders tighter than `sin`/`cos` (the itemization
  loses the reduction truncation entirely; what remains is the
  `πx` const multiply and the series). Sketch: rung 1 budgets near
  `10^4` against trig's `1.5×10^14`, escalation `~10^−12` per call
  against trig's 3%. Users get the fast, never escalating trig.
- The exact set and every blessed anchor family are unconditional,
  and the no-ties proof (if it closes) makes `ladder_audit`
  vacuous by construction.

The S5 spike does NOT gain this family: its algebraicity is real
but useless, and that sentence belongs in the ADR so nobody
relitigates it.

## 5. Naming (identity bearing; one decision)

House rule: Rust std spelling where std has a name; otherwise the
nearest ecosystem convention, IEEE name as doc alias. Std has none
of these. C23 names them `sinpi`/`cospi`/`tanpi`/`asinpi`/`acospi`/
`atanpi`/`atan2pi`; CUDA and Julia agree. Recommendation: snake case
the C23 names exactly as std snake cased `expm1` into `exp_m1`:

> `sin_pi`, `cos_pi`, `tan_pi`, `asin_pi`, `acos_pi`, `atan_pi`,
> `atan2_pi`, each with `#[doc(alias = "sinPi")]` and
> `#[doc(alias = "sinpi")]` so both the IEEE and C23 spellings
> resolve in rustdoc search.

## 6. Feature gate (one decision, user visible)

The family needs none of `trig`'s machinery: no reduction tables,
no pi constants at working width beyond the one `π` multiply, no
`argred`. Recommendation: its own `trig-pi` feature that does NOT
imply `trig`. The embedded user doing revolutions based geometry
(the dominant real world use of these functions) gets correctly
rounded `sin_pi` without paying for Payne and Hanek tables on a
Cortex-M0+; the thumbv6m size delta gets measured and stated. The
alternative (riding `trig`) couples the family to exactly the
machinery its existence avoids. Inversion: a user expecting `sin_pi`
under `trig` finds it one feature away; the README's feature table
and the rustdoc make the split loud.

## 7. Instruments and process (mostly settled by precedent)

- Corpus: `gen_transcend_vectors.py` gains the seven functions via
  `--funcs` with names appended after every legacy name (stream
  stability); its exact output filter learns §2's tables; the
  planted families are the anchor neighborhoods at exact `δ` decades
  (`cosPi(1 + 10^−k)` and friends) plus `asinPi(±1/2)`'s
  non-terminating `1/6`.
- MPFR gate: native `*_pi` arms (fact §1), the same three legged
  posture as D3 (agent suites, Arb corpus, MPFR).
- Budgets: itemized fresh per the ADR-0050 discipline; the
  reduction item is provably zero, which the itemization should
  state rather than omit.
- Process: the validated pattern at seven agents in worktrees under
  binding specs; supervisor owns derivations, budgets, side
  theorems, naming, coherence; first action baseline hash gate;
  deterministic rebuild for `exact.rs` splices; one signed merge
  for the group (the charter's "each group is one merge").

## 8. The decision list for the pass

1. Bless §3's anchor disposition table as the CLOSED list
   (recommended) or send any row back for rework.
2. Bless the no-ties claim as a spec obligation (recommended: it is
   the family's cleanest property and makes `ladder_audit` vacuous
   by construction).
3. Accept the tier statement in §4, explicitly recording the
   adjudicator route as closed for this family (recommended).
4. Naming per §5 (recommended: `sin_pi` family, dual doc aliases).
5. Feature gate per §6 (recommended: standalone `trig-pi`).
6. Scope: all seven in one group and one merge (recommended,
   matching the charter), or split forward/inverse into two merges.
7. `atan2Pi`'s §9.2.2 preferred exponent and special table get the
   same re-extraction treatment as `tanPi`'s parity table (fact §1:
   nothing in tree to trust).

Related: `docs/decisions/plans/2026-07-25-correctly-rounded-d128-transcendentals-lane.md`
(the D4 section this brief expands), ADR-0051 (residual channel),
ADR-0059 (ladder, tiers), ADR-0060 (the adjudicator whose route §4
closes for this family), `niven-irrational-numbers` and `arb-flint`
(registry).
