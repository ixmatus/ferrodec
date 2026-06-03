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

## Provenance

- Source: <https://speleotrove.com/decimal/dectest.zip>
- Suite version: 2.62
- Archive size: 791 733 bytes
- Archive SHA-256:
  `b70a224cd52e82b7a8150aedac5efa2d0cb3941696fd829bdbe674f9f65c3926`
- Retrieved: 2026-06-02 (the 0.2.0 numerical set and the 0.3.0
  miscellaneous-operation set were extracted from the same archive)
- Files extracted: the **general** (precision-driven) files, listed under
  "What is vendored" below.

Unlike the `dq*` / `dd*` / `ds*` files vendored at the workspace root (which
pin the three fixed IEEE 754-2019 interchange widths), these general files each
set their own `precision`, `maxExponent`, `minExponent`, `rounding`, and `clamp`
through in-file directives, which is exactly the arbitrary-precision contract
`ferrodec-decimal` implements. The conformance runner in `tests/conformance.rs`
parses the directives at test time and builds a `ferrodec_decimal::Context` per
file (and per directive change within a file).

The copies are unmodified, including the upstream CRLF line endings and the
copyright headers. The per-file SHA-256 of every committed file is pinned in
`SHA256SUMS` (the standard `shasum -a 256` format) and enforced by
`tests/vendored_integrity.rs`, which fails the build on any byte drift or
unpinned file (ADR-0042). New vectors are added by re-fetching the upstream
archive, verifying its SHA-256, copying the relevant general files here, pinning
the per-file pass count in `tests/conformance.rs` (the record-then-pin
discipline of ADR-0010), and regenerating the manifest
(`shasum -a 256 *.decTest > SHA256SUMS`).

## What is vendored

The general files for the operations `ferrodec-decimal` implements:

`add subtract multiply divide divideint remainder remaindernear fma squareroot
quantize tointegral tointegralx reduce plus minus abs compare comparetotal max
min copyabs copynegate copysign`, plus `base` (the number-syntax parse and
`toSci` / `toEng` / `apply` surface) and `clamp` (the dedicated exponent-clamping
test, all `apply`).

The transcendental files `exp ln log10 power powersqrt` (the four numerical
transcendentals plus `power(x, 0.5)`), and the mixed-operation flag tests
`rounding` and `inexact` that exercise them, are also vendored. `power` is
compared within a one-ulp band rather than cohort-exact, since the reference is
only "almost always" correctly rounded while this crate's `power` is correctly
rounded by construction.

The miscellaneous operation files (ADR-0041): logical `and or xor invert`,
positioning `rotate shift`, exponent `scaleb logb`, next-value `nextplus
nextminus nexttoward`, `class`, `samequantum`, `comparesig`, `comparetotmag`,
`maxmag minmag`, and plain `copy`. `class` is dispatched as a string operation
like `toSci`.

## What is not vendored, and why

- **Out of the specification surface**: `trim`, a decNumber library convenience
  that is not a General Decimal Arithmetic operation and has no general decTest
  file.
- **Format-specific**: the fixed-width `dd* dq* ds*` files and the
  `decSingle decDouble decQuad` encoding files belong to the fixed-format crates.
- **Generated / driver files**: `testall` (a `dectest:` include driver the line
  parser does not follow), `randoms`, `randombound32`.

The full skip taxonomy is recorded in the conformance ADR.
