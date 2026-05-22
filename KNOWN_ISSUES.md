# Known issues

This file enumerates the cases ferrodec deliberately or unintentionally
skips in the decTest conformance suite. The runner is
`tests/conformance.rs`; vectors live under `tests/vectors/`.

## Headline numbers

As of ferrodec 2.1.0:

| count |  share | category |
|------:|-------:|----------|
| 10 964 | 100.0 % | total cases |
| 10 865 |  99.1 % | pass |
|      0 |   0.0 % | fail |
|     99 |   0.9 % | skip |

The 0-fail floor is enforced by `tests/conformance.rs::dectest_conformance`,
which pins the expected pass count per file under the ADR-0010
discipline (the authoritative table lives at
`tests/conformance.rs::expected_per_file`). Any change that drops the
pass count below the per-file pin or raises the fail count above 0
fails the build.

All 99 residual skips fall under a single category — non-IEEE
rounding directives — that ferrodec deliberately doesn't support.
Every other operation, encoding, and special-value combination in
the suite passes.

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
total from 8 721 to 10 964 vectors. The residual 99 skips still fall
under the single will-not-fix non-IEEE rounding category.

## Skip taxonomy

### 1. Non-IEEE rounding directives — 99 cases (100 %)

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


## Per-file totals

```
dqAbs.decTest                    75 pass     0 fail     0 skip
dqAdd.decTest                  1004 pass     0 fail     8 skip
dqAnd.decTest                   357 pass     0 fail     0 skip
dqCanonical.decTest               0 pass     0 fail     0 skip
dqClass.decTest                  42 pass     0 fail     0 skip
dqCompare.decTest               659 pass     0 fail     0 skip
dqCompareTotal.decTest          613 pass     0 fail     0 skip
dqCompareTotalMag.decTest       613 pass     0 fail     0 skip
dqDivide.decTest                687 pass     0 fail     1 skip
dqDivideInt.decTest             374 pass     0 fail     0 skip
dqEncode.decTest                  0 pass     0 fail     0 skip
dqFMA.decTest                  1425 pass     0 fail    26 skip
dqInvert.decTest                193 pass     0 fail     0 skip
dqLogB.decTest                  109 pass     0 fail     0 skip
dqMax.decTest                   257 pass     0 fail     0 skip
dqMin.decTest                   247 pass     0 fail     0 skip
dqMinus.decTest                  43 pass     0 fail     0 skip
dqMultiply.decTest              473 pass     0 fail     0 skip
dqNextMinus.decTest              84 pass     0 fail     0 skip
dqNextPlus.decTest               84 pass     0 fail     0 skip
dqOr.decTest                    341 pass     0 fail     0 skip
dqQuantize.decTest              622 pass     0 fail    64 skip
dqReduce.decTest                134 pass     0 fail     0 skip
dqRemainderNear.decTest         530 pass     0 fail     0 skip
dqRotate.decTest                248 pass     0 fail     0 skip
dqSameQuantum.decTest           333 pass     0 fail     0 skip
dqScaleB.decTest                202 pass     0 fail     0 skip
dqShift.decTest                 248 pass     0 fail     0 skip
dqSubtract.decTest              520 pass     0 fail     0 skip
dqXor.decTest                   348 pass     0 fail     0 skip
TOTAL: 10964 cases — 10865 pass, 0 fail, 99 skip
```

Default-feature build (no `dpd`); `dqCanonical` and `dqEncode` route
to skip without the `dpd` feature and pin to 90 / 368 pass when it
is enabled (see `tests/conformance.rs::expected_per_file`).

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
- All GDA decNumber extension operations wired in the 2.0 cycle
  (ADR-0031): `and`, `or`, `xor`, `invert`, `divideInteger`,
  `reduce`, `rotate`, `shift`. Every one passes its full `dq*`
  decTest file on first run.

10 865 vectors at 0 fail across that surface is the meaningful number
to look at; the residual 99 skips fall under a single will-not-fix
category (non-IEEE rounding directives), with no concrete fix path
in scope.
