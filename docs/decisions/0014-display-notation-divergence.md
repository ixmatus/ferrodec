# ADR-0014: `Display` notation divergence between Decimal128 and the siblings

- **Status**: accepted
- **Date**: 2026-05-10

## Context

The default `Display` (the `{}` format) for the three ferrodec
sibling crates produces different strings for the same numeric
value. The 6-agent correctness review surfaced this as Finding M4:

| Input parsed by each crate | Decimal128 `to_string()` | Decimal32 / Decimal64 `to_string()` |
| --- | --- | --- |
| `"1E+3"` | `"1000"` | `"1E+3"` |
| `"1.0"` | `"1.0"` | `"1.0"` |
| `"1E-9"` | `"1E-9"` | `"1E-9"` |
| `"1E+22"` | `"1E+22"` | `"1E+22"` |

The two rules:

- **Decimal128** uses a binary-float-style boundary: plain notation
  when the magnitude is roughly in `[10⁻⁶, 10²¹)`. Specifically
  `src/convert/format.rs:309-315`:
  ```text
  fixed iff (-6 < scale ≤ 21 && unbiased ≤ 0) || (unbiased ≥ 0 && scale ≤ 21)
  ```
  where `scale = digits + unbiased_exp`. The second clause is the
  divergence: any value with non-negative quantum and ≤ 21 magnitude
  digits is rendered as an integer (with cohort-preserving trailing
  zeros), regardless of how the user spelled it on input.

- **Decimal32 / Decimal64** use the General Decimal Arithmetic
  `toSci` rule (Cowlishaw §3.1, IBM decTest's expected output):
  plain notation iff `unbiased_exp ≤ 0 && adjusted_exp ≥ -6`. The
  adjusted exponent is `unbiased_exp + digits − 1`. Any value with
  `unbiased_exp > 0` (i.e. quantum to the *left* of the decimal
  point) is rendered in scientific notation, preserving the
  exponent the user typed.

Both rules are sensible defaults for their respective target
audiences:

- Decimal128's rule reads like `f64::Display` and produces the
  result a user migrating from `f64` would expect: `1E+3` looks
  like 1000, and the library writes "1000" by default. Has been
  the default since v1.0; 14 minor releases of momentum and
  unknown downstream reliance.

- The siblings' rule matches the IBM decTest conformance suite
  (the suite's expected-output column is `toSci`), so the
  conformance harness can string-compare the formatted result
  against the vector. It also matches Python's `decimal.Decimal`,
  C99 `_Decimal*`, and Java's `BigDecimal` `toString()` — the
  cohort the input was typed with is the cohort the output
  preserves.

## Decision

Leave the divergence in place for the v1.x release line. Document
the call here so future contributors don't accidentally "harmonize"
either side without breaking compatibility.

The principled long-term direction is `toSci` for all three crates:
the GDA spec is the canonical decimal-IO convention; the alignment
with decTest / Python / Java is the right convergence point.
Adopting it on Decimal128 is a v2.0 concern (downstream `Display`
output changes for any value with `unbiased_exp ≥ 0`, e.g. parsing
`"1E+3"` then printing it).

When v2.0 of `ferrodec` lands:

1. Switch `Decimal128`'s default `Display` to `toSci`.
2. Document the change in `CHANGELOG.md` under `Breaking`.
3. Optionally provide an opt-in `Notation::FixedPreferred` mode
   for callers who want the v1.x behaviour back.

For v1.x:

- The divergence is documented in each crate's `Display` impl
  rustdoc (callers reading docs see "Decimal128 uses
  binary-float-like boundary; siblings use GDA `toSci`").
- The siblings' choice is locked in: it's required for the
  conformance harness to round-trip vectors correctly.
- Cross-precision interop callers who need a single uniform output
  rule should call the crate's `Engineering` adapter (or roll
  their own formatter on top of `coefficient` + `unbiased_exp`),
  not rely on `Display`.

## Consequences

**Wins.**

- No behaviour change for any current downstream user. Decimal128
  v1.x.x stays byte-identical in its `Display` output.
- The siblings' conformance harness keeps working without a
  parallel `to_dectest_string` formatting path.
- The v2.0 harmonization is queued with a clear plan and rationale.

**Costs.**

- `f(Decimal32(1) * Decimal128(1000))` produces inconsistent
  string output depending on which side's `Display` is invoked
  last. Mitigation: use `Engineering` adapter or hand-format on
  `(coefficient, unbiased_exp)` for cross-precision callers who
  need a uniform rule.
- The "default `Display` is GDA `toSci`" promise the siblings'
  README implies is *partly* misleading: it's true for the
  siblings, false for `ferrodec`. Each crate's `Display` rustdoc
  now mentions the divergence.

**Non-consequences.**

- Conformance counts unchanged. Decimal128's harness compares via
  `partial_cmp`, not `Display`; the rule choice is independent of
  conformance pass / fail.
- No effect on `LowerExp` / `UpperExp` (always scientific) or
  `Engineering` (always engineering). Only `Display` (the `{}`
  format) is affected.

## Related

- Plan: `plans/2026-05-09-workspace-and-decimal-siblings.md`
  (planning artifact for the sibling crates).
- ADR-0012: `ferrodec-ieee` extraction — same shape (cross-
  precision-consistent surface vs Decimal128's mature behaviour).
- ADR-0013: conformance-harness consolidation — the conformance
  shape that the siblings' `toSci` choice supports.
- 6-agent review (2026-05-10), Finding M4.
- General Decimal Arithmetic Specification (Mike Cowlishaw),
  §3.1 "Numbers" / `toSci` definition.
