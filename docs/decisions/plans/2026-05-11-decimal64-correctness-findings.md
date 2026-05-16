# Phase 1 findings: ferrodec-decimal64 correctness review

> **Status**: Phase 1 deliverable, landed 2026-05-11.
>
> Six general purpose agents audited the decimal64 op surface in
> parallel under the discipline named in
> `docs/decisions/plans/2026-05-11-decimal64-correctness.md` (provenance:
> spec only, no Intel decimal library or IBM decNumber recall;
> security audit on `debug_assert!` sites; Garner / Gopen prose).
> Output: prioritized findings list. **No source changes lived during
> Phase 1.** Phase 2..N consumes this doc as the fix work order.

## Workspace totals

| Agent | Domain | H | M | L | Total |
|---|---|---|---|---|---|
| 1 | addsub + round | 3 | 2 | 5 | 10 |
| 2 | mul + div + rem | 5 | 3 | 4 | 12 |
| 3 | fma | 5 | 3 | 4 | 12 |
| 4 | sqrt + quantum | 1 | 1 | 3 | 5 |
| 5 | parse + Display + conversions | 2 | 3 | 6 | 11 |
| 6 | transcendentals + Kani | 0 | 10 | 1 | 11 |
| **workspace total (pre dedup)** | | **16** | **22** | **23** | **61** |

After cross agent dedup the unique findings count is 9 H, 14 M, 17 L
(see "Cross agent dedup map" and "Phase 2 work order" below).

## Cross agent dedup map

Findings that surface under multiple agents around the same root cause
collapse to one Phase 2 fix item.

- **`pack_finite` precondition family** (input derived arithmetic
  feeding `debug_assert!`): Agent 1 F4, Agent 3 F1 + F2 + F8 + F9,
  Agent 4 quantize H finding, Agent 6 F5. All trace to the same fix
  shape (typed `BiasedExp` + `Coefficient` newtypes in `bid.rs`).
  Consolidated as **H3 family** in the Phase 2 work order.
- **H2 effective subtraction residue mis attribution**: Agent 1 F2
  (root in `addsub.rs:158`), Agent 3 F4 (FMA early return mirror).
  Same algorithmic bug at two sites; both need the
  `effective_sub` split. Consolidated as **H2** below.
- **H1 magnitude loss on asymmetric zero operand**: Agent 1 F1 and F3
  (cancellation degenerate case). Same root path in `addsub.rs`.
  Consolidated as **H1** below.
- **Missing IEEE `Clamped` informational flag**: Agent 1 F7, Agent 2
  rem and div findings. `ferrodec-ieee::Status` carries only the five
  IEEE mandatory flags; adding `Clamped` is a workspace level change
  (extend `ferrodec-ieee`) or accept the gap. Recorded as a separate
  decision below.
- **Decimal128 H3 / H4 (non canonical Form A) does not propagate**:
  Agent 5 confirmed decimal64's Form A coefficient cap is naturally
  inside `COEFFICIENT_LIMIT` by encoding (`coef_high3 << 50 | T_MASK
  < 2^53 < 10^16`). No analogue finding. Recorded as audit clean.
- **Decimal128 M2 `sqrt(±0)` quantum already mirrored in
  decimal64**: Agent 4 confirmed `sqrt_special_cases::Zero` already
  uses `exp.div_euclid(2)`. No analogue finding. Recorded as audit
  clean.
- **Decimal128 H1 / H2 (pow + parse)**: these were spec section
  anchors only; no direct decimal64 mirror surfaced in Phase 1.

---

## Agent 1 — addsub + round

10 findings. Per agent budget 1900 words.

### F1: far alignment magnitude loss when one operand is zero
* Tier: **H**
* Reproducer: `ddAdd.decTest:358` (`ddadd360`): `add 0E+50 10000E+1` under `half_up`. Spec wants `1.0000E+5`; ferrodec returns `0E+50`.
* Symptom: when one operand has coefficient zero and the other is finite nonzero with an exponent gap exceeding `WORKING_PRECISION = 23`, the result keeps the zero operand's exponent and loses the other operand's magnitude entirely.
* Mechanism: `add_inner` (`addsub.rs:128`) sorts operands by exponent, so the zero operand becomes `hi` (it has the higher exponent). Line 152's `diff > WORKING_PRECISION` branch sets `aligned_hi = u128::from(coef_hi) = 0`, `aligned_lo = 0`, `pre_sticky = coef_lo != 0`. The same sign branch at line 156 then computes `combined_coef = 0 + 0 = 0` and routes to `round_and_pack_into_u64(0, align_exp = 50, q_preferred = 1, false, true, rm)`. `round_and_pack_finite` sees `coef == 0` with `pre_sticky == true`, skips the fast path (line 68), produces `kept = 0`, runs `finalise_finite` which honours the zero short circuit at line 172 and emits a canonical zero. IEEE 754-2019 §5.4.1 requires `x + 0 = x` (with quantum bumped to `min(quantum(x), quantum(0))`). The early "both zero" return at line 115 only fires when both coefficients are zero; the asymmetric case falls into the alignment path that collapses both sides to zero.
* Fix shape: short circuit at the top of `add_inner` when exactly one operand has `coef == 0`: return the other operand requantised to `q_preferred = min(exp_a, exp_b)` via `round_and_pack_finite(coef_other, exp_other, q_preferred, sign_other, false, rm, OK)`. The same path closes the F3 cancellation degenerate case below.
* Provenance: `addsub.rs:115`, `addsub.rs:128..154`, `addsub.rs:156..176`, IEEE 754-2019 §5.4.1 + §6.3.

