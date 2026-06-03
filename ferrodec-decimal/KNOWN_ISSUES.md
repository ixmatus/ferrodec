# Known issues (ferrodec-decimal)

This file records `ferrodec-decimal`'s known issues, chiefly the cases the
general decTest conformance suite deliberately skips. The runner is
`tests/conformance.rs`; the vendored vectors live under `tests/vectors/`; the
authoritative per-file pass counts are pinned in
`tests/conformance.rs::expected_per_file` under the ADR-0010 discipline, so the
numbers below are a rendered summary, not the source of truth. See ADR-0039 for
the decision and rationale.

## Known defects

None outstanding in the conformance surface. The previously recorded
to-engineering-string gap (`fd-7la`) is resolved: [`Decimal::to_eng_string`]
now implements the General Decimal Arithmetic to-engineering-string rule (the
shown exponent a multiple of three, one to three digits before the point), and
the runner exercises the 174 `toEng` cases in `base.decTest`.

## Headline numbers

| count |   share | category |
|------:|--------:|----------|
| 16549 | 100.0 % | total cases |
| 16479 |  99.6 % | pass |
|     0 |   0.0 % | fail |
|    70 |   0.4 % | skip |

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
fma.decTest           2612 pass     0 fail      0 skip
max.decTest            328 pass     0 fail      0 skip
min.decTest            317 pass     0 fail      0 skip
minus.decTest          113 pass     0 fail      0 skip
multiply.decTest       521 pass     0 fail      0 skip
plus.decTest           122 pass     0 fail      0 skip
quantize.decTest       742 pass     0 fail     33 skip
reduce.decTest         168 pass     0 fail      0 skip
remainder.decTest      517 pass     0 fail      0 skip
remaindernear.decTest  446 pass     0 fail      0 skip
squareroot.decTest    3586 pass     0 fail      0 skip
subtract.decTest       681 pass     0 fail      0 skip
tointegral.decTest     168 pass     0 fail      0 skip
tointegralx.decTest    180 pass     0 fail      0 skip
TOTAL: 16549 cases — 16479 pass, 0 fail, 70 skip
```

Reproduce with:

```sh
cargo test -p ferrodec-decimal --test conformance -- --nocapture
```

## Files not vendored

The transcendental files (`exp`, `ln`, `log10`, `power`, `powersqrt`) and the
mixed-operation flag tests that depend on them (`rounding`, `inexact`) arrive
with the transcendental phase. The operations outside this crate's surface are
not vendored: logical `and` / `or` / `xor` / `invert`, `rotate` / `shift`,
`scaleb` / `logb`, `nextplus` / `nextminus` / `nexttoward`, `class`,
`samequantum`, `comparesig`, `comparetotmag`, `maxmag` / `minmag`, plain `copy`,
and `trim`. Nor are the format-specific encoding files (`decSingle` /
`decDouble` / `decQuad`, and the `dd*` / `dq*` / `ds*` widths) or the generated
driver files (`testall`, `randoms`, `randombound32`).
