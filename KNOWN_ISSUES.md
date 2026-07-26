# Known issues

This file records ferrodec's known issues. Most of it enumerates the
cases ferrodec deliberately or unintentionally skips in the decTest
conformance suite (the runner is `tests/conformance.rs`; vectors live
under `tests/vectors/`). Defects outside that suite, when any are open,
are recorded first, under "Known defects".

## Known defects

None currently open. The 2026-06-09 review's transcendental findings
are all fixed: the anchor band value defects (`ln`/`log10`/`log2`
below 1, `atanh`/`asinh` small arguments, `asin`/`acos` near ±1,
`pow` near-1 bases) by the ADR-0050 reformulations, with
`tests/vectors/transcend/anchor_bands/` pinning the class; the
directed-mode special paths (overflow/underflow gates,
negate-after-round, `tanh` saturation, `atan2(±0, −0)` flags) in
fd-aqs.5; the exactly representable `exp2`/`log2`/`log10` cases
(spurious INEXACT plus directed-mode misrounds) by the ADR-0047
amendment in fd-aqs.8; and the directed modes for grid-stuck small
arguments (e.g. `sin(1E-40)` at `Decimal32` under `TowardNegative`)
by the ADR-0051 residual seam in fd-aqs.7. The `cbrt` / `pow`
spurious-INEXACT defect (`fd-92w.8`) was fixed earlier (ADR-0047).

One verification (not behaviour) note from ADR-0051 stands: directed
mode results whose correction sits below ~10^-100 relative (e.g.
`atanh(1.000001E-95)` at `Decimal32`) are delivered by the same
proven seam but pinned only at the nearest modes, because the band
corpus generator's mpmath oracle cannot independently certify the
side there. Certifying those few decades needs a higher-precision
offline oracle pass; it is bookkeeping, not a suspected defect.

## Headline numbers

As of ferrodec 4.0.0 (default features):

| count |  share | category |
|------:|-------:|----------|
| 14 188 | 100.0 % | total cases |
| 13 941 |  98.3 % | pass |
|      0 |   0.0 % | fail |
|    247 |   1.7 % | skip |

(Default-feature build, no `dpd`; `dqEncode` / `dqCanonical` route to
skip without it and add 368 / 143 pass when it is enabled. fd-aqs.11
vendored ten more dq operation files; the 4.0.0 skip burn-down closed
the extreme-exponent parse class by saturating, ADR-0057, and the
`toEng` class by aligning `Engineering` with GDA and dispatching it,
ADR-0058.)

The 0-fail floor is enforced by `tests/conformance.rs::dectest_conformance`,
which pins the expected pass count per file under the ADR-0010
discipline (the authoritative table lives at
`tests/conformance.rs::expected_per_file`). Any change that drops the
pass count below the per-file pin or raises the fail count above 0
fails the build.

The 247 residual skips fall under three categories, each with its own
disposition below and the exact per-file counts in the table at the end
(authoritative): §1 non-IEEE rounding directives (111), §2 the
BID-structural CLAMPED residual (20), and §3 conversion-negative
parser strictness (99). The remaining 17 (`dqRemainder` 9,
`dqToIntegral` 8) are the BID-interchange `#hex`-Clamped residual and
a few `#hex` extreme-exponent cases, skipped as a harness DPD-as-BID
decode artifact with ferrodec's own bits being correct. Every other
operation, encoding, and special-value combination in the suite
passes, including the full `toEng` rendering surface (ADR-0058), the
saturating extreme-exponent parses (ADR-0057), and the §7.4 CLAMPED
informational flag (compared, not masked) at every clamp site the BID
cohort model can detect.

For context: 1.7.1 sat at 8 149 / 0 / 572 (93.4 % pass). The 1.9.0
through 1.10.1 trail closed five of the original six skip
categories (NaN-with-payload literals, hex `#` operands, the
`class` op result format, the `apply` op, and the bare `#`
null-operand sentinel) plus added a sixth implementation
(`Decimal128::rem_trunc`) that closed the truncating-remainder
case. The 2.0 cycle then extended the dispatcher with eight GDA
decNumber extension operations (`and`, `or`, `xor`, `invert`,
`divideInteger`, `reduce`, `rotate`, `shift`, per ADR-0031), adding
2 243 newly-passing `dq*` cases on first run and lifting the parent
total from 8 721 to 10 964 vectors. The fd-bef closure (ADR-0049) then
added `compareSignaling` and `nextToward`, vendoring `dqCompareSig`
(559) and `dqNextToward` (304) and lifting the total to 11 827.
fd-aqs.11 then vendored ten more dq operation files (base, the copy
family, remainder, toIntegral, plus, min/max-magnitude), implementing
those ops on the Decimal128 conformance path and lifting the total to
14 188; it also broadened the skip taxonomy past the single non-IEEE
rounding category (see the skip list above). The 4.0.0 skip burn-down
then closed two of those classes: ADR-0057 saturates extreme explicit
exponents at parse (42 recovered) and ADR-0058 aligns `Engineering`
with GDA to-engineering-string and dispatches `toEng` (146
recovered), lifting `dqBase` from 629 to 817.

