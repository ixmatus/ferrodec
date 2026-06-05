# Vendored decTest conformance vectors

These `.decTest` files come from Mike Cowlishaw's
**General Decimal Arithmetic Testcases** suite, version 2.62.

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
- Retrieved: 2026-05-03 (initial set); 2026-06-04 (`dqCompareSig.decTest`, fd-bef.1)
- Files extracted: the `dq*.decTest` decimal128-specific variants (precision 34,
  emax 6144, emin -6143).

The local copies are unmodified. The per-file SHA-256 of every committed
`dq*.decTest` is pinned in `SHA256SUMS` (the standard `shasum -a 256` format) and
enforced by `tests/vendored_integrity.rs`, which fails the build on any byte
drift or unpinned file (ADR-0042); the conformance runner in
`tests/conformance.rs` parses the files at test time and pins per-file pass
counts (ADR-0010). To add or refresh vectors, re-fetch the archive above, verify
its SHA-256, copy the relevant `dq*.decTest` files here, then regenerate the
manifest (`shasum -a 256 *.decTest > SHA256SUMS`) and update the retrieved date.

## DPD-encoding vectors

`dqEncode.decTest` and `dqCanonical.decTest` test the IEEE 754-2019
DPD bit pattern (`#hex32` operands and expecteds). They only run when
the `dpd` Cargo feature is enabled, since they require
`Decimal128::to_dpd_bytes` / `from_dpd_bytes`. Without the feature
the runner skips both files and the existing 8 622-pass / 0-fail /
99-skip baseline is preserved.
