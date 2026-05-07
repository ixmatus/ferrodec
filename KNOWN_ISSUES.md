# Known issues

This file enumerates the cases ferrodec deliberately or unintentionally
skips in the decTest conformance suite. The runner is
`tests/conformance.rs`; vectors live under `tests/vectors/`.

## Headline numbers

As of ferrodec 1.10.0:

| count | share | category |
|------:|------:|----------|
| 8 721 | 100 % | total cases |
| 8 592 | 98.5 % | pass |
|     0 |  0.0 % | fail |
|   129 |  1.5 % | skip |

The 0-fail floor is enforced by `tests/conformance.rs::dectest_conformance`:
any change that drops the pass count below 8 592 or raises the fail
count above 0 fails the build.

For context: 1.7.1 sat at 8 149 / 0 / 572 (93.4 % pass). 1.9.0 closed
four of the original six skip categories (NaN-with-payload literals,
hex `#` operands, the `class` op result format, and the `apply` op),
leaving the residual 130 skips as documented below.

## Skip taxonomy

The runner's `run_case` function checks five gates in order; each
skipped case bottoms out at the first one that matches.

### 1. Non-IEEE rounding directives — 101 cases (78 %)

decTest extends the IEEE 754 rounding set with two GDA-only modes:

- `half_down` — round to nearest, ties **toward zero**
- `05up` — round-zero-five-up (a banker's-rounding variant)

ferrodec implements only the five IEEE 754:2019 rounding-direction
attributes. Cases in directive blocks selecting the GDA modes are
skipped rather than coerced onto a kernel mode that doesn't match the
spec.

Per-file: `dqQuantize` 66, `dqFMA` 26, `dqAdd` 8, `dqDivide` 1.

**Will not fix.** ferrodec is positioned as IEEE 754 conformant, not
GDA conformant. Adding `half_down` / `05up` would expand the
`RoundingMode` surface beyond the spec's five-direction enumeration,
and embedded callers paying the kernel size already get every
direction the standard defines.

### 2. Bare `#` "null operand" sentinel — ≈ 13 cases (10 %)

decTest's `#` followed by hex chars encodes a raw 128-bit BID literal
(supported in 1.9.0). Bare `#` (no hex) is a different convention:
the "null test" that exercises operand-missing behavior. Each
affected case expects `NaN` + `Invalid_operation` from the operand-
parse failure. The runner's `parse_value` returns `None` for empty
hex which routes the case to `Outcome::Skip`; the conformance flag
machinery doesn't propagate a "parse-error → INVALID" signal up to
the comparator yet.

**To fix:** route `#` (empty hex) to `(Decimal128::NAN,
Status::INVALID)` in `parse_value` rather than `None`. ~10 minutes;
closes the entire null-test category.

### 3. Other parse failures — ≈ 15 cases

The residual after the categories above are the cases where some
operand fails to parse for reasons not captured by the patterns
above — typically unusual significand-exponent combinations near
the BID encoding boundary that the parser handles strictly, plus a
small number of NaN-with-payload literals where the payload exceeds
the 110-bit (`T_MASK`, ≈ 33 decimal digits) field.

## Per-file totals

```
dqAbs.decTest                    74 pass     0 fail     1 skip
dqAdd.decTest                  1002 pass     0 fail    10 skip
dqClass.decTest                  42 pass     0 fail     0 skip
dqCompare.decTest               657 pass     0 fail     2 skip
dqCompareTotal.decTest          611 pass     0 fail     2 skip
dqCompareTotalMag.decTest       611 pass     0 fail     2 skip
dqDivide.decTest                685 pass     0 fail     3 skip
dqFMA.decTest                  1421 pass     0 fail    30 skip
dqLogB.decTest                  108 pass     0 fail     1 skip
dqMax.decTest                   255 pass     0 fail     2 skip
dqMin.decTest                   245 pass     0 fail     2 skip
dqMinus.decTest                  43 pass     0 fail     0 skip
dqMultiply.decTest              471 pass     0 fail     2 skip
dqNextMinus.decTest              83 pass     0 fail     1 skip
dqNextPlus.decTest               83 pass     0 fail     1 skip
dqQuantize.decTest              620 pass     0 fail    66 skip
dqRemainderNear.decTest         528 pass     0 fail     2 skip
dqSameQuantum.decTest           333 pass     0 fail     0 skip
dqScaleB.decTest                202 pass     0 fail     0 skip
dqSubtract.decTest              518 pass     0 fail     2 skip
TOTAL: 8721 cases — 8592 pass, 0 fail, 129 skip
```

Reproduce with:

```sh
cargo test --features=transcendentals --test conformance -- --nocapture
```

## Closed in 1.9.0 / 1.10.0

For provenance, the categories that closed between 1.7.1 (572 skips)
and 1.10.0 (129 skips):

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
- **`remainder` op (1.10.0)** (1 case): new
  `Decimal128::rem_trunc` method implements the truncating-quotient
  remainder (C99 `fmod` / decTest `remainder`), distinct from the
  IEEE 754 §5.3.1 round-half-to-even `Decimal128::rem`.

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

8 592 vectors at 0 fail across that surface is the meaningful number
to look at; the residual 129 skips break down into a small set of
categories, each with a documented reason and (for most) a concrete
fix path.
