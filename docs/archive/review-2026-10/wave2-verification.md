# Wave 2 verification: the transcendental kernel

Coordinator's verdicts on the Wave 2 readers (A1, A2, A3, A4, A5),
launched at `40f8499` after the Wave 1 close. Every cited line below
was opened by the coordinator; every `reproduced` finding that
carries a P0 or P1 was rerun from the reader's scratch (the runs are
under `reproducers/W2-*`, each with the coordinator's log); severity
and closure wording are the coordinator's. Reader reports are saved
verbatim under `readers/`. Verdict vocabulary as in Wave 1:
confirmed, refuted, engineering, hypothesis.

The tree stayed clean through the wave (`git status` after every
hand-back). One reader (A4) ran cargo once with the brief's target
directory convention inverted (`target-A4` under the scratchpad, not
`A4/target`), which changes nothing.

## Wave 2 in one paragraph

Four new P0 wrong value or panic classes, all runtime witnessed and
all in code the stacks swept without seeing: `sinh`/`cosh` saturate
on `exp`'s overflow gate and return infinity for finite results on
every format (A5-1); the Decimal128 `exp` underflow gate sits inside
the region where the answer is `1E-6176`, so the nearest modes return
+0 for `exp`, `exp2`, `exp10`, `pow`, `powr` (A5-2); Decimal32
`atan2` returns wrong values through the subnormal Newton seed (A3-1);
and `atan2` has no anchor seam for tiny exact ratios, so the directed
modes return the ratio itself with the wrong cohort on all three
formats (A3 lead). The P1B-2 panic reaches Decimal128 `atan2_pi` too
(A5-3). The certification chain has a real gap: the S2 margins for
the three Decimal128 trig groups are rung 1 mirror noise, so the
README's "deepest certified margin" is false (A3-2), though every S2
output remains Arb certified. The dynamic rung inherits the P1B-1
depth premise and its p = 220 budget is measurably exceeded (A4-1).
The Newton seed class (P1B-2, fd-k9t) was found independently by four
readers (A2-2, A3-1, A3-6, A4-6, A5-6) with one closure. The
classifiers are clean on 339,012 direct and 323,000 API cases (A1),
and the adjudicator and the algebraic group's values are clean on
every probe (A2). No reader finding was refuted.

## A1: the exact and tie classifiers

Three findings, all engineering; the deliverable is the clean verdict
and the instrument that produced it (a forwarding `DecimalFormat`
adapter that runs the `pub(crate)` classifiers against the real
rounders, which is the in-tree closure for C1-11 and C1-13).

| id | verdict | ruling | note |
|---|---|---|---|
| A1-1 | confirmed | P2 | Coordinator rerun: `cargo check -p ferrodec-transcend --no-default-features --features trig-pi` emits 46 dead code warnings, `exp-log` alone 8, `trig` alone 0. No CI or battery lane builds a single feature under `-D warnings`; the lib.rs:92-94 gate comment is stale. Pairs with the Phase 0 D1/C3 seed. Closure: cfg the non pi items on `exp-log` and the pow only items on `pow`, or move `strip_trailing_zeros`; a per feature clippy lane (C5/D1). |
| A1-2 | confirmed | P3 | exact.rs:2844-2847 says "top three limbs" and tests four; nonzero `limbs[2]` proves `S' ≥ 2^256`, sound since `2^256 > 10^70`. exact.rs:2775 "exactly one of the two shifts nonzero" is false when `qw = qz`. Doc. |
| A1-3 | confirmed | P3 | exact.rs:64-65 promises a proof at every bail site; `log2_exact` and `log10_exact` carry none at the lines named. The header's function list and count disagree. Doc. |

Found clean, adopted: every classifier's theorem and bail sites
(dossier in the report), 339,012 direct classifier calls and
323,000 public API calls on three formats and five modes against an
integer arithmetic oracle validated on planted errors, including
~107,300 calls under `force_escalate` + `force_adjudicate` with no
`Equal` panic; the splice check of every D3 agent's `exact.rs` body
against the landing (nothing lost or interleaved; every difference
has a named cause). Blind spots the reader named and the coordinator
adopts: cohort and CLAMPED were not compared (A2-1 is exactly a
cohort defect, in the kernels' special rows rather than the
classifiers); the pow fast path bypasses the classifier (value and
flags matched, cohort not). Design note 2 (four copies of the
`2^α 5^β t → (coef, w)` assembly with identical 127/55 gates) goes to
C3.