### F2: effective subtract sub ULP residue mis attributed as positive sticky
* Tier: **H**
* Reproducer: `ddAdd.decTest:802` (`ddadd71100`): `add 1e+2 -1e-383` under `down` (TowardZero). Spec wants `99.99999999999999 Rounded Inexact`; ferrodec returns `100.0000000000000`. 19 sibling cases through `ddadd71119` (rounding directives `down` and `ceiling`, not `half_even` as the original KNOWN_ISSUES H2 entry claimed). One mirror in `ddMultiply.decTest`; 20 in `ddFMA.decTest` per Phase 0's scaffold.
* Symptom: when an effective subtraction's smaller operand falls below the working window, the residue is recorded as `pre_sticky = true` on the result. The residue's true effect is to *decrement* `combined_coef` by a sub ULP amount; the current encoding (sticky bit on the unchanged `aligned_hi - aligned_lo`) treats it as a sub ULP *addition*. Directional rounding modes round the wrong way: TowardZero on a positive result leaves the over estimate instead of dropping to the `(combined_coef - 1)` cohort whose top digit is `9...9`.
* Mechanism: in `add_inner`, when signs differ and `aligned_hi > aligned_lo` and `pre_sticky = true` (the lo operand's coefficient was either truncated by the `22 < diff ≤ 23` branch or entirely below the working window in the `diff > 23` branch), the code at `addsub.rs:158` computes `combined_coef = aligned_hi - aligned_lo` and passes `pre_sticky = true` forward. The true value is `(aligned_hi - aligned_lo) - ε` where `ε ∈ (0, 1)` ULP at `align_exp`. `should_round_up` reads sticky as "residue above the dropped LSB" and rounds away accordingly. The right shape is the two candidate model decimal128 implements: decrement `combined_coef` by 1, multiply up to `combined_coef × 10 - 1` worth of nines in the digit positions that opened up, then route to `round_and_pack_finite` with sticky representing the residue inside the new low ULP.
* Affects: `addsub + mul + fma` (the rounding consumers share the same effective subtract residue path through `round_and_pack_into_u64` and `round_and_pack_finite`).
* Fix shape: in the effective subtract branches at `addsub.rs:158..160`, when `pre_sticky == true` and the residue originated from the lo side, borrow one ULP from `combined_coef` and re extend the low digits to `PRECISION` nines before rounding. Cross check the `aligned_hi == aligned_lo` arm at line 161, which under the same conditions returns `1 × 10^exp_lo` and similarly under represents the residue. Cross listed with Agent 3 F4.
* Provenance: `addsub.rs:136..154`, `addsub.rs:156..176`, `round.rs:79..125`, IEEE 754-2019 §5.4.1 + §4.3. Behavior cross checked against Decimal128 commits `5780569` (addsub) and `105fe40` (FMA mirror); no code shape lifted.

### F3: `aligned_hi == aligned_lo` opposite sign residue under represents magnitude
* Tier: **H** (degenerate case of F1 + F2)
* Reproducer: any `add x -y` where `|x| == |y|` at align_exp granularity but the lo side has a sub align_exp residue. Seed: property test under signs differ effective subtract with operands whose first 16 digits match exactly and lo carries truncated low digits.
* Symptom: result returns `1 × 10^exp_lo` rather than the actual residue magnitude (off by up to `10^trim` ULP at `exp_lo`).
* Mechanism: `addsub.rs:161..175` handles the `aligned_hi == aligned_lo` cancellation arm; with `pre_sticky == true` it claims the result is `1 × 10^exp_lo` and routes through `round_and_pack_into_u64`. The truth is the residue is somewhere in `(0, 10^trim × 10^exp_lo)`.
* Fix shape: do not assume the residue is a single sub LSB ULP. Pass the actual lo side residue forward.
* Provenance: `addsub.rs:161..175`, IEEE 754-2019 §5.4.1.

### F4: `pack_finite` precondition admits release mode garbage bits
* Tier: **H** (security family; consolidated under H3 in the work order)
* Reproducer: any path where `finalise_finite` reaches the final `pack_finite(sign, biased as u32, coef)` at `round.rs:236` with a biased value that was not validated against `BIASED_EXP_MAX`. The function checks `biased > biased_exp_max` (line 180) and `biased < 0` (line 215) up front, so the final fall through should hold the invariant; defensive concern is uniform across the `bid.rs` `pack_finite` callers.
* Symptom: under release, an invariant break produces garbage bits via `pack_finite`'s out of range biased_exp encoding. Same hazard class as H3 (Phase 0 finding).
* Mechanism: `pack_finite` at `bid.rs:214..233` is a `const fn` with two `debug_assert!`s on its preconditions. Release builds drop them; the bit packing proceeds with truncated `biased_exp` bits.
* Fix shape: typed `BiasedExp` newtype per the slice plan's H3 preferred shape. Apply uniformly across the `pack_finite` call sites in `round.rs:73`, `round.rs:175`, `round.rs:195`, `round.rs:210`, `round.rs:226`, `round.rs:229`, `round.rs:236`, `addsub.rs:119`, `addsub.rs:169`.
* Provenance: slice plan §"H3 fix shape", global CLAUDE.md security posture.

### F5: `round_and_pack_into_u64` debug_assertions on loop invariant terms
* Tier: **L**
* Reproducer: none; static analysis only.
* Symptom: `addsub.rs:234..235` debug_asserts `c < keep_threshold` and `c <= u128::from(u64::MAX)` after a `while c >= keep_threshold` loop. Both are tautologies of the loop guard.
* Fix shape: drop the asserts; add a `// loop exits with c < keep_threshold = 10^19 < u64::MAX` comment if the cast needs explanation.
* Provenance: `addsub.rs:226..235`.

### F6: `zero_sum_sign` accepts post `neg` signs for `sub` without doc record
* Tier: **L**
* Reproducer: none; spec letter check.
* Symptom: `Decimal64::sub(self, other)` negates `other` before dispatching, so sNaN propagation sign is the post negation sign. IEEE 754-2019 §6.2.3 leaves NaN sign propagation implementation defined; the chosen convention is internally consistent but undocumented.
* Fix shape: add a one line doc comment on `Decimal64::sub` recording the convention.
* Provenance: `addsub.rs:57..61`, `addsub.rs:75..82`, IEEE 754-2019 §6.2.3.

### F7: `finalise_finite` clamping path silent on the Clamped status flag
* Tier: **M**
* Reproducer: `ddFMA.decTest:281` (`ddfma2504`) per KNOWN_ISSUES H3; many `ddAdd` cases of the shape `add 1E+384 0` where the result is `1.000...0E+384`.
* Symptom: `finalise_finite` (`round.rs:180..199`) performs the §6.3 exponent clamping pad of trailing zeros and returns without raising any status. The IEEE 754-2019 "Clamped" condition is informational; the conformance harness checks for it.
* Mechanism: line 192 to 198 returns `(decimal, status)` with `status` unchanged. `ferrodec_ieee::Status` does not carry a `CLAMPED` flag.
* Fix shape: workspace level decision. Either extend `ferrodec-ieee::Status` with a `CLAMPED` flag and emit at the clamp sites, or accept the omission and document it. See "Workspace decisions" below.
* Provenance: `round.rs:180..199`, IEEE 754-2019 §6.3 + §7.3.

### F8: `kept_digits` not updated when round up triggers the COEFFICIENT_LIMIT renormalisation
* Tier: **L**
* Reproducer: none observable; defensive.
* Symptom: `round.rs:96..106` increments `rounded`, then if `rounded >= COEFFICIENT_LIMIT` it divides by 10 and bumps `exp_after`. `kept_digits` is not updated. The invariant holds today but by accident; future refactor breaks it silently.
* Fix shape: recompute `kept_digits = digit_count_u64(rounded)` after the round up block, or document the invariant.
* Provenance: `round.rs:94..125`.

### F9: cohort lowering exit condition is undocumented
* Tier: **L**
* Reproducer: none; minor.
* Symptom: the strip trailing zeros loop at `round.rs:117..123` exits when `rounded % 10 != 0`. Spec correct (IEEE 754-2019 §5.4.1 preferred quantum), but the loop's exit condition is not explained.
* Fix shape: add a `// q_preferred preferred but not representable; keep nearer cohort` comment.
* Provenance: `round.rs:117..124`.

### F10: TowardZero overflow path encodes constants through a `debug_assert!`
* Tier: **L**
* Reproducer: none.
* Symptom: at `round.rs:210`, `pack_finite(sign, BIASED_EXP_MAX, COEFFICIENT_LIMIT - 1)`. Both arguments are crate constants, but the call still triggers the `debug_assert!` machinery.
* Fix shape: subsumed by F4 (typed `BiasedExp` proves the precondition statically).
* Provenance: `round.rs:208..212`.

---

## Agent 2 — mul + div + rem

12 findings. Per agent budget 1950 words.

### M1: rem panics in `pack_finite` precondition on `ddrem424`
* Tier: **H** (consolidated under H3 family)
* Reproducer: `ddRemainder.decTest:316` (`ddrem424`, `remainder 1E+384 3E+383`). Also `ddrem425..ddrem430` and `ddrem422..ddrem423`.
* Symptom: debug panics at `bid.rs:216`; release packs garbage bits.
* Mechanism: lines 116 to 123 of `rem.rs` call `pack_finite(sign_a, (target_q + BIAS) as u32, residue)` without clamping. `target_q = exp_a.min(exp_b)`; for operand exponents above `+369`, the biased value exceeds `BIASED_EXP_MAX = 767`.
* Fix shape: route every rem return through a quantum clamping pack or the funnel; the typed `BiasedExp` newtype absorbs the call site.
* Provenance: `bid.rs:216`, `ddRemainder.decTest:316`, IEEE 754-2019 §5.4.2 + §7.4 + §3.5.2.

### M2: rem returns NaN INVALID when quotient still fits 16 digits
* Tier: **H**
* Reproducer: dividend `1E+25` (coef 1, exp 25), divisor `9999999999999999` (coef `10^16 - 1`, exp 0). Quotient is `~10^9`, well inside 16 digit precision, but `shift_a = 25 > MAX_SAFE_SHIFT = 22`. Property test seed: any `(coef_a × 10^exp_a) / coef_b` whose magnitude difference falls in `[23, ~600]` while quotient digit count stays under 16.
* Symptom: ferrodec returns `(NaN, INVALID)`; spec requires the truncated remainder result.
* Mechanism: lines 87 to 99 short circuit to NaN whenever `shift_a > MAX_SAFE_SHIFT`. The bound is necessary to keep `aligned_a` inside `u128`, but conflates "alignment overflow" with "spec quotient exceeds 16 digits." The spec test is the quotient digit count `digits(trunc(a/b)) > PRECISION`.
* Fix shape: gate the NaN on a digit count test of `floor(|a|/|b|)`.
* Provenance: `rem.rs:87..99`, IEEE 754-2019 §5.4.2.

### M3: rem path swallows the Clamped status flag
* Tier: **H** (folds into the workspace decision on Clamped)
* Reproducer: `ddRemainder.decTest:314` (`ddrem422`).
* Symptom: expected `Clamped`; ferrodec returns `Status::OK`.
* Mechanism: lines 73, 82, 91, 99, 116, 123 hand build results without consulting a quantum clamping helper.
* Fix shape: depends on workspace Clamped decision; see "Workspace decisions" below.
* Provenance: `rem.rs:73..123`.

### M4: mul and div forward unclamped preferred exponent into the rounding funnel
* Tier: **H** (consolidated under H3 family; affects `mul/div + round`)
* Reproducer: `ddDivide.decTest:285` (`dddiv497`, `divide 0E+380 1000E-13 -> 0E+369 Clamped`); symmetric mul case at `Decimal64::MAX × Decimal64::MAX`.
* Symptom: funnel mis packs or H3 mirror panic fires.
* Mechanism: `div.rs:68..72` forwards `q_preferred = exp_a - exp_b` (range `[-767, +767]`) directly; `mul.rs:46, 51` forwards `q_preferred = exp_a + exp_b` (range `[-796, +738]`). Both exceed `[BIASED_EXP_MIN, BIASED_EXP_MAX - BIAS]` at the extremes.
* Fix shape: clamp before passing to the funnel; the typed `BiasedExp` newtype absorbs.
* Provenance: `div.rs:68`, `mul.rs:46`, KNOWN_ISSUES H3 anchor.

### M5: mul and div mirror the H2 half even rounding bug at the 16 digit boundary
* Tier: **H** (cross listed with Agent 1 F2; affects `mul/div + round`)
* Reproducer: Slice D probe reports 1 fail in `ddMultiply.decTest` and 2 in `ddDivide.decTest`. Candidates need a probe rerun once Agent 1's H2 fix shape is in place. KNOWN_ISSUES H2 prose names 1 ddMultiply failure without a pinned case ID.
* Symptom: same as Agent 1 F2.
* Fix shape: subsumed by F2's fix in `addsub.rs`'s residue handling — but mul and div have separate effective subtract sites; verify Agent 2 will benefit from the same patch.
* Provenance: KNOWN_ISSUES H2.

### M6: Zero / Infinity div path loses Clamped flag
* Tier: **M** (workspace Clamped decision)
* Reproducer: `ddDivide.decTest:408` (`dddiv788`, `divide -1000 Inf -> -0E-398 Clamped`).
* Symptom: ferrodec returns `(-0E-398, OK)`; spec says `(-0E-398, Clamped)`.
* Fix shape: depends on Clamped decision.
* Provenance: `div.rs:154..160`, IEEE 754-2019 §7.4.

### M7: div zero coefficient path misses INEXACT under preferred quantum clamp
* Tier: **M**
* Reproducer: `ddDivide.decTest:285` (`dddiv497`).
* Symptom: depending on funnel behavior, ferrodec may either panic (H3 mirror) or return `0E+369` without Clamped.
* Fix shape: aligns with H3 family fix.
* Provenance: `div.rs:70..72`.

### M8: missing IEEE `remainder` (round half even) operation
* Tier: **M**
* Reproducer: `ddRemainderNear.decTest` exists in `tests/vectors/`; no method on `Decimal64` implements it.
* Mechanism: `rem.rs` defines only truncated remainder.
* Fix shape: add `Decimal64::remainder_near(self, other, rm) -> (Self, Status)`. Slice scope decision: this is new surface, possibly defer to a follow up.
* Provenance: `rem.rs`, `tests/vectors/ddRemainderNear.decTest`, IEEE 754-2019 §5.3.1.

### L1: `MAX_SAFE_SHIFT` magic constant lives in two places
* Tier: **L**
* Reproducer: `rem.rs:32` and `rem.rs:36`.
* Fix shape: hoist to a single `pub(crate) const`.
* Provenance: `rem.rs:29..36`.

### L2: doc claims `rem_trunc` exists; only `rem` exists
* Tier: **L**
* Reproducer: brief vs `rem.rs:41`.
* Fix shape: rename `Decimal64::rem` to `Decimal64::rem_trunc` (matching the truncated vs IEEE remainder distinction) or update the brief and KNOWN_ISSUES.
* Provenance: `rem.rs:41`.

### L3: `_ = rm` discards rounding mode in `rem`
* Tier: **L**
* Reproducer: `rem.rs:42`.
* Fix shape: drop the parameter or document why it is preserved.
* Provenance: `rem.rs:41..42`.

### L4: Infinity / Zero div spec status flag
* Tier: **L**
* Reproducer: search `ddDivide.decTest` for `Inf / 0`.
* Symptom: `div.rs:144..153` returns `Inf / 0 -> Inf` with `Status::OK`. Spec (IEEE §7.3) treats this as the same divisionByZero exception as `±finite / ±0`.
* Fix shape: add a `(Infinity, Zero)` arm above the generic `Infinity / *` arm.
* Provenance: `div.rs:144..153`, IEEE 754-2019 §7.3.

---

## Agent 3 — fma

12 findings. Per agent budget 1900 words.

### F1: H3 panic on `target_q + BIAS` cast to `u32` wrap when `target_q < -BIAS`
* Tier: **H** (the seed H3; consolidated under H3 family)
* Reproducer: `ddFMA.decTest:281` (case `ddfma2504`), operands `fma 0E-260 1000E-260 0E+384`. Spec answer `0E-398 Clamped`.
* Symptom: debug panics at `bid.rs:216`; release silently packs a wrapped biased_exp and returns garbage bits.
* Mechanism: `fma.rs:91` sums two operand exponents in `[-398, +369]`, so `ab_exp ∈ [-796, +738]`. The zero product and cancellation branches feed `target_q = min(ab_exp, c_exp)` to `pack_finite` via `(target_q + BIAS as i32) as u32`. When `target_q < -BIAS = -398`, the i32 result is negative and the cast wraps. IEEE 754-2019 §6.3 requires clamping the ideal exponent to `[-398, +369]` before encoding.
* Fix shape: typed `BiasedExp` newtype per the slice plan. Saturating clamp fallback if (1) does not fit.
* Provenance: KNOWN_ISSUES H3, IEEE 754-2019 §6.3, slice plan "H3 fix shape" subsection.

### F2: H3 mirror in cancellation zero branch
* Tier: **H** (consolidated under H3 family)
* Reproducer: `fma 1E-200 1E-200 -1E-400` (constructed; not in vectors directly). Property test seed.
* Mechanism: `fma.rs:163..173` reuses the same `(q_preferred + BIAS as i32) as u32` cast on the cancellation result.
* Fix shape: same `BiasedExp` newtype.
* Provenance: traced from `fma.rs` callers; spec §6.3.

### F3: early return on alignment overflow drops IEEE §6.3 preferred quantum
* Tier: **H**
* Reproducer: `ddFMA.decTest:113` (`fma0306`, `fma 1e-398 0.1 1 -> 1.000000000000000 Inexact Rounded`).
* Symptom: spec wants the 16 digit cohort with full trailing zero padding (coef `10^15`, q `-15`). The early return at `fma.rs:147..155` discards `target_q` and passes the dominant side's own exponent as `q_preferred`, starving the pad branch in `round.rs:109..116`. Result returns the canonical short cohort (coef 1, q 0, prints `"1"`).
* Mechanism: IEEE 754-2019 §6.3 sets the preferred exponent for an inexact additive result to `min(q(ab), q(c))`. The early return calls need `target_q` as the third argument, not the dominant side's `unbiased_exp`.
* Fix shape: change `fma.rs:140` and `fma.rs:147` to pass `target_q` as `q_preferred`.
* Provenance: IEEE 754-2019 §6.3.

### F4: early return conflates same sign and opposite sign sub ULP (Decimal128 H5 analogue)
* Tier: **H** (cross listed with Agent 1 F2)
* Reproducer: `ddFMA.decTest:1321` (`ddfma371100`, `fma 1 1e+2 -1e-383 -> 99.99999999999999 Rounded Inexact`). Twenty cases `ddfma371100..371119` per the H2 mirror in FMA.
* Symptom: spec wants `99.99999999999999` (16 nines, one ULP below 100) under `NearestEven`. Early return treats sub ULP c as a positive sticky regardless of sign, routing to 100.
* Mechanism: when `shift_ab > ab_safe_shift`, `fma.rs:138..141` builds `pre_sticky = coef_c != 0` and dispatches without tracking `effective_sub = ab_sign != sign_c`. Same defect at `fma.rs:143..155` for the c dominant branch.
* Fix shape: split each early return on `effective_sub`. Opposite sign needs lower / upper candidate selection mirroring Decimal128 commit `105fe40`'s behavior (not code shape).
* Provenance: IEEE 754-2019 §5.4.1 + §4.3.

### F5: `ddfma2901` missing UNDERFLOW flag on subnormal product
* Tier: **M**
* Reproducer: `ddFMA.decTest:554` (`ddfma2901`, `fma 0.3000000001E-191 0.3000000001E-191 0e+384 -> 9.00000000600000E-384 Underflow Inexact Subnormal Rounded`).
* Symptom: result value correct; status flag is `INEXACT` (16) only, spec wants `INEXACT | UNDERFLOW` (24).
* Mechanism: `finalise_finite` in `round.rs:215..232` raises UNDERFLOW only when `biased < 0`. A subnormal product whose quantum is exactly at `E_MIN` has `biased = 15 ≥ 0` and the underflow branch never fires.
* Fix shape: detect subnormality from the adjusted exponent of the unrounded product: `unbiased_exp + (digits − 1) < E_MIN`. Affects `fma + round`.
* Provenance: IEEE 754-2019 §7.5.

### F6: early return discards `ab_exp` / `c_exp` distinction for inexact path
* Tier: **M** (cohort half of F3)
* Reproducer: same family as F3.
* Fix shape: subsumed by F3.
* Provenance: IEEE 754-2019 §6.3.

### F7: `Class::Zero` quantum participates in product exponent without explicit zero quantum convention
* Tier: **M** (folds into H3)
* Reproducer: `ddfma2504` itself.
* Symptom: `fma 0E-260 1000E-260` lands at `ab_exp = -520` directly, amplifying the F1 panic surface.
* Fix shape: H3 family fix absorbs.
* Provenance: spec re derivation.

### F8: `debug_assert!` security audit on zero coefficient `as u32` cast
* Tier: **H** (security family; consolidated under H3)
* Provenance: global CLAUDE.md security posture; slice plan security audit subsection.

### F9: `pack_finite` precondition shape applies to addsub, mul, div, quantize too
* Tier: **L** (coordination)
* Symptom: every caller computing `biased_exp` from input derived arithmetic shares F1's wrap risk.
* Fix shape: `BiasedExp` newtype is the cross cutting fix.
* Provenance: `bid.rs:214..216`.

### F10: `handle_specials` order matches IEEE §6.2.3 for sNaN to qNaN propagation (positive finding)
* Tier: **L** (audit clean)
* Symptom: not a bug; flagging that decimal64's `fma.rs:218..243` matches Decimal128's post M5 shape (`4b256ce`).
* Provenance: IEEE 754-2019 §6.2.3.

### F11: `decimal_digit_count_u128(0)` behavior used at `fma.rs:124..125`
* Tier: **L**
* Symptom: not user reachable in current control flow.
* Fix shape: none required.
* Provenance: source inspection.

### F12: FMA side observation of H2
* Tier: **H** (deduplicated with Agent 1 F2)
* Reproducer: `ddFMA.decTest:1321..1340` (20 cases). Cross listed with F4 since the early return effective subtract split and the `round.rs` tie break logic share the failing test set.
* Fix shape: deferred to Agent 1's H2 fix.

---

## Agent 4 — sqrt + quantum

5 findings. Per agent budget 1100 words.

### Q1: `quantize(0, target with quantum below 1e-16)` returns NaN INVALID
* Tier: **H**
* Reproducer: `ddqua537 quantize 0 1e-299 -> 0E-299` (also synthesisable for any `pad > 16` against zero).
* Symptom: spec mandates the result is zero requantised to the target quantum; the code returns quiet NaN with INVALID.
* Mechanism: `quantize` at `quantum.rs:201..212` enters the pad branch with `coef == 0`, sets `new_digits = pad`, then rejects `new_digits > PRECISION` at line 208. For zero, the coefficient is zero at any padding amount, so the operation is always representable as long as `target_q` lies in the format's exponent range — and it does, because `target_q` was decoded from a `Decimal64`. Per GDA `quantize` and IEEE 754-2019 §5.3.2, validity depends on whether the result fits in the format's exponent envelope, not on the padded "digit count" of a zero coefficient.
* Fix shape: special case `coef == 0` before the digit count gate; emit `pack_finite(sign, target_biased, 0)` directly.
* Provenance: ddqua537 in `tests/vectors/ddQuantize.decTest`; IEEE 754-2019 §5.3.2.

### Q2: scaleb exponent arithmetic wraps in release on `i32::MIN` / `i32::MAX`
* Tier: **M**
* Reproducer: `Decimal64::ONE.scaleb(i32::MAX, RoundingMode::NearestEven)`.
* Symptom: release `q = biased_exp - BIAS + n` wraps two's complement and feeds the funnel a corrupt unbiased exponent; debug may panic. Spec wants OVERFLOW or UNDERFLOW with ±∞ or ±0, plus INVALID for `|n| > 2 × (Emax + precision)` ≈ 800.
* Mechanism: `quantum.rs:248` and `:256` compute `q = biased_exp as i32 - BIAS as i32 + n`. The addition overflows `i32` for `n` near `i32::MAX` or `i32::MIN`.
* Fix shape: lift the exponent arithmetic into `i64`, clamp to a sentinel that drives the funnel into the overflow / underflow path, or raise INVALID for `|n|` outside GDA's envelope.
* Provenance: IEEE 754-2019 §5.3.3; GDA `scaleb` constraint table.

### Q3: sqrt routing uses `saturating_sub` for a statically non negative difference
* Tier: **L**
* Reproducer: static.
* Symptom: `scale = target_d.saturating_sub(d)` at `sqrt.rs:110` cannot underflow because `d ≤ 17` and `target_d ∈ {33, 34}`.
* Fix shape: prefer `target_d - d` with a `debug_assert!(d <= target_d)` above it.
* Provenance: `sqrt.rs:108..112`.

### Q4: `logb` finite branch carries a dead `unwrap_or` arm
* Tier: **L**
* Reproducer: static.
* Symptom: `Decimal64::try_new(i64::from(adj), 0).unwrap_or(...)` at `quantum.rs:293` and `:298` cannot fail.
* Fix shape: replace with `.expect("logb adjusted exponent fits decimal64")`.
* Provenance: `quantum.rs:288..301`.

### Q5: `next_up` / `next_down` signaling NaN path re decodes the bit pattern
* Tier: **L**
* Reproducer: static.
* Symptom: `if self.is_signaling_nan() { if let Class::SignalingNaN { ... } = classify_bits(self.0) { ... } }` at `quantum.rs:317..324` and `:406..413` decodes twice.
* Fix shape: pull a single `classify_bits` at the top and match its result.
* Provenance: `quantum.rs:316..324, 405..413`.

### Audit clean items (recorded so the next reviewer does not redo the work)

* `sqrt_special_cases::Zero` at `:80..87` **already mirrors the Decimal128 M2 fix** (`exp.div_euclid(2)`). Decimal64's range `[-398, 369]` folds to `[-199, 184]`, well inside the biased envelope; no clamp needed.
* `sqrt_positive_finite`'s parity asserts at lines 111, 112, 117 all hold by static parity arguments.
* `next_up`'s renormalisation pre step at `:355..358` is the right shape; the regression test at `:530..545` pins the behaviour.
* `quantize`'s drop digits loop at `:159..170` correctly identifies the round digit and accumulates sticky.

---

## Agent 5 — parse + Display + conversions

Threat model plus 11 findings. Per agent budget 2400 words.

### Threat model

`Decimal64::parse_str` (and its `FromStr` delegate) is the only attacker controlled surface in this domain. Callers downstream of `str::parse::<Decimal64>()` may feed it bytes from any source: file content, user keystrokes, JSON / SMIL inputs to the ferrodec calculator core.

* **Worst outcomes**: (i) a debug mode panic on a malformed or oversized literal (DoS / crash); (ii) a release mode silent miscompute where overflow wraps the unbiased exponent and yields a numerically wrong `Decimal64` with no `INVALID` flag; (iii) an unbounded loop scaling linearly with input length (no quadratic blowup observed).
* **Resource bounds**: all accumulators are fixed width (`u64` coefficient, `u32` digit counters, `i32` exponent); allocation is zero. The risk is integer overflow inside the counters, not memory exhaustion.
* **Doc gap**: `parse_str`'s doc comment (parse.rs:69 to 73) does not state the threat model.

`from_f64` (`convert/binary.rs:73`) is not attacker controlled in the usual sense, but a hand crafted bit pattern that decodes as `f64::NAN` (signaling at the bit level) is silently mapped to quiet `Decimal64::NAN` with `Status::OK`, dropping the IEEE 754 §5.4.2 INVALID signal.

### B1: `to_f64` does not raise INVALID on signaling NaN
* Tier: **H**
* Reproducer: `let s = Decimal64::SIGNALING_NAN; s.to_f64()` returns `f64::NAN` silently. Decimal128 commit `67bd45c` pins the analogous case.
* Symptom: IEEE 754-2019 §5.4.2 `convertFormat` requires every sNaN operand to raise INVALID and yield a quiet NaN; the current impl swallows the signal.
* Mechanism: `convert/binary.rs:32` matches `QuietNaN | SignalingNaN` together and returns `f64::NAN`. The method's signature is `pub fn to_f64(self) -> f64`; there is no `Status` channel.
* Fix shape: change the signature to return `(f64, Status)` and emit `Status::INVALID` on the `SignalingNaN` arm; mirror commit `67bd45c`. **Breaking API change**; release notes for 1.4.0.
* Provenance: IEEE 754-2019 §5.4.2; ferrodec commit `67bd45c`.

### B2: `parse_str` panics in debug on adversarial leading fractional zeros
* Tier: **H**
* Reproducer: a `&str` shaped `"0." + "0".repeat(u32::MAX as usize) + "1"`. In debug, `digits_after_point: u32` overflows.
* Symptom: panic with `attempt to add with overflow` inside the digit loop. Tier H per ADR-0010 ("a panic in any documented execution path"). STM32U targets will not see 4GB strings, but `parse_str` is also called from desktop calculator front ends.
* Mechanism: leading fractional zeros increment `digits_after_point` without bumping `digits_total` (parse.rs:133..139). The increment is `digits_after_point += 1`, not `saturating_add`.
* Fix shape: `digits_after_point = digits_after_point.saturating_add(1)` at parse.rs:139 and parse.rs:144; clamp the cast at parse.rs:234 too (saturate to `i32::MAX`).
* Provenance: parse.rs:115, 139, 144, 234; threat model audit.

### B3: `<Decimal64 as ToPrimitive>::to_f32` double rounds through `f64`
* Tier: **M**
* Reproducer: any decimal sitting on a half ULP of f32 but off boundary in f64. Decimal128 commit `c9e53f5` cites `8589973000` as the canonical case.
* Mechanism: `num_traits_impls.rs:197` evaluates `Some(Decimal64::to_f64(*self) as f32)`. Same `(float)(double)x` hazard Decimal128 had.
* Fix shape: format via Display into a 32 byte stack buffer and parse as `f32`. Mirror commit `c9e53f5`'s structure (re derived).
* Provenance: `num_traits_impls.rs:197`; ferrodec commit `c9e53f5`; IEEE 754-2019 §5.4.2.

### B4: `<Decimal64 as ToPrimitive>::to_i64` / `to_u64` lose precision via f64 intermediate
* Tier: **M**
* Reproducer: `Decimal64::try_new(9_223_372_036_854_775_806, 0).unwrap().to_i64()` returns `Some(i64::MAX)`.
* Mechanism: `num_traits_impls.rs:163..191` converts through `to_f64`. A 16 digit coefficient close to `2^63` rounds to the nearest f64 (which equals `2^63` exactly), then `as i64` saturates.
* Fix shape: extract `(coefficient, exponent)` from `classify_bits` directly; do decimal rounding to the integer quantum; then range check the exact `u64` / `i64`.
* Provenance: `num_traits_impls.rs:163..200`; ferrodec ADR-0010 M4 anchor.

### B5: `parse_str` doc comment omits the threat model
* Tier: **M**
* Reproducer: read parse.rs:69..73.
* Mechanism: doc gap. The user's CLAUDE.md security posture ("the answer belongs in the doc comment") names this.
* Fix shape: add a `## Threat model` block to the `parse_str` doc comment.
* Provenance: parse.rs:69..73; global CLAUDE.md security posture.

### B6: `from_f64` does not distinguish signaling f64 NaN bit patterns
* Tier: **L**
* Reproducer: `f64::from_bits(0x7FF0_0000_0000_0001)` is sNaN at the bit level.
* Mechanism: `convert/binary.rs:74` calls `x.is_nan()` which is true for both quiet and signaling NaNs. Both collapse to `Decimal64::NAN` with `Status::OK`.
* Fix shape: inspect `x.to_bits()` against the IEEE 754 binary64 signaling mask.
* Provenance: binary.rs:74; IEEE 754-2019 §5.4.2.

### B7: `extra_int_digits as i32` reinterprets on adversarial input
* Tier: **L**
* Reproducer: a `&str` shaped `"1" + "0".repeat(3_000_000_000)`.
* Symptom: `unbiased_exp` becomes negative instead of saturating positive.
* Fix shape: clamp `extra_int_digits` and `digits_after_point` at MAX_EXPONENT_MAGNITUDE inside the digit loop.
* Provenance: parse.rs:115, 116, 233, 234.

### B8: zero in engineering notation pads with leading zeros rather than mantissa zeros
* Tier: **L**
* Reproducer: `Decimal64::try_new(0, -7).unwrap().engineering()` produces `"000E-9"`.
* Fix shape: special case zero in `write_engineering`; emit `"0"` and the rebased exponent without padding.
* Provenance: format.rs:242..281; GDAS §3.1.

### B9: `parse_str` `InvalidCharacter` error diagnostic is indistinct
* Tier: **L**
* Symptom: callers cannot tell from the error whether the input had a malformed NaN token or a generic character mid mantissa.
* Fix shape: deferred to v2.0 (breaking enum addition).
* Provenance: parse.rs:289..315.

### B10: `Decimal64BuildError::ExponentOutOfRange` message is opaque
* Tier: **L**
* Symptom: error text says `"exponent outside [-398, 369]"` without context.
* Fix shape: rephrase to `"unbiased exponent outside [-398, 369] (quantum range for 16 digit Decimal64)"`.
* Provenance: decimal.rs:46, 141.

### B11: `Debug` for `Decimal64` exposes internal `Class` enum
* Tier: **L**
* Symptom: `format!("{:?}", Decimal64::ONE)` leaks `bid::Class` variants into the public `Debug` contract.
* Fix shape: write a manual Debug formatter that prints stable identifiers.
* Provenance: decimal.rs:148.

### Audit clean items

* `classify_bits` Form A canonicalisation already correct by encoding: `coef_high3 << 50 | T_MASK ≤ 7 × 2^50 + 2^50 − 1 < 2^53 < 10^16 = COEFFICIENT_LIMIT`. The Decimal128 H3 / H4 concern cannot occur for Decimal64. One line comment at `bid.rs:187` would record the bound by construction.
* `pack_finite` preconditions are sound on every internal call site within Agent 5's domain (`decimal.rs::try_new_unsigned_with_sign` validates both; constants in `decimal.rs:78..111` are in range; `round.rs` clamps).

---

## Agent 6 — transcendentals + Kani

11 findings. Per agent budget 2000 words.

### T1: exp UNDERFLOW flag not raised on denormal f64 outputs
* Tier: **M**
* Reproducer: `Decimal64::try_new(-720, 0).unwrap().exp(RoundingMode::NearestEven)`.
* Symptom: `UNDERFLOW` is set only when `libm::exp` saturates to `0.0`; denormal f64 outputs in `(0, ~2.2e-308]` whose Decimal64 reading lands in the subnormal range return `INEXACT` only.
* Mechanism: §7.5 / GDA underflow rule fires whenever the rounded result's adjusted exponent is below `E_MIN = -383`. Current code at `exp.rs:64..65` triggers UNDERFLOW only on saturation. Mirrors Decimal128 ADR-0010 M7.
* Fix shape: after `from_f64`, raise UNDERFLOW when adjusted exponent < `E_MIN`.
* Provenance: ADR-0010 M7; IEEE 754-2019 §7.5; `exp.rs:58..74`.
* **Resolution (2026-05-15, commit pending): not reproducible, no behavior change.** The symptom conflates an f64 denormal output with a Decimal64 subnormal result. Decimal64's range reaches `1E-398`, far below f64's smallest non-zero (`~5E-324`), so every non-zero `libm::exp` output maps to a *normal* Decimal64. An empirical probe over `x ∈ [-700, -2000]` found no input that yields a subnormal Decimal64: `libm::exp` saturates to exactly `0.0` near `x = -745`, well before the Decimal64 subnormal window near `x = -881`, and that `0.0` is already covered by the existing `r == 0.0 → UNDERFLOW` branch. The proposed `is_subnormal` guard would be unreachable on the f64 path (untestable dead branch) and is therefore not landed; it would become relevant only under the deferred pure-decimal kernel, whose introduction is the right place to add it. A regression test (`exp_underflow_contract_m7`) pins both ends of the real contract and guards against an over-eager future fix that would flag the normal mid-range result. ADR-0018 records this outcome.

### T2: exp / sinh / cosh saturation thresholds asymmetric with Decimal64 range
* Tier: **M** (with documented limitation note)
* Reproducer: `from_int(800, 0).exp(...)` returns `+∞ + OVERFLOW`; Decimal64 can represent `exp(800) ≈ 5.5e347` finitely.
* Mechanism: the f64 routing caps at `|x| ≈ 710`. Decimal64's `E_MAX = 384` admits up to `x ≈ 885`. Inputs in `[710, 885]` lose finite results.
* Fix shape: documented limitation surface today; the v1.1+ transcendentals rewrite (1.16 era per ferrodec 1.15.0 CHANGELOG `Deferred`) replaces this. For 1.4.0, narrow the documented envelope.
* Provenance: `exp.rs:18..27`; `hyper.rs:11..21`; `decimal.rs` `E_MAX`.

### T3: trig argument reduction inherits f64 precision loss above `2^53`
* Tier: **M** (with documented limitation note)
* Reproducer: `Decimal64::try_new(9_999_999_999_999_999_i64, 0).unwrap().sin(...)`.
* Mechanism: `f64_unary` at `f64_bridge.rs:29..31` calls `to_f64` then libm; libm's argument reduction is correct only to f64 precision. Decimal128 retains a margined `argred` module (ADR-0010 M8); decimal64 has no analogue.
* Fix shape: for 1.4.0, narrow the documented ULP envelope to `|x| < 2^53`. Decimal aware reduction lands in the transcendentals rewrite.
* Provenance: ADR-0010 M8; `trig.rs:50, 68, 93, 182`.

### T4: `equals_one` cohort walk magic literal
* Tier: **L**
* Reproducer: static.
* Symptom: `pow.rs:30..32` uses literal `15` for the cohort cap; not derived from `PRECISION = 16`.
* Fix shape: replace `15` with `PRECISION - 1`.
* Provenance: `pow.rs:11..36`; `bid::PRECISION`.

### T5: `debug_assert!(new_coef < COEFFICIENT_LIMIT)` in `quantize` pad branch
* Tier: **H** (security family; consolidated under H3)
* Reproducer: `Decimal64::try_new(9_999_999_999_999_999_i64, 0).unwrap().quantize(Decimal64::try_new(1, -15).unwrap(), RoundingMode::NearestEven)`.
* Mechanism: control flow before the assert constrains the bound today, but the invariant lives in two scattered checks rather than the type system.
* Fix shape: convert to `assert!` or lift into a `Coefficient` newtype paralleling `BiasedExp`.
* Provenance: global CLAUDE.md security posture; `quantum.rs:213..218`.

### T6: missing `exp` / `ln` Kani shims
* Tier: **M**
* Reproducer: `cargo kani --package ferrodec-decimal64 --features=fmt`; no harness exists.
* Symptom: §9.2 special case dispatch for `exp` and `ln` (NaN propagation, sNaN INVALID, `exp(±∞)`, `exp(±0) = 1`, `ln(±0) = -∞ + DIV_BY_ZERO`, `ln(neg) = NaN + INVALID`) is unverified.
* Fix shape: add `*_special_only_for_kani` shims and harness files.
* Provenance: ADR-0016; `exp.rs:46..58, 82..94`.

### T7: missing trig and inverse trig Kani shims
* Tier: **M**
* Symptom: §9.2 dispatch for `sin / cos / tan / asin / acos / atan / atan2` is unverified.
* Fix shape: per op shim; `atan2` shim takes two operands and pins the §6.2.3 dual sNaN ordering.
* Provenance: ADR-0016; `trig.rs`.

### T8: missing hyperbolic Kani shims
* Tier: **M**
* Symptom: §9.2 dispatch for `sinh / cosh / tanh / asinh / acosh / atanh` is unverified.
* Fix shape: per op shim.
* Provenance: ADR-0016; `hyper.rs`.

### T9: missing `pow` and `cbrt` Kani shims (cohort walk regression site)
* Tier: **M**
* Mechanism: pow's `equals_one` cohort walk has a unit test (`pow_non_canonical_one_cohort_short_circuits`) but no symbolic guard.
* Fix shape: shim returning `Some` on every dispatched arm; bounded operand harness extending `operand(idx)` with a 16 element cohort selector.
* Provenance: ADR-0016; `pow.rs:52..134`.

### T10: missing `quantize` / `scaleb` / `logb` / `next_up` / `next_down` Kani shims
* Tier: **M**
* Mechanism: §5.3 rule tables for these five ops are unverified.
* Fix shape: per op shim; `next_up_special_only_for_kani` highest value (two recent regression unit tests).
* Provenance: ADR-0016; `quantum.rs`.

### T11: no astro-float oracle property tests for transcendentals
* Tier: **M**
* Mechanism: `ls ferrodec-decimal64/tests/` returns only `conformance.rs` and `vectors/`; no `tests/property_*.rs`.
* Fix shape: add `astro-float` as a dev dep and seed property tests at `tests/property_transcendentals.rs` per op, asserting result within a stated ULP envelope.
* Provenance: memory record `feedback_oracle_choice`; `Cargo.toml` dev dep block.

---

## Workspace decisions

Two decisions need to be made at slice scope before Phase 2 starts.

### Decision 1: IEEE Clamped informational flag

Multiple findings (Agent 1 F7, Agent 2 M3, Agent 2 M6) cite a missing IEEE 754 `Clamped` flag emission. `ferrodec_ieee::Status` carries only the five IEEE mandatory flags (`INVALID`, `DIV_BY_ZERO`, `OVERFLOW`, `UNDERFLOW`, `INEXACT`). Adding `Clamped` is a workspace level change (touches `ferrodec-ieee`, ripples through every dependent crate).

Options:

- **(a)** Extend `ferrodec_ieee::Status` with a `CLAMPED` flag plus a status bit, emit at clamp sites. Closes the conformance gap on Clamped marked decTest cases.
- **(b)** Accept the omission; document in `ferrodec-ieee` that only IEEE mandatory flags are modeled; skip Clamped marked cases in the conformance harness.

Recommendation: **(a)** for the slice, since multiple H tier findings depend on the flag. Add the flag in a pre Phase 2 commit on `ferrodec-correctness` branch.

### Decision 2: IEEE `remainder` (round half even) operation

Agent 2 M8 names the missing IEEE 754 §5.3.1 `remainder` operation (distinct from the truncated `rem`). `tests/vectors/ddRemainderNear.decTest` exists in the vendored corpus.

Options:

- **(a)** Add `Decimal64::remainder_near` in this slice. New surface; pushes 1.4.0 toward 1.5.0 minor bump.
- **(b)** Defer to a follow up slice. 1.4.0 closes the correctness train without surface growth.

Recommendation: **(b)** — defer. The slice's purpose is closing the H tier correctness train; the new method is additive and can ship on its own cadence.

---

## Phase 2 work order (consolidated, dedup applied)

H tier findings drive Phase 2..N. M and L tier follow once H is closed.

### H tier (Phase 2)

Ordered by dependency. Each line names the consolidated fix item, reproducer, owning files, and a short fix shape.

1. **H3 family — `pack_finite` precondition / typed encoding**
   * Reproducers: `ddfma2504` (pinned), `ddrem424`, `dddiv497` (probable), property test candidates in mul and quantize.
   * Files: `bid.rs` (newtypes), `fma.rs` (zero product + cancellation), `rem.rs:116..123`, `mul.rs:46`, `div.rs:68..72`, `quantum.rs:218`, `addsub.rs:119, 169`, `round.rs:73, 175, 195, 210, 226, 229, 236`.
   * Fix shape: introduce `BiasedExp` newtype in `bid.rs` whose constructor proves `0 ≤ value ≤ 767`; introduce `Coefficient` newtype proving `< COEFFICIENT_LIMIT`. Route all `pack_finite` callers through the constructors. Remove `debug_assert!` on input derived arithmetic across the slice.
   * Tier consolidation: absorbs Agent 1 F4, Agent 3 F1 + F2 + F7 + F8 + F9, Agent 4 Q1 (consequence of quantize path), Agent 6 T5, Agent 2 M1 + M4.

2. **H1 — finite finite addition magnitude loss when one operand is zero**
   * Reproducer: `ddadd360`.
   * Files: `addsub.rs:115, 128..154, 161..175`.
   * Fix shape: short circuit at top of `add_inner` for asymmetric zero operand; route through `round_and_pack_finite` with correct `q_preferred`.
   * Tier consolidation: Agent 1 F1 + F3.

3. **H2 — effective subtract sub ULP residue mis attribution**
   * Reproducers: `ddadd71100..71119` plus mirror `71200..71219`; `ddfma371100..371119`.
   * Files: `addsub.rs:158..160` (root); `fma.rs:140, 147` (mirror in early return).
   * Fix shape: borrow one ULP from `combined_coef` and re extend the low digits; split FMA's early return on `effective_sub`.
   * Tier consolidation: Agent 1 F2, Agent 3 F4 + F12, Agent 2 M5.

4. **H4 — FMA early return drops IEEE §6.3 preferred quantum**
   * Reproducer: `fma0306`.
   * Files: `fma.rs:140, 147`.
   * Fix shape: thread `target_q` as `q_preferred` (third argument) instead of dominant side's `unbiased_exp`.
   * Tier consolidation: Agent 3 F3 + F6.

5. **H5 — rem `Division_impossible` predicate uses exponent gap, spec says quotient digit count**
   * Reproducer: property test seed (no pinned decTest yet).
   * Files: `rem.rs:87..99`.
   * Fix shape: replace `shift_a > MAX_SAFE_SHIFT` test with a digit count test of `floor(|a|/|b|)`.
   * Tier consolidation: Agent 2 M2.

6. **H6 — quantize on zero with deep target quantum returns NaN INVALID**
   * Reproducer: `ddqua537`.
   * Files: `quantum.rs:201..218`.
   * Fix shape: special case `coef == 0` before the digit count gate.
   * Tier consolidation: Agent 4 Q1.

7. **H7 — `to_f64` does not raise INVALID on signaling NaN (breaking API)**
   * Reproducer: `Decimal64::SIGNALING_NAN.to_f64()`.
   * Files: `convert/binary.rs:32`, signature change.
   * Fix shape: change signature to `(f64, Status)`; mirror Decimal128 commit `67bd45c`. 1.4.0 release note.
   * Tier consolidation: Agent 5 B1.

8. **H8 — `parse_str` debug panic on adversarial leading fractional zeros**
   * Reproducer: long zero pad string seed.
   * Files: parse.rs:115, 139, 144, 234.
   * Fix shape: `saturating_add` on the digit counter; clamp the `i32` cast at parse.rs:234.
   * Tier consolidation: Agent 5 B2.

9. **(Conditional, depends on Decision 1) — emit `Clamped` flag at clamp sites**
   * Reproducers: many `ddAdd`, `ddDivide`, `ddRemainder`, `ddFMA` cases with `Clamped` condition.
   * Files: ferrodec-ieee extension + every clamp site in this crate.
   * Fix shape: extend `Status`; emit at `finalise_finite`, `rem.rs`, `div.rs`'s `(Inf, finite)` paths.
   * Tier consolidation: Agent 1 F7, Agent 2 M3 + M6.

### M tier (Phase 3, after H closure)

* M1: `ddfma2901` missing UNDERFLOW (Agent 3 F5; root in `round.rs::finalise_finite`).
* M2: scaleb i32 exponent overflow on extreme n (Agent 4 Q2).
* M3: `from_f64` does not catch f64 bit level sNaN (Agent 5 B6; folds into H7 if signature changes).
* M4: `to_f32` double rounding via f64 (Agent 5 B3).
* M5: `to_i64` / `to_u64` precision loss via f64 (Agent 5 B4).
* M6: `parse_str` doc threat model gap (Agent 5 B5).
* M7: exp UNDERFLOW missing on f64 denormal outputs (Agent 6 T1).
* M8: exp / sinh / cosh saturation threshold asymmetry — accept as documented limitation surface in 1.4.0; rewrite lands later (Agent 6 T2).
* M9: trig argument reduction f64 precision loss above 2^53 — narrow documented envelope in 1.4.0; rewrite lands later (Agent 6 T3).
* M10: missing Kani shims for `exp` + `ln` (Agent 6 T6).
* M11: missing Kani shims for trig + inverse trig (Agent 6 T7).
* M12: missing Kani shims for hyperbolic (Agent 6 T8).
* M13: missing Kani shims for `pow` + `cbrt` (Agent 6 T9).
* M14: missing Kani shims for quantum family (Agent 6 T10).
* M15: no astro-float property tests for transcendentals (Agent 6 T11).

### L tier (Phase 4 or deferred)

* L1: addsub loop invariant debug_asserts dead (Agent 1 F5).
* L2: `Decimal64::sub` doc gap on NaN propagation (Agent 1 F6).
* L3: `kept_digits` invariant not maintained explicitly (Agent 1 F8).
* L4: cohort lowering exit condition undocumented (Agent 1 F9).
* L5: `rem.rs` `MAX_SAFE_SHIFT` duplicated (Agent 2 L1).
* L6: `rem_trunc` named in slice plan but only `rem` exists (Agent 2 L2).
* L7: `_ = rm` unused in `rem` (Agent 2 L3).
* L8: `Inf / 0` spec status flag (Agent 2 L4).
* L9: sqrt `saturating_sub` redundant (Agent 4 Q3).
* L10: `logb` `unwrap_or` dead arm (Agent 4 Q4).
* L11: `next_up` / `next_down` double decode (Agent 4 Q5).
* L12: `extra_int_digits` cast reinterprets (Agent 5 B7).
* L13: zero engineering notation padding (Agent 5 B8).
* L14: `parse_str` error diagnostic indistinct (Agent 5 B9 — defer to v2.0).
* L15: `ExponentOutOfRange` message opaque (Agent 5 B10).
* L16: `Debug` exposes internal `Class` (Agent 5 B11).
* L17: `equals_one` magic literal `15` (Agent 6 T4).

---

## Provenance summary

All findings derive from:

- IEEE 754-2019 spec text (re derived; no implementation lifted).
- General Decimal Arithmetic spec at speleotrove.com (vendored README references it).
- ferrodec's own source code (this repo).
- Decimal128 fix commits in this repo, used as oracle for **behavior**, not code shape: `67bd45c`, `7b7a0fd`, `96f6d3d`, `f0b6a16`, `105fe40`, `4b256ce`, `28f1f4c`, `7183717`, `9e9911d`, `c9e53f5`, `5780569`.
- ADR-0010 MEDIUM findings catalog (used to cross check decimal128 to decimal64 propagation).

**No code shape was consulted or lifted from**: Intel's Decimal Floating Point Math Library, IBM's decNumber, the `astro-float` crate's transcendental implementations, libm's CORDIC / polynomial expansions, rustc's `f64::parse` / `f64::to_string` paths.

If a future reader spots a finding that smells like recall rather than derivation, that is a defect to flag; the discipline was named explicitly in each agent brief.
