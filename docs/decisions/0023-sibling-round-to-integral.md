# ADR-0023: decimal64 / decimal32 roundToIntegral (§5.9 completion)

- **Status**: accepted
- **Date**: 2026-05-17

## Context

IEEE 754-2019 §5.9 makes `roundToIntegralExact` and the
`roundToIntegralTiesToEven` / `roundToIntegralTiesAway` /
`roundToIntegralToward{Positive,Negative,Zero}` family mandatory for
every format. Only the parent `Decimal128` carried them (S11, fd-wkf);
`ferrodec-decimal64` and `ferrodec-decimal32` had no
`round_to_integral`, `round_to_integral_exact`, or the `floor` / `ceil`
/ `trunc` / `round` / `round_ties_even` wrappers. The published-intent
sibling crates therefore failed a mandatory operation, and
`ddToIntegral.decTest` (178 `->` cases) was unrunnable for decimal64.
The S11 scope gate surfaced the gap and filed fd-hnx.

An empirical scope gate (run six times, every premise confirmed)
established the boundary: decimal64 ships
`tests/vectors/ddToIntegral.decTest`, so conformance dispatch is in
scope there; decimal32 ships only `dsBase` / `dsEncode` with no
integral vectors, so it is property plus Kani only, with no
conformance dispatch. `round_to_integral*` was absent in both
siblings and present in the parent.

Round-to-integral is an exact operation: the result is the operand
rounded to an integer under the active direction, at the GDA preferred
quantum `max(exponent, 0)`, with no precision loss, no over/underflow,
`INVALID` only for a signaling NaN, and `INEXACT` only for the
`…Exact` variant when a non-zero fractional part is discarded. The
exact correctly-rounded oracle (ADR-0021), not a tolerance envelope,
is the arbiter.

## Decision

Port the parent §5.9 kernel to both siblings. `src/ops/integral.rs`
in each crate carries the typed-BID kernel: the loop-free
`round_to_integral_special_cases` resolves the non-finite and zero
classes (the parent ADR-0016 shim strategy), and the finite path
drops `-unbiased` low digits with a local `should_round_up_int`
transcribing the §4.3.3 table. The algorithm derives from IEEE §5.3
with the parent kernel as a behaviour oracle; identifiers, structure,
and the d32 deltas (`PRECISION = 7`, `COEFFICIENT_LIMIT: u32 = 10^7`,
`decimal_digit_count(u32)`, `BIAS = 101`) were chosen fresh for each
crate's typed bid API. The carry-into-a-new-decade branch is retained
defensively though it is unreachable for round-to-integral: dropping
at least one fractional digit caps the kept integer below
`COEFFICIENT_LIMIT`.

Verification follows the established tiers:

- `src/verify/integral.rs` in both crates: five Kani harnesses driving
  the loop-free special-only shim through the shared
  `verify::operand` set, so CBMC never unrolls the finite digit-drop
  loop.
- `tests/property_integral.rs` in both crates: a 4096-case exact
  correctly-rounded sweep over the finite domain and every rounding
  direction, asserting the decoded `(sign, coefficient, exponent)`
  cohort and the conformance-masked status bit-for-bit. The reference
  decodes the operand with the vetted cohort-faithful
  `decode_decimalNN` and splits digits on the decimal string, so an
  exponent at `qmin` never overflows a `10^drop`.
- decimal64 only: `tointegral` / `tointegralx` dispatch wired into
  `tests/conformance.rs`, with `ddToIntegral.decTest` pinned at its
  exact observed pass count.

The exact-oracle sweep surfaced one instrument defect, reproduced
before fixing. The broad `any::<uNN>()` generator produces
non-canonical Form-B encodings whose raw coefficient is `≥ 10^P`;
`ferrodec` canonicalises these to `±0` (the documented BID layout
rule in `src/bid.rs`), but `decode_decimalNN` returns the raw
coefficient, so the oracle saw a spurious fractional part and expected
`INEXACT` where production correctly raised nothing. The fix applies
the same documented canonicalisation rule in the oracle, keeping it
production-independent. No `ferrodec` correctness defect surfaced:
`ddToIntegral.decTest` is 164 of 178 with zero failures, the 14 skips
being non-IEEE rounding directives, operands past the parser cap, and
`#`-hex interchange.

These ship within the existing 1.4.0 sibling crates as the §5.9
completion; the added API surface is the parent's, unchanged in shape.

## Consequences

Both siblings now implement the full mandatory §5.9 family with the
same public surface as the parent. The exact oracle is a standing
regression guard at 4096 cases per crate per run; the
`ddToIntegral.decTest` pin (164) makes any future drift visible in git
under the ADR-0010 exact-match discipline. The persisted decimal32
`property_integral.proptest-regressions` seed pins the non-canonical
Form-B case that exposed the instrument defect.

Provenance is clean: the kernel is derived from IEEE §5.3 with the
parent as a behaviour oracle and fresh per-crate identifiers; the
oracle canonicalisation rule is derived from the in-repo BID layout
documentation, not from recall.

This ADR does not supersede any prior ADR. It completes §5.9 across
all three precisions, the sibling analogue of the parent S11 / fd-wkf
work, with the exact correctly-rounded oracle (ADR-0021) as the
arbiter.

## Related

- Issues: fd-hnx (filed, discovered-from fd-wkf / S11)
- Other ADRs: parent §5.9 testing strategy ADR-0010; exact-oracle
  contract ADR-0021; Kani special-only shim strategy ADR-0016
- Engagement: faithfulness-remediation follow-up
  (`memory/project_faithfulness_remediation.md`)