## A2: the adjudicator and the algebraic group

Three findings; the coordinator's independent adjudicator read
(Phase 1b) and the reader's list agree: no defect in the deciders,
`Equal` unreachable on every probe, the design question stands.

| id | verdict | ruling | note |
|---|---|---|---|
| A2-1 | confirmed | P1 | Coordinator rerun of the cohort probe: seven BAD rows per format on all three formats (`powi(0E+5, 3)` = `0` want `0E+15`; `rootn(0E+6, 3)` = `0` want `0E+2`; `compound(0E-3, 2)` = `1` want `1.000000`), 187 nonzero exact rows and `hypot`'s zero rows pass. Code read: pow.rs:418-432 returns `F::ZERO`/`F::NEG_ZERO`, rootn.rs:129-144 runs the zero row before the `n = 1` identity (so rootn.rs:17-19 is false for zeros), compound.rs:149-151 returns `F::ONE`; hypot.rs:197-210 routes zeros through `pack_exact`. Premise stated: §9.2.2 is pointer only; the crate's own contract ("on every exact delivery") and its `hypot` precedent say the quantum binds. Closure: deliver the special rows through the preferred quantum pack; cohort sensitive tests. |
| A2-2 | confirmed | P1 | Coordinator rerun of the Newton probe: Decimal32 rung 1 `recip` 8.539e-26, `sqrt` 4.028e-26 relative, against itemizations of 15 units (1.5e-49) in `HYPOT`, `POWI_INT` and `RSQRT`; the rsqrt polish table's 28/56 digits versus the real 25/50; the in-tree polish test fed 50 digits fails. No value changes: the reader's exhaustive Decimal32 `powi` floor (1.5e-15 at `c = 1867094`, `n = −4`) is the same number reader A3 found by the same enumeration independently (A3-6). This is the fd-k9t class; one closure with A3-1, A3-6, A4-6, A5-6 and P1B-2 (below). |
| A2-3 | engineering | P3 | The derivation (`S < 2·10^(2δ₀+2P)`, so 2·10^104 at Decimal128) is accepted on reading; adjudicate.rs:57 says `2·10^170` and ADR-0060's "conditional for S > 10^105" band is empty. Telemetry over 51,855 edge calls: 0 adjudications. Doc plus a telemetry pin. |

Found clean, adopted: the five deciders and `cmp_aligned` traced
line by line; `round_adjudicated` and `deliver_at_boundary`; the
P1B-2 shape widened over every reciprocal, division and square root
in the five kernels (51,855 calls × six builds, zero panics, byte
identical, oracle clean); §9.2.1 dispatch on 8,190 rows; the
powering arm and `compound` on 720,000 rows; the `|y| ≤ 256` fast
path; the hypot band gate; preferred exponents on nonzero exact
rows. Blind spots: random operands far from boundaries; deciders
compared to the oracle, not to an independent integer
re-implementation. The reader's shared target directory hazard (a
`git archive` copy and the repo share unit hashes) is recorded as a
tooling lesson for the brief.

## A3: the ladder, the two fixed rungs, the certification chain

Nine findings and one lead outside scope; the lead is a P0. The
reader's six Decimal32 witnesses and the anchor seam probe were rerun
by the coordinator before the beads were filed.

