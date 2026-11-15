# Wave 3 inputs (every B reader reads this after common.md)

Wave 2 closed at `6bf3574`. What the first two waves established, and
what they hand to the format crate readers:

- **Trust table.** `readers/C2.md` ("Trust table (for Waves 2 and 3)")
  lists every arithmetic instrument with what it asserts, its pin,
  and its blind spot. Cite it when you answer "why the stack missed
  it". Two rows matter most to you: the root `property_*` arithmetic
  suites exclude zeros and structured coefficients, and the siblings
  have no exact oracle property suite for add, sub, mul, div, sqrt
  (C2-10; the reader's 180,000 case differential found nothing).
- **The exact integer oracle is validated (C2).**
  `ferrodec-test-support/src/oracle.rs`: 309,000 cases against
  libmpdec at every mode and format, zero unexplained mismatches. It
  models no CLAMPED and no out of format operands; use CPython
  `decimal` with `clamp=1` beside it where CLAMPED matters. Usage
  examples: `tests/oracle_soundness.rs`, `tests/property_addsub.rs`.
- **P0 already found in your area.** `Decimal128::fma(10, 1E33,
  -1E-102)` returns `10^34 − 10` for `10^34 − 1` under TowardZero and
  TowardNegative: `src/ops/fma.rs:791-804` drops one digit and
  `kept.sub(1)` ignores the binade (`reproducers/P1B-3`, reader C2's
  driver). Do not re-report it; do look for the same shape wherever a
  rounding step subtracts or adds one at a power of ten
  (`addsub::sub_ulp_effective_sub`, `round_and_pack_finite`, the
  sibling fma which is separate code and was clean on the same
  family).
- **Recurring defect classes from Waves 1 and 2, carry them into
  every file:** special case dispatch order (an early return on a
  quiet NaN before the signaling check: C2-2, C2-3; the sibling
  `pow_special_cases` copies reproduce `pow(±0, −∞)` with DIV_BY_ZERO
  and disagree with the root on `pow(sNaN, ±0)`: A5-4, A5-5);
  cohorts of special rows (the transcend kernels' zero rows return a
  canonical zero or one at exponent 0 and drop the §9.2.2 preferred
  quantum: A2-1); gate constants borrowed from a neighbour with a
  wrong sign derivation (the Decimal128 `exp` underflow gate 14221
  sits inside the `1E-6176` region, `src/math/format_impl.rs:150`:
  A5-2; `sinh`/`cosh` saturate on `exp`'s gate though `e^x/2`
  overflows `ln 2` later: A5-1); seeds or intermediates rounded
  through the destination format (P1B-2, fd-k9t, A3-1); tests that
  compare a copy of a constant with itself (C1-12) or pin a count
  floor instead of a value (C1).
- **Contradictions in your area from Wave 1** (`wave1-verification.md`
  "Contradiction list"): `%` is per format mathematics with rustdoc
  that cites ADR-0027 falsely (D3-4, B3); `ferrodec-decimal` `to_f64`
  sets INEXACT unconditionally with a false doc premise (D3-6, B4);
  `ferrodec-decimal/src/arith.rs:248` `u32::try_from(...).expect` at
  precision `u32::MAX` (B4); the d64 and d32 KNOWN_ISSUES and
  CHANGELOG numbers (D2-18, 19, 21, 22, B2); the d64
  `transcend_impl.rs` underflow comment "≈ −916.98" (true −917.122,
  gate safe; A5-10, B2); `bigconst.rs:437-487` has `inv_pi_digits`
  inserted inside the doc comment written for `inv_ln10_digits`
  (D2 lead, B5).
- **Phase 1 artifacts you receive:** `counts.md` (declarations, `as`
  casts and panic sites per crate; corpus rows; the battery totals);
  `mirror_summary.md` (normalized diff sizes per mirrored file pair;
  regenerate the full diffs with
  `bash docs/archive/review-2026-10/mirror_diff.sh <out-dir>` into
  your scratch); `phase1b-derivations.md` §4 (the `bigconst`
  generators pinned against mpmath).
- **June's verdicts in your area** (`docs/archive/REPORT-rigorous-
  review-2026-06-09.md`): the arithmetic pipelines, multiword carry
  and divide, the ieee kernel, codecs and total order were found
  clean; the confirmed findings were the `MIN_POSITIVE_NORMAL` class
  (a named constant with no test reading its value), the d64
  `POW10_U64` table sized for the wrong format, NaN payload
  canonicality in `[10^33, 2^110)`, and the Algorithm D add back
  branch with zero coverage. Re-derive where the diff since `3b9f063`
  touched or where this sheet points; spend the rest on the lens.
