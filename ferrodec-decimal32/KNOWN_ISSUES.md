# ferrodec-decimal32 known issues

Tracks coverage gaps, deferred work, and the closed correctness
audit trail. The 2026-05-11 conformance investigation (see
`docs/decisions/0017-decimal64-conformance-coverage-gap.md` in the
workspace root) found no correctness bugs in `Decimal32` because the
dispatcher exercised only `tosci` and `apply`. The decimal32
correctness slice (1.4.0, 2026-05-16, ADR-0019) added a `Decimal64`
cross-check oracle and a six agent review, which closed eight H, six
M, and four L findings. The closed defects are recorded as an audit
trail in the last section. The remaining open sections are coverage
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

## Resolved: transcendentals route through f64 / libm

* **Status**: closed by the fd-r0l train. The whole transcendental
  surface — the exp-log family (`exp`, `ln`, `exp2`, `log2`,
  `log10`), `cbrt`, the whole trig family (`sin`, `cos`, `tan`,
  `asin`, `acos`, `atan`, `atan2`), the whole hyperbolic family
  (`sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`), and `pow` —
  is faithfully rounded via the shared `ferrodec-transcend`
  Extended-precision kernel (≤ 1 ULP at 7 digits, every IEEE
  754-2019 rounding direction), at exact parity with the Decimal128
  parent and the decimal64 sibling. No operation routes through f64
  any more; `libm` is no longer a dependency of this crate.
* **What shipped**: the proven Decimal128 `Extended` kernel was
  lifted into a shared `ferrodec-transcend` crate (so the dependency
  graph stays acyclic rather than depending on the parent `ferrodec`
  crate); `ferrodec-decimal32` depends on `ferrodec-transcend` /
  `ferrodec-multiword` (pulled by `exp-log`) and stays
  astro-float-free. The exp-log family and `cbrt` migrated in the
  P2 phase, the trig family in the P3 phase (Payne-Hanek argument
  reduction), the hyperbolic family in the P4 phase (on the
  already-faithful exp / ln primitives), and `pow` in the P5 phase
  (`exp(y · ln(|x|))` at Extended precision with the bit-exact
  integer-exponent fast path) on the identical `DecimalFormat`
  seam. With `pow` migrated, no `src/` code makes any functional
  `libm` call, so the P5 cleanup dropped `dep:libm` from the
  `exp-log` / `trig` feature arrays and removed the `libm`
  dependency line. The `pow_special_cases` short-circuits and the
  ADR-0016 Kani shims stayed byte-identical across every phase. The
  faithful contract is proven by the per-family `tests/property_*`
  suites (`property_exp`, `property_ln`, `property_cbrt`,
  `property_sincos`, `property_inverse_trig`, `property_hyperbolic`,
  `property_pow`, `property_pow_specials`), astro-float-free
  (Design A). ADR-0021 records the faithful-rounding contract; the
  closing fd-r0l ADR records the train.

## Closed correctness audit trail (decimal32 1.4.0 slice)

The `Decimal64` cross-check oracle
(`ferrodec-decimal32/tests/d64_crosscheck.rs`) and the six agent
review closed the defects below. Every cross-check block is now
active with zero ignored; each fix carries a reproducer that is red
on the pre slice tree and green on the fix. Entries are kept as the
audit trail.

### H1: addsub static `ALIGN_LIMIT` window dropped the residue — CLOSED

* **Closed** by commit `45cdfaf` (dynamic per side shift over a u128
  register plus the zero operand fast path; the fd-d47 power of ten
  regime is guarded by `eeb9f72`).
* **Reproducers**: `add(-1E-101, 1E-88)` `TowardZero` returned
  `1.000000E-88`, now `9.999999E-89`; `sub(-0E-74, -3.145728E-95)`
  `NearestEven` returned `1E-101`, now `3.145728E-95`.
* **Mechanism**: the fixed `ALIGN_LIMIT = 12` window routed the
  lower operand to sticky once the exponent gap exceeded the static
  bound, losing the effective subtract borrow, and the drop branch
  treated a signed zero as the dominant operand. The decimal64
  fd-d47 and H1 shape; `src/ops/fma.rs` was the in crate dynamic
  reference.

### H2: addsub asymmetric-zero magnitude loss — CLOSED

* **Closed** by the same commit `45cdfaf` (one root cause with H1;
  the zero operand fast path handles it).
* **Reproducer**: covered by the H1 `sub(-0E-74, -3.145728E-95)`
  case above.

### H3 (rem): static `MAX_SAFE_SHIFT` raised spurious `INVALID` — CLOSED (defect confirmed, oracle was unsound)

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

### Remaining closed findings (H4, H5, H6, H7, H8, M2, M3, M4, L1)

H4 (FMA effective subtract borrow), H5 (quantize zero short
circuit), H6 (the breaking `to_f64` signature), H7 (inherent
`to_f32`), H8 (the `parse_str` adversarial counter cap, a security
fix), M2 (the `scaleb` envelope), M3 (`from_f64` signaling bit), M4
(the exact integer conversion surface), and L1 (zero engineering
rendering) all landed in the slice. The full per finding accounting
with IEEE section citations and before and after values is in
`CHANGELOG.md` under `[1.4.0]`; ADR-0019 records the train. The
`Decimal64` cross-check (`tests/d64_crosscheck.rs`, seven blocks,
zero ignored) plus the Kani special case proofs are the permanent
regression net.