| id | verdict | ruling | note |
|---|---|---|---|
| A3-1 | confirmed | **P0** (shipped wrong value) | Decimal32 `atan2(6304256E86, 9523351E90)` returns `0.00006619787` under NearestEven where Arb gives `6.6197875000000000004…e-5`, so `…788` is correct; six witnesses, 14 wrong (input, mode) pairs, identical in the `unbounded-ladder` build. Cause: `Extended::recip` (extended.rs:778-791) seeds from the format's reciprocal of the rounded operand; for `x` in `(1E96, MAX32]` that reciprocal is subnormal with five digits, and two Newton steps reach only ~1e-20 relative where the budget prices 1e-46. Decimal64 and Decimal128 subnormal seeds keep 14 and 32 digits (harmless). Bead fd-h3jk; `reproducers/W2-A3-atan2-d32`. |
| A3 lead | confirmed | **P0** (shipped wrong value and cohort) | `atan2(1E-6000, 1)` returns `1E-6000` in all five modes on Decimal128 (TowardZero must give the predecessor, as `atan(1E-6000)` does: `9.999…E-6001`); same on Decimal32 (`atan2(2E-60, 1)`) and Decimal64 (`atan2(6E-300, 3)` under TowardZero → `2E-300`). inverse_trig.rs:403-418 has no ADR-0051 anchor for a tiny exact ratio, so the 110 digit value lands exactly on `q` and rung 2 delivers it; the `unbounded-ladder` build gets the Decimal128 case right, so the two builds disagree. Bead fd-gu75. |
| A3-2 | confirmed | P1 (record and tooling) | Coordinator's own mpmath check of the README's input `9552873219007404877823877103706725E421`: true margin 2.33e-5 ULP against the label 1.774e-11. The sin, cos and tan Decimal128 S2 margins are distances measured on the rung 1 mirror (sample.rs:287-292), whose own error is ~1e-4 ULP; the frozen "hardest 50" for those groups are ranked by noise; only 55 of 2,816 trig survivors truly lie inside 1e-6 ULP. What stands: every one of the 207,862 S2 rows is an Arb certified correct output in all five modes, and the 198,534 D rows are genuine rung 2 corrections. What falls: README.md:204 and :239, ADR-0059:475-477, MANIFEST's `min_margin_ulp` for sin/cos/tan, the `.prov` labels. Closure: measure trig margins on a rung 2 or Arb value in `sample.rs` and `freeze_campaign_corpus.py`, restate the documents, and a test that every frozen row's margin matches Arb within 1%. |
| A3-3 | confirmed | P1 (record) | Settles D3-13. Coordinator's arithmetic agrees with the reader's: rung 2 drops 76 digits, each field of 10^76 units holds one grid point and one midpoint, so the band has measure `4·B₂·10^-76` per call (4e-72 for sin, 1.2e-67 for pow) and `≈ 4·B₂·10^-38` per unary domain (4e-34 for sin, 4e-30 for cbrt). ADR-0059:191-194's "about 10^-36 per function" would need `B₂ ≈ 100`, below every rung 2 budget (smallest 250); the per call restatements (README.md:266, ladder.rs:1726) are off by 31 to 35 orders in the safe direction. For `pow`, `powr`, `atan2`, `atan2_pi` the same model gives an expected exception count of order 1 to 10^7 over the domain, not small. Closure: an ADR-0059 addendum carrying the derivation in one form (per unary function `≤ 4·B₂·10^-38`; binary functions stated as counts), propagated to README, lib.rs, extended2.rs, ladder.rs; the Phase 6 report's claim wording follows this. |
| A3-4 | confirmed | P2 | Coordinator rerun of `exp_red.py`: `|round50(k·ln10) − k·ln10|` peaks at 4.999e-46 (k = 5599), measured rung 1 `exp` error 49,908 predicate units against the itemized 22,400 (ladder.rs:858-866 sizes the product at "1.5 units × magnitude"); the 250,000 budget holds (pad ×5 not ×10). The exp family, sinh, cosh and tanh inherit; POW, POWI, POWR, COMPOUND, CBRT, ROOTN omit the term (A5-9 and Phase 1b found the same omission). Doc plus a rung 1 probe test. |
| A3-5 | confirmed | P2 (tooling) | Coordinator rerun through `tools/campaign_certify.py` with the pfloat venv: the wrong side rows are rejected (misround=3); the rows with planted status cells `#00` and `#ff` pass (ok=5). The certifier never checks flags; S2's 1,039,310 cells all carry 0x10 so nothing was hidden, but future campaigns are blind on flags. The reader's exit status claim was not separately captured by the coordinator. Closure: derive expected flags from the ball and the range; `oor` with a finite production output fails. |
| A3-6 | confirmed | P1 (record) | ADR-0060:117 "smaller formats … strictly easier" and the README's "unconditional in every build" assume format independent rung error; at Decimal32 rung 1 the Newton steps carry ~1e-25. `powi` holds after the fact (the enumeration, convergent with A2); Decimal32 `hypot` is not discharged (floor ~1.2e-37, expected wrong outputs ~1e-4). Closure: the Newton class fix discharges both; ADR-0060 table note. |
| A3-7 | confirmed | P3 | `boundary_distances_at_width` has one caller, margin.rs:103, the normal width wrapper in the same module; the sampler never passes a subnormal width (margin.rs:28-31 says it does). Doc. |
| A3-8 | confirmed | P3 | extended.rs:771 "≥ 33 digits" holds only for Decimal128; the other doc items accepted on reading. Doc. |
| A3-9 | confirmed | P3 | `pub fn parse_str` (extended.rs:204) has two `panic!` arms on invalid characters; every production call passes a literal. Type: `pub(crate)` or `Option`. |
| A3-H1 | hypothesis (open) | Decimal32 `atan2_pi` through the same seed. Not split; the A3-1 closure removes the mechanism regardless. Candidate for the reserve reader. |

