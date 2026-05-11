# ferrodec-decimal32 known issues

Tracks coverage gaps and deferred work. The 2026-05-11 conformance
investigation (see
`docs/decisions/0017-decimal64-conformance-coverage-gap.md` in the
workspace root) found **no correctness bugs** in `Decimal32` itself,
unlike `Decimal64`. The entries below are coverage / scope gaps, not
correctness defects.

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

## No known correctness bugs

The 2026-05-11 investigation that found three correctness bug
classes in `Decimal64` did not surface any analogous failures in
`Decimal32`. This is consistent with the published 1.2.0 release
notes; if a future review reveals defects, they land in this file.
