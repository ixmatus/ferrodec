# ferrodec-decimal32 known issues

Tracks coverage gaps, deferred work, and confirmed correctness
defects. The 2026-05-11 conformance investigation (see
`docs/decisions/0017-decimal64-conformance-coverage-gap.md` in the
workspace root) found no correctness bugs in `Decimal32` because the
dispatcher exercised only `tosci` and `apply`. The 2026-05-15
decimal32 correctness slice added a `Decimal64` cross-check oracle
(every finite `Decimal32` is exactly representable in `Decimal64`,
and `Decimal64` reached full conformance in its 1.4.0 slice), and it
immediately surfaced three correctness defect classes in the
arithmetic paths. Those are pinned in the first section below and
tracked for the slice's H tier. The remaining sections are coverage
and scope gaps, not correctness defects.

## Resolved: dsEncode DPD interchange dispatch

* **Status**: resolved. `dsEncode.decTest` is a DPD interchange
  vector file: its header reads "Selected DPD codes" and the `#hex`
  literals are IEEE 754-2019 decimal32 DPD byte patterns (8 hex
  chars = 4 big-endian bytes), not BID raw bits. An earlier draft
  of this section misdescribed it as a "BID `#hex` decoder" gap;
  that wording was inaccurate and is corrected here.
* **What shipped**: a `dpd`-gated DPD interchange codec
  (`ferrodec-decimal32/src/dpd.rs`): the format-independent declet
  primitive (pure IEEE 754-2008 §3.5.2 boolean equations, no lookup
  tables) plus `Decimal32::to_dpd_bytes` / `from_dpd_bytes` for the
  32-bit interchange framing (1 sign bit, 5-bit combination field,
  6-bit exponent continuation, 2 declets). BID stays the arithmetic
  storage encoding (ADR-0001); DPD is a byte-level interchange
  adapter only (ADR-0009). The codec is off by default to preserve
  the embedded code-size floor.
* **Conformance with `dpd` on**: `dsEncode.decTest` passes 250 of
  268 cases (up from 2). dsBase is unchanged at 698. With the `dpd`
  feature off the dispatcher skips every `#hex` case, so the
  feature-off baseline holds (`dsEncode` = 2). The
  `expected_per_file` table in `tests/conformance.rs` is
  feature-conditional and pins both counts exactly (ADR-0010).
* **Residual skips (18, `dpd` on)**: every residual is a
  `value -> #hex` case carrying a `Clamped` condition (decs035,
  decs037, decs130, decs132, decs400, decs413..437, decs601..611).
  decimal32's `parse_str` does not perform the IEEE 754-2019 §7.4
  preferred-exponent clamp, so the encoded bytes for these inputs
  legitimately differ. The matching `#hex -> value` decode
  direction passes for all of them, so the codec itself is fully
  exercised; the gap is a `parse_str` quantization-policy edge,
  tracked below under "dsBase residual skips" (the same §7.4 clamp
  decision). No DPD codec defect remains.

## Coverage gap: dsBase residual skips (deferred parse edges)

* **Status**: by design. `dsBase.decTest` reports 698 of 909 cases
  pass. The 211 skips break down as ~7 pathologically large
  exponents (deferred, see `ParseDecimalError::ExponentOutOfRange`)
  plus ~204 cases under non-IEEE rounding directives
  (`half_down`, `05up`) which mirror ferrodec's ADR-0005 posture of
  not coercing decTest's extra modes onto an IEEE mode.
* **Closing this gap**: lift `parse_str`'s exponent saturation
  policy to match the dec spec (return ±Inf or ±0 at parse time
  rather than `Err(ExponentOutOfRange)`). Cross-crate decision —
  see decimal128 for the parallel concern.

## Coverage gap: transcendentals route through f64 / libm

* **Status**: documented as v1.0 baseline in each
  `src/ops/{exp,trig,hyper,pow}.rs` docstring.
* **Symptom**: `exp` / `ln` / `pow` / `sin` / `cos` / ... convert to
  `f64`, call the corresponding `libm` function, convert back. f64's
  ~15.95-digit precision is comfortably above Decimal32's 7 digits,
  so the round-trip error stays under 1 ULP at the boundary. But the
  result is faithfully-rounded (≤ 1 ULP) rather than
  correctly-rounded (exact best-rounding).
