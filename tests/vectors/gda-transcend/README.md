# Vendored decTest transcendental vectors (p = 34 replay)

The four general (precision-driven) transcendental files from Mike
Cowlishaw's **General Decimal Arithmetic Testcases** suite, version
2.62: `exp`, `ln`, `log10`, `power`. Byte-identical copies of the
files vendored at `ferrodec-decimal/tests/vectors/` (compare the two
`SHA256SUMS`), duplicated here so the root crate's replay gate
(`tests/conformance_transcend.rs`, fd-4zo.8) owns its fixtures under
the workspace-root integrity test without a cross-crate path
dependency.

> Copyright (c) Mike Cowlishaw, 1981, 2010. All rights reserved.
> Parts copyright (c) IBM Corporation, 1981, 2008.
>
> The testcases are offered on an as-is basis. Achieving the same
> results as the tests here is not a guarantee that an implementation
> complies with any Standard or specification.

## Provenance

Registry entry:
[`cowlishaw-dectest`](../../../docs/references/cowlishaw-dectest.md)
holds the registry-level provenance (license, archive capture,
coverage-gap statement) for the suite across every vendored directory
(ADR-0052).

- Source: <https://speleotrove.com/decimal/dectest.zip>
- Suite version: 2.62
- Archive SHA-256:
  `b70a224cd52e82b7a8150aedac5efa2d0cb3941696fd829bdbe674f9f65c3926`
- Copied from `ferrodec-decimal/tests/vectors/` 2026-08-09 (fd-4zo.8);
  that directory's README records the original extraction.

## What the root gate replays

These files are precision-parameterized: `precision:` directives
change the working context mid-file, and the GDA crate honors every
block at its stated precision. `Decimal128` is fixed at p = 34, so
the root gate replays only the `precision: 34` blocks (all of them
`half_even`), and inside blocks whose exponent range is narrower
than Decimal128's it skips the rows whose expected conditions are
range effects (`Overflow`, `Underflow`, `Subnormal`, `Clamped`) —
those rows assert the *narrow* range's dispositions, which a wider
format correctly does not reproduce. Rows whose operands exceed 34
significant digits (GDA operands arrive unrounded at any width;
seven rows probe with 35-digit operands) are likewise skipped: they
name inputs no `Decimal128` caller can supply. Every skip lands in a
counted bucket; the pass counts are pinned exactly per file
(221 honored rows, all passing bit-exact with matching flags).
