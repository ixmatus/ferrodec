# decimal32 correctness slice: six agent review findings

> Phase 1 deliverable. Authoritative work order for Phase 2..N. No
> source changed in Phase 1. Mirrors the decimal64 slice
> (`2026-05-11-decimal64-correctness-findings.md`, ADR-0018).

## Method

Six concurrent general purpose agents swept the decimal32 op surface
under the ADR-0010 severity rubric, the provenance discipline (no
Intel Decimal Library or IBM decNumber recall; oracles are IEEE
754-2019 spec text and Decimal64/128 output behavior), and the
security audit directive (`debug_assert!` on input derived
arithmetic, parser threat model, transcendental bound independence).
Allocation: A1 addsub+round, A2 mul/div/rem, A3 fma, A4 sqrt+quantum,
A5 parse/Display/conversions, A6 Kani parity + f64 bridge.

Phase 0 already pinned three reproducers through the Decimal64
cross-check oracle (`tests/d64_crosscheck.rs`, KNOWN_ISSUES.md). The
planning era hypothesis that decimal32's narrower format might make
the static alignment windows sufficient is refuted for addsub; it is
nuanced for rem (see H2 and the oracle soundness note).

## Oracle soundness note (read before touching rem)

> Resolved 2026-05-15 by H2: the rem arm of the oracle now keys on
> the true integer quotient digit count (`rem_oracle_check`), so it
> no longer false positives when that count is 8 to 16. The note
> below is retained as the rationale.

