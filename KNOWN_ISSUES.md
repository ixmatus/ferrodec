# Known issues

This file enumerates the cases ferrodec deliberately or unintentionally
skips in the decTest conformance suite. The runner is
`tests/conformance.rs`; vectors live under `tests/vectors/`.

## Headline numbers

As of ferrodec 1.7.1:

| count | share | category |
|------:|------:|----------|
| 8 721 | 100 % | total cases |
| 8 149 | 93.4 % | pass |
|     0 |  0.0 % | fail |
|   572 |  6.6 % | skip |

The 0-fail floor is enforced by `tests/conformance.rs::dectest_conformance`:
any change that drops the pass count below 8 149 or raises the fail
count above 0 fails the build.

## Skip taxonomy

The runner's `run_case` function checks five gates in order; each
skipped case bottoms out at the first one that matches.

### 1. NaN-with-payload operand literals — ≈ 357 cases (62 %)

decTest carries diagnostic NaN payloads in the literal itself
(`NaN22`, `-NaN22`, `sNaN33`). ferrodec's `Decimal128::parse_str`
recognises only the canonical `NaN` / `sNaN` / `-NaN` tokens; anything
with a trailing payload fails to parse, the runner's `invoke()`
returns `None`, and the case is skipped.

Affected files: every dq file except `dqMinus.decTest` and the
quantum-suite ones (`dqSameQuantum`, `dqScaleB`) which don't include
payload tests.

**To fix:** extend the parser to accept and round-trip diagnostic
payloads through the BID significand field. The IEEE encoding
reserves the trailing significand bits for exactly this purpose,
so the storage is already there — only the parse / display path
would change. ~1 day of work; medium-impact ergonomics gain for
non-embedded users, no benefit on the embedded floor.

### 2. Non-IEEE rounding directives — 101 cases (18 %)

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

### 3. `class` operation result format — 42 cases (7 %)

`dqClass.decTest` is 100 % skipped. The runner dispatches the `class`
op (which corresponds to `Decimal128::classify`) but the comparator
expects the result to be a `Decimal128` value, not the GDA class-name
string (`+Normal`, `-Subnormal`, `+Infinity`, etc.). All 42 cases
parse cleanly and the value side runs; the runner just doesn't
compare class names.

**To fix:** add a class-name comparator branch to the conformance
runner. The mapping from `core::num::FpCategory` plus sign to the
nine GDA class names is mechanical. ~1 hour of work, low impact
(the `classify` API is independently unit-tested).

### 4. Hex-encoded literal operands (`#`) — 28 cases (5 %)

decTest uses a leading `#` for raw bit-pattern operands (`#FFFE…`).
The format is per-format-specific (decimal128 vs decimal64), so when
a `dq*.decTest` file uses it, the bytes are a Decimal128 BID
encoding. ferrodec's `parse_str` doesn't accept this syntax — the
runner explicitly skips on `trimmed.starts_with('#')`.

**To fix:** decode the `#` prefix as `from_bits` of the parsed hex
value. Two-line change in the runner's `parse_value` helper. ~30
minutes; minor coverage gain (28 cases).

### 5. Unimplemented ops — 5 cases (1 %)

Two operations appear in the conformance suite that ferrodec doesn't
dispatch:

- `apply` (4 cases — 2 in `dqAdd`, 2 in `dqFMA`). The op encodes a
  value through the format's quantum logic and emits the canonical
  decimal string. Equivalent to `Decimal128::canonicalize` followed
  by `Display`, so the building blocks already exist.
- `remainder` (1 case in `dqRemainderNear`). The IEEE
  remainder-to-nearest operation, distinct from `remainderNear` /
  `Decimal128::rem`. ferrodec implements `rem` (IEEE 754 §5.3.1
  remainder, exact when terminating); `remainder` is the round-tie-
  to-even variant.

**To fix:** route `apply` to `canonicalize` + `Display`, and add
`remainder` as a sibling of `rem` returning a rounded result. ~1
day for both, mostly comparator wiring; no public API additions
needed for `apply`.

### 6. Other parse failures — ≈ 39 cases (7 %)

The residual after the categories above are the cases where some
operand fails to parse for reasons not captured by the patterns above
— typically unusual significand-exponent combinations near the BID
encoding boundary that the parser handles strictly. These are case-
by-case; documenting them all would require triage work that hasn't
landed yet.

## Per-file totals

```
dqAbs.decTest                    70 pass     0 fail     5 skip
dqAdd.decTest                   976 pass     0 fail    36 skip
dqClass.decTest                   0 pass     0 fail    42 skip
dqCompare.decTest               637 pass     0 fail    22 skip
dqCompareTotal.decTest          579 pass     0 fail    34 skip
dqCompareTotalMag.decTest       579 pass     0 fail    34 skip
dqDivide.decTest                653 pass     0 fail    35 skip
dqFMA.decTest                  1352 pass     0 fail    99 skip
dqLogB.decTest                  103 pass     0 fail     6 skip
dqMax.decTest                   236 pass     0 fail    21 skip
dqMin.decTest                   226 pass     0 fail    21 skip
dqMinus.decTest                  35 pass     0 fail     8 skip
dqMultiply.decTest              437 pass     0 fail    36 skip
dqNextMinus.decTest              79 pass     0 fail     5 skip
dqNextPlus.decTest               79 pass     0 fail     5 skip
dqQuantize.decTest              588 pass     0 fail    98 skip
dqRemainderNear.decTest         517 pass     0 fail    13 skip
dqSameQuantum.decTest           323 pass     0 fail    10 skip
dqScaleB.decTest                182 pass     0 fail    20 skip
dqSubtract.decTest              498 pass     0 fail    22 skip
TOTAL: 8721 cases — 8149 pass, 0 fail, 572 skip
```

Reproduce with:

```sh
cargo test --features=transcendentals --test conformance -- --nocapture
```

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
- Every special-value combination not requiring a NaN diagnostic
  payload (signed zero, signed infinity, canonical quiet / signaling
  NaN).
- All §5.3 quantum operations (`quantize`, `scaleb`, `logb`,
  `nextplus`, `nextminus`) and §5.10 total-order operations
  (`comparetotal`, `comparetotmag`).

8 149 vectors at 0 fail across that surface is the meaningful number
to look at; the 572 skips break down into a small set of categories,
each with a documented reason and (for most) a concrete fix path.
