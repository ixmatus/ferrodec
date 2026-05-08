# Vendored decTest conformance vectors

These `.decTest` files come from Mike Cowlishaw's
**General Decimal Arithmetic Testcases** suite, version 2.62
(downloaded from <https://speleotrove.com/decimal/>).

> Copyright (c) Mike Cowlishaw, 1981, 2010. All rights reserved.
> Parts copyright (c) IBM Corporation, 1981, 2008.
>
> The testcases are offered on an as-is basis. Achieving the same
> results as the tests here is not a guarantee that an implementation
> complies with any Standard or specification.

The `dq*.decTest` files are the decimal128-specific variants (precision 34,
emax 6144, emin -6143).

The local copies are unmodified; the conformance runner in
`tests/conformance.rs` parses them at test time. New vectors should be
added by re-fetching the upstream archive and copying the relevant
`dq*.decTest` files here.

## DPD-encoding vectors

`dqEncode.decTest` and `dqCanonical.decTest` test the IEEE 754-2019
DPD bit pattern (`#hex32` operands and expecteds). They only run when
the `dpd` Cargo feature is enabled, since they require
`Decimal128::to_dpd_bytes` / `from_dpd_bytes`. Without the feature
the runner skips both files and the existing 8 622-pass / 0-fail /
99-skip baseline is preserved.
