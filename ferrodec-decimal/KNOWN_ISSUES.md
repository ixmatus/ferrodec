# Known issues (ferrodec-decimal)

This file records `ferrodec-decimal`'s known issues, chiefly the cases the
general decTest conformance suite deliberately skips. The runner is
`tests/conformance.rs`; the vendored vectors live under `tests/vectors/`; the
authoritative per-file pass counts are pinned in
`tests/conformance.rs::expected_per_file` under the ADR-0010 discipline, so the
numbers below are a rendered summary, not the source of truth. See ADR-0039 for
the runner and ADR-0040 for the transcendentals it now also exercises.

## Known defects

None outstanding in the conformance surface. The previously recorded
to-engineering-string gap (`fd-7la`) is resolved: [`Decimal::to_eng_string`]
implements the General Decimal Arithmetic to-engineering-string rule, and the
runner exercises the 174 `toEng` cases in `base.decTest`. The four
transcendentals (`exp`, `ln`, `log10`, `power`) are implemented and conformant;
`power` is compared within a one-ulp band, since it is correctly rounded by
construction while the reference is only almost always correctly rounded.

## Headline numbers

| count |   share | category |
|------:|--------:|----------|
| 23037 | 100.0 % | total cases |
| 22938 |  99.6 % | pass |
|     0 |   0.0 % | fail |
|    99 |   0.4 % | skip |

The 0-fail floor and the per-file pass pins are enforced by
`tests/conformance.rs::dectest_conformance`. Any change that moves a file's pass
count off its pin, or raises the fail count above zero, fails the build.

## Skip taxonomy

### 1. Exponent beyond i32 — 16 cases

All in `base.decTest`. These `toSci` cases use extreme exponents such as
`0.9e9999999999`, whose magnitude exceeds the `i32` exponent this crate stores.
That bound is deliberate: a `Decimal` carries an `i32` exponent, so an input
needing a wider exponent, even transiently before the context rounds it to an
infinity or a subnormal zero, is outside the representable domain. `parse_str`
reports `ParseDecimalError::ExponentOverflow` for these, distinct from a
conversion-syntax error, and the runner skips them on that signal.

### 2. Fixed-width encoding literals — 54 cases

`clamp.decTest` (21) and `quantize.decTest` (33). These use `#hex` or `NN#`
notation, a decimal32/64/128 bit pattern or a value tagged with the format it
was rounded in. An arbitrary precision value has no fixed-width encoding to
reproduce, so a case whose operand or expected uses the notation is skipped. A
bare `#` operand is not in this category: it is the null sentinel, and the
runner lets it fall through to `(NaN, Invalid_operation)`.

### 3. Reference context restrictions — 18 cases

`exp` (4), `ln` (4), `log10` (4), `power` (6). These expect `Invalid_context`
because the in-file context sets a precision or exponent bound beyond
decNumber's internal limits (a precision of 100000000, a maxExponent of
1000000). That is an implementation ceiling of the reference, not a
spec-arithmetic result; this crate places no such ceiling and computes the
operation, so the cases are skipped. The suite itself flags them as skippable by
harnesses that do not model the restriction.

### 4. Reference operand range (`DEC_MAX_MATH`) — 4 cases

`power` (the "operand range violations" section). These expect
`Invalid_operation` solely because an operand's adjusted exponent leaves
decNumber's `DEC_MAX_MATH` range (`[-1999997, 999999]`). This crate computes the
mathematically correct result for any operand within its `i32` exponent, so a
case whose only reason for the expected `Invalid_operation` is that range is
skipped; the surrounding in-range cases that return a real result still run.

### 5. Unimplemented `rescale` operation — 7 cases

`inexact.decTest`. `rescale` is the superseded form of `quantize` (a power-of-ten
second operand rather than an exemplar); this crate exposes `quantize` only, so
the `rescale` cases are skipped.

## Per-file totals

```
abs.decTest             89 pass     0 fail      0 skip
add.decTest           2100 pass     0 fail      0 skip
base.decTest          1152 pass     0 fail     16 skip
clamp.decTest          111 pass     0 fail     21 skip
compare.decTest        639 pass     0 fail      0 skip
comparetotal.decTest   670 pass     0 fail      0 skip
copyabs.decTest         43 pass     0 fail      0 skip
copynegate.decTest      43 pass     0 fail      0 skip
copysign.decTest       111 pass     0 fail      0 skip
divide.decTest         631 pass     0 fail      0 skip
divideint.decTest      389 pass     0 fail      0 skip
exp.decTest            436 pass     0 fail      4 skip
fma.decTest           2612 pass     0 fail      0 skip
inexact.decTest        145 pass     0 fail      7 skip
ln.decTest             410 pass     0 fail      4 skip
log10.decTest          385 pass     0 fail      4 skip
max.decTest            328 pass     0 fail      0 skip
min.decTest            317 pass     0 fail      0 skip
minus.decTest          113 pass     0 fail      0 skip
multiply.decTest       521 pass     0 fail      0 skip
plus.decTest           122 pass     0 fail      0 skip
power.decTest         1197 pass     0 fail     10 skip
powersqrt.decTest     2856 pass     0 fail      0 skip
quantize.decTest       742 pass     0 fail     33 skip
reduce.decTest         168 pass     0 fail      0 skip
remainder.decTest      517 pass     0 fail      0 skip
remaindernear.decTest  446 pass     0 fail      0 skip
rounding.decTest      1030 pass     0 fail      0 skip
squareroot.decTest    3586 pass     0 fail      0 skip
subtract.decTest       681 pass     0 fail      0 skip
tointegral.decTest     168 pass     0 fail      0 skip
tointegralx.decTest    180 pass     0 fail      0 skip
TOTAL: 23037 cases — 22938 pass, 0 fail, 99 skip
```

Reproduce with:

```sh
cargo test -p ferrodec-decimal --test conformance -- --nocapture
```

## Files not vendored

The operations outside this crate's surface are
not vendored: logical `and` / `or` / `xor` / `invert`, `rotate` / `shift`,
`scaleb` / `logb`, `nextplus` / `nextminus` / `nexttoward`, `class`,
`samequantum`, `comparesig`, `comparetotmag`, `maxmag` / `minmag`, plain `copy`,
and `trim`. Nor are the format-specific encoding files (`decSingle` /
`decDouble` / `decQuad`, and the `dd*` / `dq*` / `ds*` widths) or the generated
driver files (`testall`, `randoms`, `randombound32`).