* **Closing this gap**: route through Decimal128's `Extended`
  precision kernel and round once to Decimal32. Requires an
  architectural decision (Decimal32 currently depends only on
  `ferrodec-ieee`, not on the parent `ferrodec` crate). Tracked for
  a 1.16-era follow-up; see the 1.15 cycle plan at
  `~/.claude/plans/spawn-6-agents-explore-wondrous-hamster.md`
  (Slice D was originally bundled with the transcendentals routing
  but the slice's correctness scope grew during execution).

## Confirmed correctness defects (decimal32 correctness slice)

The `Decimal64` cross-check oracle
(`ferrodec-decimal32/tests/d64_crosscheck.rs`) surfaced three defect
classes on 2026-05-15. Each reproducer below is the minimal failing
input from the property search; the spec answer is the `Decimal64`
result rounded to 7 digits. The matching cross-check block carries an
`#[ignore]` whose reason names the reproducer, so the harness stays
green until the fix; the H tier fix removes the `#[ignore]` and the
block becomes the permanent guard. These three also seed the Phase 1
six agent review, which sweeps the full op surface for the remaining
tier.

### H1: addsub static `ALIGN_LIMIT` window drops the residue

* **Reproducer**: `add(-1E-101, 1E-88)` under `TowardZero` (`a` bits
  `0x80000001`, `b` bits `0x00D00001`).
* **Wrong answer**: `1.000000E-88`. **Spec answer**: `9.999999E-89`.
* **Mechanism (hypothesis)**: the fixed `ALIGN_LIMIT = 12` window in
  `src/ops/addsub.rs` routes the lower operand to sticky only once
  the exponent gap exceeds the static bound, so the borrow from the
  effective subtraction is lost and the directed rounding tips the
  wrong way. This is the decimal64 fd-d47 and H2 shape. The in crate
  reference is `src/ops/fma.rs`, which already keys its per side
  shift on the actual digit count.

### H2: asymmetric-zero magnitude loss in addsub

* **Reproducer**: `sub(-0E-74, -3.145728E-95)` under `NearestEven`
  (`a` bits `0x81B00000`, `b` bits `0x8C000000`).
* **Wrong answer**: `1E-101`. **Spec answer**: `3.145728E-95`.
* **Mechanism (hypothesis)**: subtracting a finite operand from a
  signed zero with a distant exponent loses the operand's magnitude
  and yields a near zero instead of the operand. This is the
  decimal64 H1 shape (asymmetric zero addsub magnitude loss).

### H3: rem static `MAX_SAFE_SHIFT` raises spurious `INVALID` — CLOSED (defect confirmed, oracle was unsound)

* **Status**: closed 2026-05-15 by the H2 rem slice. The pinned
  reproducer was an unsound oracle false positive; a *different*
  sound witness confirmed the underlying static-window defect, and
  the dynamic bound landed.
* **Pinned reproducer was spec-correct**: `rem(4.194304E+33,
  -3.145728E+18)` (`a` bits `0x50000000`, `b` bits `0xAF100000`).
  Decimal32 returns `NaN` with `INVALID`, and that is the
  spec-correct General Decimal Arithmetic answer. The true truncated
  integer quotient `trunc(4194304E+15 / 3145728)` is about
  `1.33 × 10^15`, roughly 16 digits, far beyond Decimal32's
  `PRECISION = 7`, so GDA `remainder` plus IEEE 754-2019 §7.2 mandate
  `Invalid_operation`. The Phase 0 cross-check oracle keyed on
  Decimal64's *finite* remainder, which exists only because
  Decimal64's own `Division_impossible` budget is 10^16 digits, not
  10^7. The oracle was unsound for `rem`; it has been corrected (see
  `tests/d64_crosscheck.rs`, `rem_oracle_check`) to assert the GDA
  result, and the pinned case is now a regression test asserting
  `NaN`/`INVALID` (`rem_pinned_known_issue_h3_is_spec_invalid`).
* **Genuine defect, sound witness**: `rem(1E+13, 9999999)` under any
  rounding mode. The alignment shift is `13 − 0 = 13`, past the old
  static `MAX_SAFE_SHIFT = 12`, but the true integer quotient is
  `10^13 / 9_999_999 = 1_000_000` (7 digits, inside `PRECISION`) and
  the exact remainder is `1_000_000`, representable. Pre-fix the code
  returned `(NaN, INVALID)`; the spec answer is `1.000000E+6`, `OK`.
  Companion zero-remainder witness: `rem(1E+13, 5000000)`, quotient
  `2_000_000`, remainder `0`.
* **Mechanism (confirmed)**: the fixed `MAX_SAFE_SHIFT = 12` over a
  `u64` alignment register conflated "aligning the operand overflows
  the register" with the GDA `Division_impossible` digit-budget test.
  Those are distinct: IEEE 754-2019 §5.3.1 defines the remainder and
  §7.2 owns the digit-count overflow. The static window rejected
  pairs whose integer quotient was small. This is the decimal64 H5
  shape.
* **Fix**: `MAX_SAFE_SHIFT` (and the `u64` `POW10_U64` table) replaced
  by a dynamic per-side bound over a `u128` register keyed on
  `decimal_digit_count_u128`, mirroring the in-crate `src/ops/fma.rs`
  and the decimal64 `src/ops/rem.rs` H5 fix. The sole
  `Invalid_operation` predicate is now `quotient >=
  COEFFICIENT_LIMIT`. For Decimal32's 7-digit coefficients and a
  `u128` register, every residual register-overflow case
  (`shift > 38 − digit_count`) provably has an integer quotient
  exceeding 7 digits, so its `INVALID` is spec-correct.

When each defect closes, this section's entry moves to a closed audit
trail in the same commit as the fix.
