# Vendored decTest conformance vectors

These `.decTest` files come from Mike Cowlishaw's
**General Decimal Arithmetic Testcases** suite, version 2.62
(downloaded from <https://speleotrove.com/decimal/dectest.zip> on
2026-05-10).

> Copyright (c) Mike Cowlishaw, 1981, 2010. All rights reserved.
> Parts copyright (c) IBM Corporation, 1981, 2008.
>
> The testcases are offered on an as-is basis. Achieving the same
> results as the tests here is not a guarantee that an implementation
> complies with any Standard or specification.

## Provenance

- Source: <https://speleotrove.com/decimal/dectest.zip>
- Archive size: 791 733 bytes
- Archive SHA-256:
  `b70a224cd52e82b7a8150aedac5efa2d0cb3941696fd829bdbe674f9f65c3926`
- Retrieved: 2026-05-10
- Files extracted: every `dd*.decTest` file in the archive (42 files,
  17 901 lines). Unlike `ds*` (Decimal32, only 2 files) and `dq*`
  (Decimal128, ferrodec uses 22 of the available 42), the Decimal64
  surface in the IBM distribution is the most complete: it covers
  every operation in IEEE 754-2019 §5 plus a number of GDA-specific
  ones (`and`, `or`, `xor`, `rotate`, `shift`, `invert`, `copy*`)
  that are outside the IEEE 754 surface and will skip in the
  conformance harness.

The local copies are unmodified; the conformance runner will parse
them at test time. The per-file SHA-256 of each committed file is pinned in
`SHA256SUMS` and enforced by `tests/vendored_integrity.rs`, which fails the
build on any byte drift or unpinned file (ADR-0042).

## Coverage scope

The `dd*` files exercise the §5 mandatory and §9 recommended
operations at decimal64 precision (16 digits, exponent range
`10⁻³⁸³..=10⁺³⁸⁴`). Initial harness wiring will dispatch
`tosci` / `apply` (parse + format round-trip) and add per-op
arms as each arithmetic operation lands; ops not yet wired
report as Skip. The asymmetric per-file expectation table (per
ferrodec ADR-0010) starts at zero and rises by the count each new
dispatch arm passes.

## Updating

To re-fetch the vectors, download the upstream archive from the URL
above, verify the SHA-256, extract the dd*.decTest files here, update
the retrieval date in this file, and regenerate the manifest with
`shasum -a 256 *.decTest > SHA256SUMS`.
