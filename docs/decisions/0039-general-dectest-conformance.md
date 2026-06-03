# ADR-0039: General decTest conformance for ferrodec-decimal

- **Status**: accepted
- **Date**: 2026-06-02

## Context

`ferrodec-decimal` 0.1.0 validated its 23 operation surface cohort exact against
CPython libmpdec, the General Decimal Arithmetic reference, through a randomized
differential (`tests/differential.rs`, 8000 cases, opt-in behind the
`differential` feature). ADR-0038 names vendoring the static general decTest
suite as one of the two substantive gates on the path to 1.0, complementary to
that differential: the spec authors chose the static cases deliberately, and
they exercise corners a constant-seed random sweep does not reach. Two such
corners turned out to matter. The differential's operand generator never emits a
negative zero, and its driver runs with clamping off, so the sign of zero in the
selection and reduction operations and the entire `Clamped` flag went untested.

The fixed format crates already vendor the width specific `dq*` / `dd*` / `ds*`
files and run them through the shared `ferrodec-test-support` conformance driver.
That driver does not transfer to an arbitrary precision crate: it tracks neither
`clamp` nor the three General Decimal Arithmetic rounding modes (`half_down`,
`up`, `05up`), and it masks `Clamped` on the comparison because the fixed
formats decline those modes (ADR-0005). The general files, by contrast, set
`precision`, `maxExponent`, `minExponent`, `rounding`, and `clamp` per file
through in-file directives, which is exactly the arbitrary precision contract.

## Decision

Vendor the general (precision driven) decTest files for the implemented
operation surface under `ferrodec-decimal/tests/vectors/`, verbatim with their
IBM and Cowlishaw copyright headers, and run them through a bespoke runner in
`ferrodec-decimal/tests/conformance.rs`.

The runner builds a `ferrodec_decimal::Context` per file from the directives,
including `clamp`; maps all eight rounding directives onto the crate's
`Rounding` (there is no skip for rounding bucket, unlike the fixed format
harnesses); dispatches the 23 operations plus `toSci` and `apply`; and compares
cohort exact, representation equality on the value and an exact match on the
status flags with `Clamped` and `Underflow` compared for real. `toSci` reads its
operand under the context, so it rounds a finite value and rejects a NaN whose
payload exceeds the precision as `conversion_syntax`; `apply` preserves a zero's
sign that `plus` would resolve away. Per file pass counts are pinned under the
ADR-0010 record then pin discipline; the authoritative table is
`tests/conformance.rs::expected_per_file`.

The decTest line parser is copied from the workspace root runner rather than
shared from `ferrodec-test-support`, because depending on that crate pulls its
`num-bigint` oracle into this crate's development build, and the `.decTest`
grammar is frozen, so the copy will not drift.

## Consequences

The first run found four defects in the shipped 0.1.0 surface, all corrected on
the same branch (commit `8fb6ee6`), each with a unit reproducer and cross
checked against libmpdec:

- `max` and `min` rounded the selected operand with `plus`, which resolves a
  zero's sign through the add from zero rule, so a selected negative zero became
  positive. The pick now keeps the operand's own sign.
- `reduce` took the sign of its `plus` rounded value, dropping a negative zero's
  sign.
- `divide` of a finite by an infinity is a signed zero at Etiny; the exponent is
  constrained, so it now signals `Clamped`.
- The rounding core did not signal `Clamped` when a nonzero value rounded away to
  zero in the subnormal range, although the exact zero path already did; the two
  paths now agree.

After the fixes the suite stands at 16305 pass, 0 fail, 244 skip across 25
vendored files. The skip taxonomy is recorded in `ferrodec-decimal/KNOWN_ISSUES.md`;
in brief it is three categories: to-engineering output, which the `fmt` surface
advertises but does not yet implement (174 cases, tracked as a separate defect);
inputs whose exponent exceeds `i32`, which is outside this crate's deliberate
representation bound (16 cases); and fixed-width encoding literals in `#hex` or
`NN#` notation, which an arbitrary precision value cannot reproduce (54 cases).

The conformance test runs on the default `fmt` feature, so it is part of a plain
`cargo test`. In a debug build its runtime is dominated by the unoptimized
schoolbook `DecBig` arithmetic and is several minutes; a release build is far
faster, and routine iteration can target `--lib`. The runtime drops with the
perf pass that ADR-0038 defers.

The randomized libmpdec differential still passes unchanged after the fixes,
confirming the corrected inputs lie outside its distribution rather than
contradicting it. The two checks are complementary: the differential sweeps
breadth, the static suite covers the spec authors' chosen corners.

## Related

- Plan: `plans/spicy-dazzling-deer.md`
- Commits: `684ec07` (vendor), `8fb6ee6` (fixes), `a4c561c` (runner)
- Issues: `fd-rd0.1` / `fd-rd0.2` / `fd-rd0.3` (closed), `fd-7la` (to-engineering gap)
- Other ADRs: builds on ADR-0010 (per-file pins) and ADR-0038 (the 1.0 gates)