Found clean, adopted: the escalation predicate at both rungs on
13,593 constructed cases against libmpdec (positive values only);
the three margin definitions proven the same function at normal
width; the certifier's value path; `certify_anchor_floor.py`; the
`extended` versus `extended2` mirror (defects documentation only
plus the shared seed defect); `parse_str` reachability. The budget
table (which constants rest on derivation alone: every rung 2
constant) is adopted into the report's trust table.

## A4: argument reduction, constants, the dynamic rung

Six findings. The reader's reproducer was rerun by the coordinator
with byte identical results (`sc_result_coordinator_rerun.txt`).

| id | verdict | ruling | note |
|---|---|---|---|
| A4-1 | confirmed | P1 (unsound budget, no wrong value) | At the Ziv entry width p = 220 the working `sin` error on `±9308532438209917461067659354862169E4639` is 15,130 ulp against `sin_budget_dyn(220)` = 14,000 (ladder.rs:739-741), because `frac_digits_dyn(p) = p + 33` (argred.rs:880-882) carries P1B-1's false 33 digit premise into the dynamic rung: the fraction's absolute truncation error does not shrink with p, so for inputs deeper than 33 digits the residual's relative error never reaches 10^-(p+3). Of the widths the driver reaches only 220 is unsound (440 and 880 measured inside budget). Wrong value probability ~10^-180. Closure: `frac_digits_dyn`, `FRAC2_DIGITS` and the dynamic trig items restated from depth 37.88 (one shared depth constant per format, design note 1); a planted test at p = 220. |
| A4-1 (refinement of P1B-1) | adopted | correction to the coordinator | Phase 1b stated the rung 2 window term in relative units and called the overrun "~8×"; in the predicate's own ulps the term is `≈ 1.57·10^(-34-e_r)` (`e_r` the decade of the residual), at most ~1.6e4 against the 10,000 budget, so the overrun is 1.6× and applies only to inputs with `|r| < 10^-37` (one per exponent minimum). The finding's direction is unchanged; its size in the report is A4's. |
| A4-2 | confirmed | P2 | argred.rs:1293-1297 and :1585-1589 test `round34(π/2)` (depth 33.55) and call it the deepest input; :1668-1672 uses q = 6077 where q_max is 6111. The existing 10^-108 assertion shape fails on the P1B-1 inputs (reader's measurement). Test. |
| A4-3 | confirmed | P2 | consts.rs:219-236: the certificate's budget comment says "10^4 units of 10^-50 ≡ 10^-46" and then calls `c2.near_rounding_boundary` on the `Extended2` value, whose unit is one ulp of the 110 digit value (extended2.rs:375), so the assertion establishes ~10^-106, not the 10^-46 the interval argument at :209-214 needs. True distances are 10^-34 and larger (reader's mpmath), so the deliveries are correct; the test does not prove what its doc says (C1-13 class). Test. |
| A4-4 | confirmed | P3 | ladder.rs:360-366 counts `count_rung3_entry` in the `else` arm for every escalating rung; `ExtendedDyn::ESCALATES = true` (extended_dyn.rs:1389), so every Ziv doubling counts as an entry; the adjudicated arms never count; nothing reads the counter. Doc plus control flow. |
| A4-5 | confirmed | P3 | ladder.rs:558 "p = 57,344 after eight escalations" (56,320 is the last width that runs); argred.rs:901-903 "doubles from 110" (ZIV_START_PRECISION is 220). Doc. |
| A4-6 | confirmed | P2 | extended_dyn.rs:791-793 and :825-827 seed through `to_format`/`from_format` exactly as the fixed rungs do: the dynamic mirror of P1B-2. Rung 1 panics first today; a fix limited to the fixed rungs leaves it latent. Same closure as the class. |

