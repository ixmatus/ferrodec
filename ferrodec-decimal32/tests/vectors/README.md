# Vendored decTest conformance vectors

These `.decTest` files come from Mike Cowlishaw's
**General Decimal Arithmetic Testcases** suite, version 2.62
(downloaded from <https://speleotrove.com/decimal/dectest.zip> on
2026-05-09).

> Copyright (c) Mike Cowlishaw, 1981, 2010. All rights reserved.
> Parts copyright (c) IBM Corporation, 1981, 2008.
>
> The testcases are offered on an as-is basis. Achieving the same
> results as the tests here is not a guarantee that an implementation
> complies with any Standard or specification.

## Provenance

Registry entry: [`cowlishaw-dectest`](../../../docs/references/cowlishaw-dectest.md) holds the registry-level
provenance (license, archive capture, coverage-gap statement) for this
suite across all four vendored directories (ADR-0052).

- Source: <https://speleotrove.com/decimal/dectest.zip>
- Archive size: 791 733 bytes
- Archive SHA-256:
  `b70a224cd52e82b7a8150aedac5efa2d0cb3941696fd829bdbe674f9f65c3926`
- Retrieved: 2026-05-09
- Files extracted: `dsBase.decTest`, `dsEncode.decTest` (the only two
  `ds*` files in the archive).

The local copies are unmodified; the conformance runner will parse them
at test time. The per-file SHA-256 of each committed file is pinned in
`SHA256SUMS` and enforced by `tests/vendored_integrity.rs`, which fails the
build on any byte drift or unpinned file (ADR-0042).

## Coverage scope

Unlike the IBM decTest distribution's coverage of Decimal64 and
Decimal128 — each of which gets ~22 dedicated `dd*` / `dq*` files
covering arithmetic, comparison, classification, quantize, scaleb,
nextUp / nextDown, encode, canonical form, and so on — the Decimal32
coverage is just two files:

- `dsBase.decTest` (909 cases, prefix `dsbas`): string-to-decimal-to-
  string conversion at precision 7 with `maxExponent: 96`,
  `minExponent: -95`, `rounding: half_even`. Covers parse, format,
  rounding boundary cases, NaN payloads, infinity, sign handling,
  and exponent edge conditions. The "left hand side" of these tests
  may include numbers that are not representable in decimal32; the
  test then exercises rounding and clamping per IEEE 754-2019 §3.5.
- `dsEncode.decTest` (268 cases, prefix `decs`): four-byte BID and DPD
  encoding bit-pattern tests. Operands and expecteds use the `#hex8`
  notation (eight hex digits = 32 bits). Runs only when the `dpd`
  Cargo feature is enabled, since DPD interchange is the IEEE 754
  byte-pattern format.

The remaining arithmetic surface (add, subtract, multiply, divide,
remainder, sqrt, FMA, comparison, classification, quantize, scaleb,
logb, nextUp / nextDown) has no Decimal32-specific vectors in the IBM
distribution. Those operations are verified by:

- Unit tests with hand-derived expected outcomes from IEEE 754-2019
  §5 worked examples.
- Property tests cross-checked against the `astro-float` arbitrary-
  precision library at sufficiently high working precision (≥ 80 bits)
  to bound the 7-digit Decimal32 result correctness.
- Differential tests against ferrodec's `Decimal128` operating on
  inputs converted from Decimal32, where applicable.

This shape mirrors the methodology in ferrodec's verification posture
(per `docs/decisions/0010-testing-strategy-after-six-agent-review.md`):
conformance vectors form one of several pillars rather than the sole
basis of correctness evidence. The IBM decTest scope for Decimal32
just happens to be narrower than for the larger precisions.

## Updating

To re-fetch the vectors, download the upstream archive from the URL
above, verify the SHA-256, extract `dsBase.decTest` and
`dsEncode.decTest` here, update the retrieval date and any new
case counts in this file, and regenerate the manifest with
`shasum -a 256 *.decTest > SHA256SUMS`.