The Decimal64 cross-check oracle is exact for `mul` (the product of
two 7 digit coefficients fits Decimal64's 16) and for the in range
value of `add` / `sub` away from the double rounding boundary. It is
**unsound for `rem` when the true integer quotient has more than 7
but at most 16 digits**: Decimal64's `Division_impossible` predicate
keys on its own 10^16 coefficient limit, so Decimal64 returns a
finite remainder where Decimal32 must, per GDA, raise
`Invalid_operation`. The pinned reproducer
`rem(4.194304E+33, -3.145728E+18)` has an integer quotient near 16
digits, so Decimal32's `NaN` may be spec correct and the oracle's
finite expectation a false positive. H2 must first correct the rem
arm of the oracle, then establish a sound reproducer whose integer
quotient is at most 7 digits before claiming a defect. The static
window concern is still real in principle (it conflates a u64
overflow guard with the quotient digit count); it just needs a sound
witness.

## H tier

The fix order is dependency first. The typed newtype work (M1) lands
before H3/H4 because it is their structural mechanism.

### H1 — addsub static `ALIGN_LIMIT` window drops the residue and the asymmetric zero magnitude

* **Agents**: A1-F1, A1-F2, A1-F3 (one root, two branches, one
  generalization).
* **Tier**: H. Wrong value, no signal, confirmed by the cross-check
  and pinned in KNOWN_ISSUES (slice H1, H2).
* **Site**: `ferrodec-decimal32/src/ops/addsub.rs:192-206` (the
  truncate branch, `coef_lo / 10^trim` losing a low operand with no
  more digits than `trim`) and `:207-210` (the `diff > WORKING_PRECISION`
  drop branch, taken even when `coef_hi == 0`), interacting with the
  exact cancellation `pre_sticky` recovery at `:235-248`.
* **Reproducers**: `add(-1E-101, 1E-88)` TowardZero gives
  `1.000000E-88`, spec `9.999999E-89`; `sub(-0E-74, -3.145728E-95)`
  NearestEven gives `1E-101`, spec `3.145728E-95`. Property
  neighborhood: `13 <= diff <= 25` and
  `digit_count(coef_lo) <= diff - 12`.
* **Cause**: the static window assumes a low operand past the fixed
  exponent gap can only contribute below the working precision, true
  only when the operand has more digits than the truncation amount.
  A one digit low operand collapses entirely into a single sticky
  bit, so an effective subtract borrow that should cross the kept
  digit boundary never happens (IEEE 754-2019 §5.4.1 requires the
  correctly rounded exact sum; a sticky bit cannot represent a
  borrow). The drop branch additionally treats a zero `hi` as the
  dominant operand, discarding the real magnitude (§6.3 `x ± 0 = x`).
* **Fix shape**: one commit. Dynamic per side shift over a u128
  working register mirroring the in crate `fma.rs`
  (`decimal_digit_count_u128`, `U128_DIGIT_CAP`, per side
  `safe_shift`), plus an explicit zero coefficient fast path so a ±0
  operand never enters the dominant side branch. Retires A1-F1/F2/F3.
* **Guard**: un-ignore `add_matches_decimal64`,
  `sub_matches_decimal64`, `addsub_small_coef_large_gap_neighborhood`
  in `tests/d64_crosscheck.rs`.

### H2 — rem static `MAX_SAFE_SHIFT` raises spurious `INVALID` — CLOSED (defect confirmed with a sound witness; pinned case was an unsound oracle false positive)

* **Agents**: A2-F1, A2-F2.
* **Outcome (2026-05-15, evidence first)**: defect **confirmed** with
  a sound witness; the Phase 0 pinned reproducer was a false
  positive of the unsound `rem` oracle, now corrected.
* **Site**: `ferrodec-decimal32/src/ops/rem.rs` (former
  `MAX_SAFE_SHIFT = 12` over a `u64` register).
* **Pinned case refuted as a defect**: `rem(4.194304E+33,
  -3.145728E+18)`. The true truncated integer quotient
  `trunc(4194304E+15 / 3145728) ≈ 1.33 × 10^15` has about 16 digits,
  far beyond Decimal32's `PRECISION = 7`. The General Decimal
  Arithmetic `remainder` operation plus IEEE 754-2019 §7.2 mandate
  `Invalid_operation`, so Decimal32's `NaN`/`INVALID` is
  spec-correct. The Phase 0 cross-check oracle expected Decimal64's
  finite remainder, which exists only because Decimal64's own
  `Division_impossible` budget is 10^16 digits, not 10^7. The oracle
  was unsound for `rem` and is now corrected.
* **Sound witness (genuine defect)**: `rem(1E+13, 9999999)` under any
  rounding mode. The alignment shift is `13 > 12` (the old static
  bound), yet the true integer quotient is
  `10^13 / 9_999_999 = 1_000_000` (7 digits, inside `PRECISION`) and
  the exact remainder is `1_000_000`, representable. Pre-fix result:
  `(NaN, INVALID)`. Spec answer: `1.000000E+6`, `OK`. Companion
  zero-remainder witness: `rem(1E+13, 5000000)`, quotient
  `2_000_000`, remainder `0`.
* **Cause**: the static `MAX_SAFE_SHIFT` was the `u64` alignment
  overflow guard for the chosen register width; using it as the
  `Division_impossible` predicate rejected pairs whose true integer
  quotient was small. IEEE 754-2019 keeps the §5.3.1 remainder
  definition separate from the §7.2 invalid case (quotient exceeds
  the format digit budget). Decimal64 H5 shape.
* **Sufficiency note for the dynamic bound**: with a `u128` register
  (`U128_DIGIT_CAP = 38`) and Decimal32's at-most-7-digit
  coefficients, a residual register-overflow case requires
  `shift > 38 − digit_count(coef) ≥ 31`. With the dividend dominant
  and the divisor at most 7 digits, the integer quotient then has at
  least `38 − 7 − 1 = 30` digits, far beyond `PRECISION = 7`. So
  every case that still trips the dynamic overflow branch is
  genuinely `Division_impossible`; the dynamic bound never produces a
  spurious `INVALID`, while the static `u64` bound at 12 did.
* **Fix landed**: one slice. Corrected the rem arm of
  `tests/d64_crosscheck.rs` (`rem_oracle_check`: compute the integer
  quotient digit count from the widened operands, expect
  `NaN`/`INVALID` when it exceeds 7 digits, the narrowed Decimal64
  remainder otherwise). Replaced `MAX_SAFE_SHIFT` and the `u64`
  `POW10_U64` table with the dynamic per-side digit-count bound over
  `u128`, mirroring `fma.rs` and decimal64 `rem.rs`;
  `quotient >= COEFFICIENT_LIMIT` is the sole `INVALID` predicate.
  Folds A2-F2.
* **Guard**: `rem_matches_decimal64` (now the dedicated GDA oracle
  block) and `rem_large_shift_neighborhood` are un-ignored and
  active, plus the in-crate unit regressions
  `rem_h2_wide_gap_small_quotient_is_finite` and
  `rem_pinned_known_issue_h3_is_spec_invalid`.

### H3 — FMA early returns pack an out of range biased exponent through a release no-op `debug_assert!`

* **Agents**: A3-F1, A3-F2. The decimal64 H3 class. **This is the
  conditional `ferrodec-ieee` 0.1.2 to 0.1.3 trigger**, not the
  quantum surface (A4 confirmed quantum does not trigger it).
* **Tier**: H. Release builds pack garbage bits with `Status::OK`.
* **Site**: `ferrodec-decimal32/src/ops/fma.rs:118-128` (both zero
  return) and `:199-211` (exact cancellation return), each doing
  `(target_q + BIAS as i32) as u32` into `bid::pack_finite`; the no-op
  gate is `ferrodec-decimal32/src/bid.rs:217`
  (`debug_assert!(biased_exp <= BIASED_EXP_MAX)`).
* **Reproducer**: `fma(0E-101, 0E-101, 0E-101)` any mode:
  `target_q = -202`, `(-202 + 101) as u32 = 4_294_967_195`, packed
  unchecked. The exact cancellation mirror triggers on any opposite
  sign pair with `ab_u128 == c_u128` and `target_q < -101`.
* **Cause**: IEEE 754-2019 §6.3 fixes the FMA preferred quantum at
  `min(Q(x*y), Q(z))`, which for Decimal32 reaches `-202`, below the
  representable minimum `-101`. §6.3/§7.4 require clamping the result
  quantum into range and raising the informational `Clamped`; instead
  the code wraps i32 to u32 and relies on a release elided assert.
* **Fix shape**: M1 typed `BiasedExp`/`Coefficient` newtypes with
  `clamp_unbiased` (decimal64 H3 shape); route both early returns
  through it and raise `Status::CLAMPED`. This bumps
  `ferrodec-ieee` from `0.1.2` to `0.1.3` in
  `ferrodec-decimal32/Cargo.toml` and switches
  `tests/conformance.rs` from the raw `status.bits()` compare to
  `ferrodec_test_support::conformance::status_conformance_eq` in the
  same commit.

### H4 — FMA overflow early returns omit the effective subtract borrow and extend

* **Agent**: A3-F3. Decimal64 fd-d47 / H2 mirror.
* **Tier**: H. Directed rounding tips one ULP the wrong way on
  effective subtraction.
* **Site**: `ferrodec-decimal32/src/ops/fma.rs:164-176` and
  `:180-190` (the `shift_ab > ab_safe_shift` and
  `shift_c > c_safe_shift` arms). They set `pre_sticky` and return
  the dominant side without computing `effective_sub` or applying a
  borrow.
* **Reproducer**: large positive product with a far smaller opposite
  sign `c` truncated past `c_safe_shift`, e.g.
  `fma(9.999999E+80, 9.999999E+80, -1E-101)` under TowardNegative or
  TowardZero.
* **Cause**: the funnel's `pre_sticky` encodes an additive residue;
  on effective subtraction the residue is subtractive, so the
  dominant coefficient must be decremented one ULP and re-extended
  before the funnel reads a round digit (IEEE 754-2019 §4.3
  directed roundings, §7 single rounding for fusedMultiplyAdd).
  decimal32's fma is the in crate reference only for the dynamic per
  side shift; it does not carry the H2 mirror borrow extend.
* **Fix shape**: one commit. Port the `effective_sub && pre_sticky`
  borrow and extend (decimal64 `fma.rs` `h2_borrow_and_extend`) into
  both overflow early returns. Add an fma cross-check or a targeted
  unit reproducer (fma is not in `d64_crosscheck.rs`).

### H5 — quantize on a zero coefficient at a deep target quantum returns `NaN` / `INVALID`

* **Agent**: A4-F1. Decimal64 H6 / `ddqua537`.
* **Tier**: H. Wrong result plus spurious `INVALID` on a valid,
  exactly representable operation.
* **Site**: `ferrodec-decimal32/src/ops/quantum.rs:205-221` (the
  `target_q < self_q` pad branch; `new_digits = if coef == 0 { pad }`
  at `:212-216`, no zero short circuit).
* **Reproducer**: `Decimal32::ZERO.quantize(1E-95)` returns
  `(NaN, INVALID)`; spec `0E-95`, `OK`. Format floor analogue:
  `quantize(0E+0, 1E-101)`.
* **Cause**: IEEE 754-2019 §5.3.3 makes validity depend on whether
  the result fits at the target quantum; a zero coefficient is
  representable at every encodable quantum. The non zero digit count
  gate is wrongly applied to `coef == 0`, counting padding zeros as
  significant.
* **Fix shape**: one commit. Short circuit `coef == 0` to
  `pack_finite(sign, target_biased, 0)` with `Status::OK` before the
  digit count gate (decimal64 H6 shape).

### H6 — `to_f64` has the pre fix decimal64 H7 dishonest signature (BREAKING)

* **Agent**: A5-F1.
* **Tier**: H, BREAKING. Version bump driver: see Phase N+2.
* **Site**: `ferrodec-decimal32/src/convert/binary.rs:27`, verbatim
  `pub fn to_f64(self) -> f64`; sNaN arm `binary.rs:29` returns a
  bare `f64::NAN`.
* **Reproducer**: `Decimal32::SIGNALING_NAN.to_f64()` raises no
  status; the caller cannot observe the invalid operation.
* **Cause**: IEEE 754-2019 §5.4.2 / §7.2: a signaling NaN to
  convertFormat signals invalid. The signature has no `Status`
  channel, so `INVALID` is unrepresentable. Exactly decimal64 H7.
* **Fix shape**: one commit. Change to
  `to_f64(self, RoundingMode) -> (f64, Status)`, sNaN arm
  `(f64::NAN, Status::INVALID)`; migrate callers. Breaking, called
  out in the CHANGELOG and the version decision.

### H7 — no inherent `to_f32`; the `ToPrimitive` path double rounds and loses sNaN `INVALID` (BREAKING surface)

* **Agent**: A5-F2.
* **Tier**: H, BREAKING surface. Pairs with H6 for the version
  decision.
* **Site**: inherent `to_f32` absent;
  `ferrodec-decimal32/src/num_traits_impls.rs:200`
  `Some(Decimal32::to_f64(*self) as f32)`.
* **Reproducer**: `<Decimal32 as ToPrimitive>::to_f32(&sNaN)` returns
  `Some(NaN)` no `INVALID`; decimal to f64 to f32 double rounds (M4
  pet case class).
* **Cause**: IEEE 754-2019 §5.4.2 requires the sNaN to `INVALID`
  signal and a single correctly rounded narrowing to binary32.
* **Fix shape**: one commit. Add inherent
  `to_f32(self, RoundingMode) -> (f32, Status)` formatting the
  decimal once onto the binary32 grid (decimal64 `binary.rs` shape);
  rewire the trait through it.

### H8 — `parse_str` counter overflow and wrap on adversarial input (SECURITY)

* **Agent**: A5-F4. Decimal64 H8 / L12.
* **Tier**: H. Debug build arithmetic overflow panic (DoS); release
  build silent misparse.
* **Threat model**: the attacker supplies an arbitrary `&str` to the
  public `parse_str` / `FromStr` / `Num::from_str_radix` (calculator
  field, config value, deserialized record). Harms: debug panic, or a
  wrapped counter feeding a wrong `unbiased_exp` into
  `round_and_pack_finite` (silent contamination of a durable
  artifact). `parse_str` is the entry point that answers it.
* **Site**: `ferrodec-decimal32/src/convert/parse.rs:139`
  (`digits_after_point += 1`, not saturating, not capped),
  `:157` (`extra_int_digits` capped at `u32::MAX` only, then
  `as i32` wraps to `-1` at `:233-234`); no
  `const _: () = assert!(MAX_EXPONENT_MAGNITUDE <= i32::MAX as u32)`.
* **Reproducers**: `"0." + 5_000_000_000 zeros` overflows
  `digits_after_point` (debug panic). `"1" + 3_000_000_000 zeros`
  saturates then `as i32 == -1`, parsing a huge value with a near
  zero exponent.
* **Cause**: the explicit exponent loop is capped at
  `MAX_EXPONENT_MAGNITUDE`; the implicit exponent counters are not.
  IEEE 754-2019 §5.12 admits arbitrarily long literals; a conformant
  parser bounds them deterministically (overflow to ±∞/±0 or reject),
  never panics or wraps.
* **Fix shape**: one commit. Make `:139` saturating, cap both
  counters at `MAX_EXPONENT_MAGNITUDE` returning `ExponentOutOfRange`,
  add the static assert (decimal64 `parse.rs:74,184,215`). Add a
  `tests/regression_*` integration test (no alloc in crate cannot
  build megabyte strings).

## M tier

* **M1 — typed `BiasedExp` / `Coefficient` newtypes in
  `bid.rs`** (A1-F4, A2-F3, A3, A4-F3). Lifts the `pack_finite`
  preconditions (`bid.rs:216-217`) and the upstream
  `debug_assert!`s (`round.rs:282`, `quantum.rs:222`, `sqrt.rs:144`,
  `rem.rs:154/164`) from release elided runtime convention into the
  type system, total constructors returning a clamped flag. Decimal64
  H3 shape. Sequenced first in the H area because H3/H4 depend on it.
* **M2 — `scaleb` has no `|n|` envelope and overflows i32** (A4-F2,
  decimal64 M2). `quantum.rs:255-266`. Add
  `SCALEB_N_LIMIT = 2 * (E_MAX + PRECISION) = 206`; reject
  `n.unsigned_abs() > SCALEB_N_LIMIT` with `(NAN, INVALID)` before
  the exponent arithmetic.
* **M3 — `from_f64` never raises `INVALID` on a signaling f64 bit
  pattern** (A5-F3, decimal64 M3). `binary.rs:62-66`. Detect the
  cleared binary64 quiet bit.
* **M4 — missing exact integer conversion surface** (A5-F6, A5-F10,
  decimal64 M5). No `convert/int.rs`; `to_i64`/`to_u64` route through
  f64 + `libm_round` (double round, wrong None/INVALID contract,
  no RoundingMode). Add `src/convert/int.rs`
  (`to_i32/i64/i128/u32/u64/u128`, RoundingMode aware, exact
  reduction, None iff INVALID); rewire the num-traits delegates.
* **M5 — Kani parity port: five special only shim groups** (A6-F5,
  decimal64 M10-M15). decimal32's exp/ln, trig, hyper, pow, quantum
  inline their special `match` with no extracted
  `*_special_cases` helper and no `*_special_only_for_kani` shim.
  Per group: one behavior preserving extraction commit
  (`<op>_special_cases(class) -> Option<(Decimal32, Status)>` shared
  by production and a `#[cfg(kani)]` shim) then one commit adding the
  ported `verify/<group>.rs` and the gated `verify/mod.rs` line.
  Roughly ten commits. Constants from decimal32 `bid.rs`, harnesses
  ported from `ferrodec-decimal64/src/verify/{exp,trig,hyper,pow,quantum}.rs`.
  Gates and runs: `exp-log`/`trig`/`hyperbolic`/`pow` for the first
  four; quantum is UNCONDITIONAL (`ops/quantum.rs` ungated, confirmed
  byte identical to decimal64), runnable under `--features fmt`.
  CBMC budget cut points (A6-F1/F2/F3): the `quantize` extraction
  stops before line 91 (the finite rescale loops), `next_up`/
  `next_down` resolve only NaN/±0/±∞, and `equals_one`'s `k > 6`
  guard stays inside the extracted `pow` helper.
* **M6 — formalize the Decimal64 cross-check as a permanent
  deliverable** (Phase 0a infrastructure, this slice). Closure is
  the progressive un-ignore of the add/sub/rem blocks as H1/H2 land,
  plus adding a `div` cross-check block (A2-F4 noted div is not
  cross-checked; Decimal64 div double rounds like add/sub, so it is a
  strong screen with the documented caveat).

## L tier

* **L1 — zero engineering rendering** (A5-F7). `format.rs:241-280`
  pads a zero coefficient with positional zeros (`0E+5` engineering
  renders `000E+3`). Special case `coef == 0` in `write_engineering`
  (and verify `write_plain`). Decimal64 carried an analogous zero
  render fix.
* **L2 — round trip cohort property test** (A5-F8). Add an
  exhaustive or randomized `parse_str(Display(d)) == d` guard over
  sampled BID patterns; current coverage is 8 hand picked strings.
* **L3 — audited safe, documentation only** (A5-F5 coefficient
  accumulation bounded by the 16 digit budget, A5-F9 `from_utf8`
  unwrap on internally built ASCII). Add a one line invariant comment
  each; no behavior change.
* **L4 — dangling `rem_special_only_for_kani` with no
  `verify/rem.rs`** (A6-F6). Pre existing, shared with decimal64, so
  parity neutral. Optional follow up bead; not blocking.

## Confirmations (no defect, recorded for completeness)

* **round.rs rounding kernel** (A1-F5): `should_round_up` matches
  IEEE 754-2019 §4.3 for all five modes; the cohort pad/strip logic
  matches §6.3. The H1/H2 errors are entirely upstream in addsub
  alignment; round.rs faithfully rounds corrupted inputs.
* **mul, div** (A2-F4, A2-F5): finite paths correct; `q_preferred`
  matches §6.3; div digit generation bound is operand digit count
  bounded, not magnitude bounded. mul is the exact cross-check
  oracle.
* **sqrt, logb, next_up, next_down** (A4): ideal exponent
  `floor(exp/2)`, specials, and the operand independent isqrt window
  all correct against the decimal64 reference. sqrt cannot reach the
  §6.3 ideal exponent clamp (halving keeps results well inside
  range), so it does not trigger the CLAMPED bump; only `scaleb` to
  the E_MAX boundary hits the already documented decimal64 Clamped
  limitation and is not a new requirement.
* **f64 bridge bound independence** (A6-F4): every loop bound in
  `pow10_f64`, `from_f64`, and `parse_str` derives from the BID or
  f64 format width, never operand magnitude; no `debug_assert!` on
  input derived arithmetic in the transcendental modules. Record this
  argument in the slice ADR.

## Work order (Phase 2..N)

1. M1 typed newtypes (structural; unblocks H3/H4).
2. H1 addsub dynamic window + zero fast path.
3. H2 rem: correct the oracle arm, then dynamic bound.
4. H3 FMA biased exp clamp (triggers ferrodec-ieee 0.1.3 +
   `status_conformance_eq`).
5. H4 FMA effective subtract borrow extend.
6. H5 quantize zero short circuit.
7. H6 `to_f64` signature (BREAKING).
8. H7 `to_f32` inherent (BREAKING surface).
9. H8 `parse_str` counter saturation (security; regression test).
10. M2 scaleb envelope, M3 from_f64 sNaN, M4 `convert/int.rs`.
11. M5 Kani parity port (≈10 commits, behavior preserving extraction
    then harness per group).
12. M6 cross-check formalization (un-ignore per fix; add div block).
13. L1 zero engineering render, L2 round trip guard, L3 comments,
    L4 optional bead.
14. Phase N+1: dsEncode `#hex` arm. Phase N+2: release. H6/H7 force a
    version decision (minor with a documented breaking note as
    decimal64 1.4.0 did, or major) to be put to the user before the
    release commit.

Each fix is one concern per commit, reproducer red on main and green
on the fix, full per commit gate (fmt, clippy, rustdoc, workspace
test, `cargo kani -p ferrodec-decimal32 --features <relevant>`),
constants from decimal32 `bid.rs`/`fma.rs` never decimal64,
Garner/Gopen prose, unsigned commits, explicit path staging.
