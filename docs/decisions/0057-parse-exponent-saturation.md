# ADR-0057: Saturate out-of-range explicit exponents in `parse_str`

- **Status**: accepted
- **Date**: 2026-07-25

## Context

fd-aqs.11 vendored `dqBase.decTest`, whose `toSci` section feeds the
parser exponent fields of astronomical magnitude (`1E-999999999`,
`111e9999999999`). GDA to-number and decNumber treat such a literal as
an ordinary out-of-range value: it overflows to `Infinity` with
`Overflow Inexact Rounded` or underflows to `0E-6176` with `Underflow
Subnormal Inexact Rounded Clamped`. IEEE 754-2019 §5.4.3
(`convertFromDecimalCharacter`) agrees: the literal denotes a finite
value, and a finite value outside the format range rounds and signals
per §7.4; only a malformed string is a conversion error.

All three fixed-format parsers instead rejected the field with
`ParseDecimalError::ExponentOutOfRange` once its accumulated magnitude
passed `MAX_EXPONENT_MAGNITUDE` (1 000 000). The conformance runners
skipped those cases (`invoke -> None`), recorded as KNOWN_ISSUES §4:
42 `dqBase` cases, with sibling analogues in `ddBase` / `dsBase`. The
parsers already handled in-cap out-of-range exponents correctly
(`1E+7000` overflows, `1E-99999` underflows); only the extreme
*exponent field* rejected rather than saturating. Values, not syntax,
were being refused, which contradicts the crate's IEEE positioning.

A naive fix — clamping the field at `MAX_EXPONENT_MAGNITUDE` itself —
is unsound. The composite exponent subtracts up to
`MAX_EXPONENT_MAGNITUDE` fractional digit positions (and adds up to
the same in integer positions), so a clamped `+1 000 000` field
combined with a million-digit fraction lands back at exponent zero and
parses as a finite in-range number for a value that is astronomically
out of range.

## Decision

An explicit `e`/`E` exponent field whose magnitude exceeds
`MAX_EXPONENT_MAGNITUDE` saturates to the sentinel
`EXPONENT_FIELD_SATURATED = 2_000_000_000` instead of rejecting; the
scan continues so trailing syntax errors still reject. The sentinel's
two required properties are compile-time asserted in each parser: the
composite exponent stays inside `i32` after every digit-position
adjustment (each capped at `MAX_EXPONENT_MAGNITUDE`, coefficient
contribution capped at `MAX_PARSED_DIGITS`), and no adjustment can
pull it back inside a representable range, so the existing
`round_and_pack_finite` path sees an unambiguous overflow, underflow,
or (for a zero coefficient) quantum clamp. No new rounding logic: past
the cap the exact magnitude cannot matter, so the saturated parse must
agree bit-for-bit, flags included, with any in-cap far-out-of-range
twin — the property the new unit tests pin under all five rounding
directions.

The literal-length caps stay rejections. A coefficient digit run or
leading-fractional-zero run past 1 000 000 positions (the H8 guards)
still returns `CoefficientOverflow`: saturating those soundly would
require tracking sticky digits across a multi-megabyte literal for an
input class no conformance vector exercises and no calculator or
interchange use produces. The line is: exponent *fields* are values
and saturate; digit *runs* are literal length and cap.

`ParseDecimalError::ExponentOutOfRange` is removed from all three
crates. With the saturation in place the variant is unreachable, and
the 4.0.0 boundary this arc ships under (ADR-0058) permits the
breaking removal; keeping a dead variant with a "no longer produced"
note was rejected as surface the compiler can no longer connect to
behavior.

## Consequences

- The 42 `dqBase` extreme-exponent skips become passes
  (`dqBase.decTest` pin 629 -> 671), with the sibling `ddBase` /
  `dsBase` analogues following; KNOWN_ISSUES §4 closes.
- `parse_str` is total over one more slice of the input space: every
  syntactically valid finite literal now produces a value and flags.
  Callers that matched `ExponentOutOfRange` to detect "absurd
  exponent" must instead inspect the returned status for
  `overflow()` / `underflow()`, which is also the more honest signal
  (the in-cap `1E+7000` always took that path).
- Removing the variant breaks any downstream `match` that names it.
  No published crate exists (3.4.0 was never pushed to crates.io), so
  the cost is confined to this repository's own tests and runners,
  all updated in the same change.
- The sentinel constant is duplicated per crate alongside the parsers
  it guards, matching the existing byte-identical-parser convention;
  a drift between the three copies would be caught by the shared
  conformance corpus, not by the compiler.

## Related

- Issue: fd-uit (discovered-from fd-aqs.11)
- Other ADRs: ADR-0029 item 2 (variant taxonomy this narrows),
  ADR-0018 (H8 literal-length guards, unchanged), ADR-0010 (per-file
  pins that record the recovery), ADR-0058 (the 4.0.0 boundary)
- KNOWN_ISSUES §4 (the skip class this closes)
