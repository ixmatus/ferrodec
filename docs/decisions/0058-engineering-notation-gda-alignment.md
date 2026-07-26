# ADR-0058: Align `Engineering` with GDA to-engineering-string (4.0.0)

- **Status**: accepted
- **Date**: 2026-07-25

## Context

All three fixed formats have shipped a public `Engineering` Display
adapter since the 1.5.0 era. Its layout was derived by analogy from
the SI prefix grid, not from the specification: it always renders
exponentially with the exponent a multiple of three and the mantissa
in `[1, 1000)`, and its zero path was a documented simplification (a
lone `0` with the non-rebased adjusted exponent) carrying an explicit
code comment that the GDA fractional-zero form was "not exercised by
the vendored conformance corpus".

fd-aqs.11 changed the corpus. The vendored `dqBase` / `ddBase` /
`dsBase` files carry the full GDA `toEng` sections, and they exercise
exactly the three places the adapter diverges from the
specification's to-engineering-string:

| Input (cohort)  | Pre-4.0 `Engineering` | GDA `toEng`  |
|-----------------|-----------------------|--------------|
| `12345` (exp 0) | `12.345E+3`           | `12345`      |
| `10e1`          | `100E+0`              | `100`        |
| `0E+1`          | `0E+1`                | `0.00E+3`    |

GDA's to-engineering-string is identical to to-scientific-string for
special values and for any magnitude shown in plain form (the
`exp <= 0 && adjusted >= -6` rule); the engineering layout applies
only "if an exponent is needed", a shown exponent of zero is omitted
entirely, and a zero coefficient shows its quantum rounded *up* to a
multiple of three with the gap rendered as fractional zeros
(`0E+1 -> 0.00E+3`, `0E-7 -> 0.0E-6`, `0E-9 -> 0E-9`). Python's
`Decimal.to_eng_string` and `ferrodec-decimal::to_eng_string` both
follow the specification, making the fixed formats' adapter the
ecosystem outlier. The conformance runners could not dispatch `toEng`
against it without failing, so 146 `dqBase` cases (with `ddBase` /
`dsBase` analogues) sat skipped, recorded as KNOWN_ISSUES §3.

Three ways out were weighed. Align the adapter in place and take the
major bump (a formatter semantics change is breaking under the
ADR-0014 precedent, which shipped the Display `toSci` switch in 2.0).
Align in place but call it a minor bump (rejected: loose SemVer
against the repo's own precedent). Add a second, GDA-exact adapter
beside the old one (rejected: two near-identical engineering
formatters on the public surface forever, for a distinction no user
has asked for). The choice was put to the user; alignment plus major
bump won.

## Decision

`Engineering` on `Decimal128`, `Decimal64`, and `Decimal32` renders
GDA to-engineering-string:

- Plain form under exactly the `toSci` plain rule (`exp <= 0 &&
  adjusted >= -6`), including zeros with quanta in `[-6, 0]`.
- Exponential form otherwise, with the shown exponent rebased down to
  a multiple of three, one to three integer digits, and a shown
  exponent of zero omitted entirely.
- Zero coefficients outside the plain range show the quantum rounded
  up to a multiple of three, the gap as fractional zeros.

An explicit `{:.N}` precision keeps the pre-4.0 forced-exponential
shape: the precision path already quantizes the mantissa for that
layout, and `{:.N}` similarly pins `Display` to fixed regardless of
the `toSci` rule, so precision choosing a definite shape is the
established convention.

The conformance runners dispatch `toeng` by rendering through the
adapter and comparing the string (a cohort-distinct comparison; the
`toSci` value-comparison path cannot express `10E+12` vs `1.0E+13`)
plus the parse status. The per-file pins record the recovered cases
exactly.

The three fixed-format crates bump 3.4.0 -> 4.0.0. The v3.4.0 tags
exist but were never published to crates.io, so the publish queue
simply becomes 4.0.0; `ParseDecimalError::ExponentOutOfRange`'s
removal (ADR-0057) rides the same boundary.

## Consequences

- The `toEng` skip classes close: `dqBase` rises 671 -> 817 and
  KNOWN_ISSUES §3 is deleted, with `ddBase` / `dsBase` rising by
  their own `toEng` counts.
- Every caller-visible rendering in the table above changes. No
  published crate exists, so no external caller can break; the
  repository's own doctests, unit pins, and examples are updated in
  the same change, and the adapter rustdoc now states the
  specification contract instead of the `[1, 1000)` mantissa claim.
- Values in the plain range no longer read as SI-grid triplets
  (`12345`, not `12.345E+3`). A caller wanting unconditional
  engineering-exponential output has `{:.N}` (which retains it) or
  can rebase from `to_parts`; if a real need surfaces, an explicit
  always-exponential adapter can return as an additive minor later,
  named for what it is rather than squatting on the `toEng` name.
- The three formats and `ferrodec-decimal` now agree with each other
  and with Python on every `toEng` rendering, so cross-checking one
  implementation against another is meaningful again.

## Related

- Issue: fd-zf8 (discovered-from fd-uit)
- Other ADRs: ADR-0014 (the Display `toSci` precedent this follows),
  ADR-0057 (the parse saturation sharing the 4.0.0 boundary),
  ADR-0010 (per-file pins)
- KNOWN_ISSUES §3 (the skip class this closes)
