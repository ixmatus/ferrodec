# Vendored decTest conformance vectors (general suite)

These `.decTest` files come from Mike Cowlishaw's
**General Decimal Arithmetic Testcases** suite, version 2.62
(downloaded from <https://speleotrove.com/decimal/dectest.zip>).

> Copyright (c) Mike Cowlishaw, 1981, 2010. All rights reserved.
> Parts copyright (c) IBM Corporation, 1981, 2008.
>
> The testcases are offered on an as-is basis. Achieving the same
> results as the tests here is not a guarantee that an implementation
> complies with any Standard or specification.

Unlike the `dq*` / `dd*` / `ds*` files vendored at the workspace root (which
pin the three fixed IEEE 754-2019 interchange widths), these are the **general**
files: each one sets its own `precision`, `maxExponent`, `minExponent`,
`rounding`, and `clamp` through in-file directives, which is exactly the
arbitrary-precision contract `ferrodec-decimal` implements. The conformance
runner in `tests/conformance.rs` parses the directives at test time and builds a
`ferrodec_decimal::Context` per file (and per directive change within a file).

The copies are unmodified, including the upstream CRLF line endings and the
copyright headers. New vectors are added by re-fetching the upstream archive and
copying the relevant general files here, then pinning the per-file pass count in
`tests/conformance.rs` (the record-then-pin discipline of ADR-0010).

## What is vendored

The general files for the operations `ferrodec-decimal` implements:

`add subtract multiply divide divideint remainder remaindernear fma squareroot
quantize tointegral tointegralx reduce plus minus abs compare comparetotal max
min copyabs copynegate copysign`, plus `base` (the number-syntax parse and
`toSci` / `toEng` / `apply` surface) and `clamp` (the dedicated exponent-clamping
test, all `apply`).

## What is not vendored, and why

- **Transcendentals** (`exp ln log10 power powersqrt`) and the mixed-operation
  flag tests that depend on them (`rounding inexact`) arrive with the
  transcendental phase, not before.
- **Out of the arbitrary-precision surface**: logical `and or xor invert`,
  positioning `rotate shift`, `scaleb logb`, `nextplus nextminus nexttoward`,
  `class samequantum comparesig comparetotmag maxmag minmag`, plain `copy`, and
  `trim`. These are GDA operations the crate does not expose.
- **Format-specific**: the fixed-width `dd* dq* ds*` files and the
  `decSingle decDouble decQuad` encoding files belong to the fixed-format crates.
- **Generated / driver files**: `testall` (a `dectest:` include driver the line
  parser does not follow), `randoms`, `randombound32`.

The full skip taxonomy is recorded in the conformance ADR.