Found clean, adopted: the 6,408 digit `2/π` table digit by digit
against the generator and mpmath (closes C1-15 for this table), with
117 digits of headroom over the deepest rung 2 read; the window
arithmetic in all three readers on the extremes and the near `k·π/2`
rows (161/161 `k mod 4` correct); `reduce_wide` equals `reduce_dyn`
at p = 110 byte for byte; `consts.rs` consumption per rung;
`DynArena` borrow discipline; `run3` termination. Blind spots: tan at
rung 2 for a deep odd k input (the derived window term 3.66e4 exceeds
TAN's 25,000; no such input found among the per exponent minima); the
398 line test hunk of the dynamic mirror not read; arena memory at
the top widths estimated, not measured.

## A5: per function kernels and the root wrapper layer

Ten findings and two hypotheses. The driver and oracle were rerun by
the coordinator for A5-1, A5-2, A5-3, A5-4, A5-5 and the H-2 split.

| id | verdict | ruling | note |
|---|---|---|---|
| A5-1 | confirmed | **P0** (shipped wrong value) | `sinh(14150.01)` at Decimal128 returns Infinity where mpmath gives 9.3375e6144; `cosh(14150.07)` Infinity (true 9.9149e6144); Decimal64 `sinh(887.1)`, Decimal32 `sinh(224.01)` the same; every mode (±MAX toward zero). hyperbolic.rs:172 and :221 saturate on `exp`'s overflow gate though `e^x/2` overflows `ln 2` later (true thresholds 14150.0785, 887.1884, 224.0439 against gates 14150, 887, 224). Exhaustive at Decimal32: 4,390 of 4,390 cells wrong in the band. Bead fd-b6zt; `reproducers/W2-A5-gates`. |
| A5-2 | confirmed | **P0** (shipped wrong value) | Decimal128 `exp(-14221.3)` and `pow(10, -6176.3)` return +0 under NearestEven and NearestAway where the true value (7.157e-6177 and 5.012e-6177) exceeds half the smallest subnormal, so `1E-6176` is correct; coordinator's mpmath: `ln(0.5·10^-6176)` = −14221.4587, format_impl.rs:150 derives it as "≈ −14220.85" (the `ln 2` added with the wrong sign) and :163-165 sets the gate at 14221. `exp2`, `exp10`, `pow`, `powr` inherit; Decimal64 (918 vs −917.12) and Decimal32 (235 vs −233.25) gates are safe. Bead fd-mq5e. |
| A5-3 | confirmed | **P0**, row of fd-0o9 | `atan2_pi(1E-6141, 1E-6176)` panics at Decimal128 in all five modes (inverse_trig_pi.rs:496-522: gap 35 < P+2 so the anchor does not fire; :519 → inverse_trig.rs:544 `div` → extended.rs:781 seed +∞ → :168 panic). The coordinator's P1B-2 probe points all hit the anchor first, which is why that table says "not reproduced at the probed points". The reader's class closure (scale the operand inside the three Newton primitives) is adopted as the class closure below. |
| A5-4 | confirmed | P1 | `pow(±0, −∞)` returns `+∞` with DIV_BY_ZERO on all three formats (coordinator rerun, root and siblings); `powr(±0, −∞)` raises nothing. pow.rs:144-157 tests only the sign of `y`; §9.2.1 reserves divideByZero for finite `y < 0`. Closure: control flow (infinite `y` first) and a test per format. H-2 split: the siblings reproduce it through their own `pow_special_cases` copy (ferrodec-decimal64/src/ops/pow.rs:161), so B2 owns the sibling fix. |
| A5-5 | confirmed | P1 (spec reading for Parnell) | Root `pow(sNaN, ±0)` and `pow(1, sNaN)` deliver `1` with INVALID (coordinator rerun: `1#1`); Decimal64 delivers `1` with no flag (`1#0`, C2-3). pow.rs:107-116's own comment says 754 makes `pow(x, ±0) = 1` "even when x is sNaN" and raises INVALID "conservatively"; the reader reads §9.2.1 as naming only a quiet NaN, which makes §7.2 govern (qNaN plus INVALID). Both readings are defensible from memory of a pointer only standard; the coordinator records the three way divergence as the defect and the reading as a decision bead for Parnell (C23 Annex F returns 1 for any NaN and has no signaling distinction). Closure: one shared special dispatcher for the three formats (design note 4) once the reading is fixed. |
| A5-6 | confirmed | P1 (the Newton class, named in full) | `recip` of an operand above MAX silently returns 0 (seed `1/∞ = 0`); `sqrt` of an operand below the range panics; a subnormal seed caps the result at `e₀⁴` (1e-20 at Decimal32 rung 1). `Extended::recip`/`sqrt` are `pub` on a published crate with "caller must ensure non-zero" as their only contract. Together with P1B-2 (fd-0o9), fd-k9t, A2-2, A3-1, A3-6, A4-6 this is one defect with one closure: the three Newton primitives (`Extended`, `Extended2`, `ExtendedDyn`) scale the operand to `[1, 10)` (recip) or by an even power of ten (sqrt) before `to_format`, as `pow::working_reciprocal` already does, and derive the step count from `F::PRECISION` as `extended_dyn::newton_steps` does. That removes the five panics, the silent zero, the Decimal32 wrong values and the false itemizations at zero budget cost. |
| A5-7 | confirmed | P2 (campaign) | sample.rs:355-372 marks every infinite, zero or subnormal production result non normal and sweep.rs:120-123 discards it before the mirror runs, so S2 swept the sinh/cosh and exp underflow bands (sample.rs:177) and could not see A5-1 or A5-2 by construction. Closure: certify saturated outputs in the gate strata. |
| A5-8 | accepted on the reader's log | P2 (instrument) | The C1-1 blast radius measured: a TowardPositive one ulp low mutant of `sinh` passes every property suite, every frozen vector suite and `transcend_directed` on all formats; only the anchor bands and the root S2/planted suites turn red (19 failing suite results across the two mutants in `mutant_iso.log`). Not rerun by the coordinator (about 15 minutes per mutant). Closure: directed rows for the eleven NE only functions (C1-5) and the two sided oracle (C1-1). |
| A5-9 | engineering | P3 | POW/POWR/CBRT itemizations omit EXP's reduction term (A3-4, Phase 1b). Doc. |
| A5-10 | confirmed | P3 | hyperbolic.rs:162-165 and format_impl.rs:150 say what the reader says; the d64 "≈ −916.98" (true −917.122, gate safe) goes to B2. Doc. |
| A5-H1 | superseded | A3-1 is the witness. |
| A5-H2 | confirmed | The sibling `pow` wrappers carry their own `pow_special_cases` (d64 ops/pow.rs:161) with the same rule order; `d64 pow 0 -Inf` → `Infinity#2`. |

Found clean, adopted: the five mode differential against mpmath on
112,014 inputs over every function in scope on all three formats
(covering S2's never swept list at sampled depth); the §9.2.1 tables
at the root apart from A5-4 and A5-5; every anchor threshold
re-derived; negate after round on every odd function; the pow fast
path (no double rounding); the root rustdoc theorems. Blind spots:
cases within 1e-40 of a boundary (no non saturating cell came that
close), flags other than INEXACT, rungs 2 and 3 not forced, the d32
tan asymptote probe (7.8 CPU hours, no zero `cos_ext`, upper decades
unfinished).

## Convergences and corrections

- The Newton class was found by four readers from four directions
  (A2 measured it, A3 witnessed the wrong values, A4 found the
  dynamic mirror, A5 named the silent zero); two readers (A2, A3)
  ran the same exhaustive Decimal32 `powi` enumeration without
  knowing it and got the same minimum. The ownership rule's cost
  bought that independence.
- Three readers (Phase 1b, A3-4, A5-9) found the EXP reduction term
  missing from the powering itemizations; A3 additionally mis-sized
  EXP's own term and measured it.
- The P1B-1 depth premise recurs as A3-2 (the mirror's own error
  swamps the trig margins), A4-1 (the dynamic rung), and A4-2 (the
  tests that cannot see it). One depth constant per format closes
  all three.
- Corrections to the coordinator's record: the P1B-1 overrun is 1.6×
  in predicate ulps, not 8× (A4); the P1B-2 table's "atan2_pi not
  reproduced at Decimal128" was a probe choice, not a fact (A5-3).
- The Tier 2 figure (D3-13) is settled by A3-3: the ADR states a per
  domain count that is wrong by 10^2 to 10^7 in the unsafe
  direction for unary functions and is not small for the binary ones;
  the per call restatements are wrong in the safe direction.

## Why the stacks missed the Wave 2 P0s

S2 discarded every non normal production result (A5-7), so the two
gate defects sat inside swept bands unseen; S2 never swept Decimal32
or `atan2`, and the Decimal32 exhaustive program covers unary
functions only (A3-1); the trig margins were measured on an
instrument whose own error exceeds the quantity measured (A3-2); the
property oracles are one sided and eleven functions have no directed
rows below Decimal128 (A5-8); the frozen corpora resolve the format
width only, so a Tier 1 overrun of 10^4 ulp at 220 digits is 10^182
ulp below what any row can see (A4-1); `ladder_audit` checks the
working value against the stated budget, never the budget against the
true error (A2-2).

## Handed to Waves 3 and 4

- B1: the zero result cohort class (A2-1) for the arithmetic
  operations' zero rows under §9.2.2; the fma binade class (C2-1)
  for `add`/`sub` at power of ten gaps.
- B2: the siblings' `pow_special_cases` copies (A5-4, A5-5, C2-3:
  three formats, three behaviours); the d64 `transcend_impl.rs`
  underflow comment (A5-10); the gate constants per format against
  `ln(k·MAX)`.
- C3: the three Newton copies and the four `2^α 5^β` assemblies
  (A1 design note 2, A5 design note 1); the literal 33 in more than
  eight places (A4 design note 1); `exact.rs` under `trig-pi` alone
  (A1-1).
- D1: `Extended::recip`, `sqrt`, `parse_str` public on a published
  crate (A3-9, A5-6); the `telemetry` counter semantics (A4-4); a per
  feature clippy lane.
- Reserve: A3-H1 (Decimal32 `atan2_pi`), the tan rung 2 deep odd k
  question (A4), and the Decimal32 `hypot` floor (A3-6) if the
  Newton class is not closed before Phase 6.

## Wave 2 summary

- P0, four new classes plus one new row: `sinh`/`cosh` overflow gate
  (fd-b6zt), Decimal128 `exp` underflow gate (fd-mq5e), Decimal32
  `atan2` through the subnormal seed (fd-h3jk), the `atan2` anchor
  seam (fd-gu75), Decimal128 `atan2_pi` panic (fd-0o9). Six P0 beads
  are now open across the arc.
- P1: the zero row cohorts (A2-1), the Newton class as an unsound
  argument (A2-2, A3-6, A5-6), the trig margins (A3-2), the Tier 2
  figure (A3-3), the dynamic rung's p = 220 budget (A4-1), `pow` at
  `(±0, −∞)` (A5-4) and with a signaling NaN (A5-5).
- Refutation rate: no finding refuted; one reader hypothesis
  superseded by another reader's witness; two corrections to the
  coordinator's own Phase 1b record. Clean verdicts adopted with
  their stated blind spots.
- The classifiers, the adjudicator, the algebraic group's values,
  the escalation predicate, the `2/π` table, the window arithmetic
  and the special tables (apart from `pow`) are clean under the
  methods named above.
