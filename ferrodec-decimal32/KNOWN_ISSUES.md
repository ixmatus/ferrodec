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

## Coverage gap: dsEncode dispatch (BID `#hex` operand decoding)

* **Status**: by design. Conformance dispatcher currently routes
  `tosci` / `apply` only.
* **Symptom**: `dsEncode.decTest` reports 2 of 268 cases pass (the
  two that route via `parse_str` without needing the `#hex` BID
  interchange decoder). The remaining 266 cases skip pending a
  dedicated dispatch arm that decodes 8-char hex strings into the
  32-bit BID pattern. (Decimal128's analog is the
  `Encoding::Bid` path in `tests/conformance.rs`.)
* **Closing this gap**: add a `parse_dsencode_hex` helper that
  zero-pads short inputs and routes through
  `Decimal32::from_bits`. Wire it into the dispatcher behind a check
  on operand-prefix `#`. Estimated 30 lines of code; deferred
  because the conformance signal is narrow (decimal32's vendored
  vector set is intentionally minimal — only `dsBase` and `dsEncode`
  ship in `tests/vectors/`).

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

### H3: rem static `MAX_SAFE_SHIFT` raises spurious `INVALID`

* **Reproducer**: `rem(4.194304E+33, -3.145728E+18)` under
  `NearestEven` (`a` bits `0x50000000`, `b` bits `0xAF100000`).
* **Wrong answer**: `NaN` with `INVALID`. **Spec answer**:
  `1.048576E+18`.
* **Mechanism (hypothesis)**: the fixed `MAX_SAFE_SHIFT = 12` in
  `src/ops/rem.rs` conflates a u64 overflow guard with the quotient
  digit count, so an alignment shift past the static bound returns
  `Division_impossible` even though the quotient is small and the
  remainder is representable. This is the decimal64 H5 shape; the fix
  is the same dynamic per side bound.

When each defect closes, this section's entry moves to a closed audit
trail in the same commit as the fix.