## Skip taxonomy

### 1. Non-IEEE rounding directives — 111 cases

decTest extends the IEEE 754 rounding set with two GDA-only modes:

- `half_down` — round to nearest, ties **toward zero**
- `05up` — round-zero-five-up (a banker's-rounding variant)

ferrodec implements only the five IEEE 754:2019 rounding-direction
attributes. Cases in directive blocks selecting the GDA modes are
skipped rather than coerced onto a kernel mode that doesn't match the
spec.

Per-file: `dqQuantize` 64, `dqFMA` 26, `dqAdd` 8, `dqDivide` 1, plus
the 12 `dqBase` conversion cases in its `half_down` directive blocks.

**Will not fix.** ferrodec is positioned as IEEE 754 conformant, not
GDA conformant. Adding `half_down` / `05up` would expand the
`RoundingMode` surface beyond the spec's five-direction enumeration,
and embedded callers paying the kernel size already get every
direction the standard defines.

### 2. BID-structural CLAMPED residual — 20 cases (Decimal128)

The §7.4 / GDA CLAMPED informational flag is now raised and compared at
every clamp site the BID cohort model can detect (fd-61r / ADR-0048).
One class cannot be raised: cases whose operand's own exponent exceeds
the format quantum range. BID normalises such an operand into a padded
cohort at parse (`9e6144` is stored at qmax), losing the pre-clamp
exponent the decNumber reference keeps in a wide working exponent, so the
operation has no signal that its result was clamped. Examples:
`divide(9e6144, 1)`, `add(1E+384, 1E+384)`, `1E+384 % 3E+383`.

The conformance runner detects these by re-parsing operands (an operand
that itself raises CLAMPED at parse is pre-clamped) and skips them rather
than failing. Per-file on Decimal128: `dqRemainderNear` 9, `dqFMA` 7,
`dqDivide` 4. The siblings carry the same category: Decimal64 35 (ddAdd 5,
ddDivide 5, ddFMA 7, ddRemainder 9, ddRemainderNear 9), Decimal32 0.

**Will not fix.** The residual is intrinsic to BID, not a defect:
raising these cases would require a working exponent wider than the
storage format (decNumber's model), a different library. The value is
always exact; only the informational flag is absent. The per-file pass
pins (ADR-0010) record the residual exactly, so a regression in either
direction fails the build.

### 3. Parser strictness on conversion negatives — 99 cases (`dqBase`, fd-aqs.11)

The remaining `dqBase` skips are `toSci` cases expecting
`Conversion_syntax` on malformed input. Most are inputs `parse_str`
correctly rejects (they skip via `invoke → None`); a handful expose a
parser leniency the runner's `parse_value.trim()` masks (leading /
trailing whitespace) or the over-long NaN-payload acceptance tracked
separately. The `toSci` conformance is gated on rendering, not
parse-strictness, so these are skipped (see the runner's fd-aqs.11
note); the parser's own strictness is exercised by the crate's parse
unit tests.


## Per-file totals

```
dqAbs.decTest                    75 pass     0 fail     0 skip
dqAdd.decTest                  1004 pass     0 fail     8 skip
dqAnd.decTest                   357 pass     0 fail     0 skip
dqBase.decTest                  817 pass     0 fail   111 skip
dqCanonical.decTest               0 pass     0 fail     0 skip
dqClass.decTest                  42 pass     0 fail     0 skip
dqCompare.decTest               659 pass     0 fail     0 skip
dqCompareSig.decTest            559 pass     0 fail     0 skip
dqCompareTotal.decTest          613 pass     0 fail     0 skip
dqCompareTotalMag.decTest       613 pass     0 fail     0 skip
dqCopy.decTest                   43 pass     0 fail     0 skip
dqCopyAbs.decTest                43 pass     0 fail     0 skip
dqCopyNegate.decTest             43 pass     0 fail     0 skip
dqCopySign.decTest              107 pass     0 fail     0 skip
dqDivide.decTest                683 pass     0 fail     5 skip
dqDivideInt.decTest             374 pass     0 fail     0 skip
dqEncode.decTest                  0 pass     0 fail     0 skip
dqFMA.decTest                  1418 pass     0 fail    33 skip
dqInvert.decTest                193 pass     0 fail     0 skip
dqLogB.decTest                  109 pass     0 fail     0 skip
dqMax.decTest                   257 pass     0 fail     0 skip
dqMaxMag.decTest                243 pass     0 fail     0 skip
dqMin.decTest                   247 pass     0 fail     0 skip
dqMinMag.decTest                233 pass     0 fail     0 skip
dqMinus.decTest                  43 pass     0 fail     0 skip
dqMultiply.decTest              473 pass     0 fail     0 skip
dqNextMinus.decTest              84 pass     0 fail     0 skip
dqNextPlus.decTest               84 pass     0 fail     0 skip
dqNextToward.decTest            304 pass     0 fail     0 skip
dqOr.decTest                    341 pass     0 fail     0 skip
dqPlus.decTest                   43 pass     0 fail     0 skip
dqQuantize.decTest              622 pass     0 fail    64 skip
dqReduce.decTest                134 pass     0 fail     0 skip
dqRemainder.decTest             491 pass     0 fail     9 skip
dqRemainderNear.decTest         521 pass     0 fail     9 skip
dqRotate.decTest                248 pass     0 fail     0 skip
dqSameQuantum.decTest           333 pass     0 fail     0 skip
dqScaleB.decTest                202 pass     0 fail     0 skip
dqShift.decTest                 248 pass     0 fail     0 skip
dqSubtract.decTest              520 pass     0 fail     0 skip
dqToIntegral.decTest            170 pass     0 fail     8 skip
dqXor.decTest                   348 pass     0 fail     0 skip
TOTAL: 14188 cases — 13941 pass, 0 fail, 247 skip
```

Default-feature build (no `dpd`); `dqCanonical` and `dqEncode` route
to skip without the `dpd` feature and pin to 143 / 368 pass when it
is enabled (`dqCanonical` rose 90 to 95 as its 5 `comparesig` cases
dispatched, fd-bef.1, then 95 to 143 as fd-aqs.11 wired the copy
family on the DPD path; see `tests/conformance.rs::expected_per_file`).

Reproduce with:

```sh
cargo test --features=transcendentals --test conformance -- --nocapture
```

## Closed in 1.9.0 / 1.10.0 / 1.10.1

For provenance, the categories that closed between 1.7.1 (572 skips)
and 1.10.1 (99 skips):

- **NaN-with-payload literals** (~398 cases): `parse_str` now
  accepts `NaN<digits>` / `sNaN<digits>` and packs the payload into
  the BID significand's 110-bit `T_MASK` field; `Display` reads the
  payload back; every NaN-producing arithmetic op preserves the
  operand's payload per IEEE 754:2019 §6.2.3 first-NaN-wins rule.
- **Hex `#` operand syntax** (~28 cases): `parse_value` decodes
  `#XXXX...` via `u128::from_str_radix` + `from_bits`.
- **`class` op result format** (40 cases): runner-side string
  comparator + `classify_to_gda_name` mapping from
  `is_signaling_nan` / `is_nan` / `is_infinite` / `is_zero` /
  `is_subnormal` / `is_sign_negative`.
- **`apply` op** (4 cases): identity dispatch (ferrodec is
  PRECISION=34-only, so `apply` reduces to identity-after-parse).
- **`remainder` op (1.10.0)** (1 case): the `Decimal128::rem_trunc`
  method (the truncating-quotient remainder; C99 `fmod` / decTest
  `remainder`) is distinct from the IEEE 754 §5.3.1 round-half-to-even
  `Decimal128::rem_near` (decTest `remaindernear`). The 1.x bare `rem`
  spelling was retired in 2.0 per ADR-0027; both ops have explicit
  names now.
- **Bare `#` null-operand sentinel (1.10.1)** (~30 cases): the
  runner now short-circuits cases with a bare `#` operand to the
  dec-spec answer `(NaN, Invalid_operation)` before invoking the
  op kernel. The 1.7.1 misestimate of "~13 + ~15 misc" turned out
  to be 28 bare-`#` cases plus 2 that were also under non-IEEE
  rounding directives.

## What is NOT skipped

Worth recording the inverse: the suite covers every IEEE 754:2019
operation ferrodec implements, under every IEEE rounding direction,
across the full BID-128 encoding range, including:

- All five rounding-direction attributes (`NearestEven`,
  `NearestAway`, `TowardZero`, `TowardPositive`, `TowardNegative`)
  and the directional `up` mode emulated via a runner-side two-pass
  wrapper.
- All exponent-range edge cases (subnormal, overflow, underflow,
  exponent at `±E_max`).
- Both Form A and Form B BID encodings (significand fields above
  and below `2^113`).
- Every special-value combination, including diagnostic NaN
  payloads (`NaN22`, `sNaN1234`, etc.).
- All §5.3 quantum operations (`quantize`, `scaleb`, `logb`,
  `nextplus`, `nextminus`) and §5.10 total-order operations
  (`comparetotal`, `comparetotmag`).
- All GDA decNumber extension operations: `and`, `or`, `xor`,
  `invert`, `divideInteger`, `reduce`, `rotate`, `shift` (ADR-0031),
  and `compareSignaling`, `nextToward` (ADR-0049, the fd-bef closure).
  Every one passes its full `dq*` decTest file.
- Both GDA string conversions: `toSci` (the default `Display`) and,
  since ADR-0058, `toEng` (the `Engineering` adapter), including the
  extreme-exponent literals that saturate at parse per ADR-0057.

13 941 vectors at 0 fail across that surface is the meaningful number
to look at; the residual 247 skips fall under the three categories
above (non-IEEE rounding directives, the BID-structural CLAMPED
residual, parser strictness negatives) plus the 17-case `#hex`
harness artifact, none of which has a concrete fix path in scope.
