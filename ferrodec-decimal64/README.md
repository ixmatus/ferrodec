# ferrodec-decimal64

IEEE 754-2019 Decimal64 in pure Rust. `no_std` capable, targeted at
embedded use, with the verification posture established by the
[`ferrodec`](https://crates.io/crates/ferrodec) crate (Decimal128).

This crate is in early development. The currently exposed surface is
only the type wrapper; arithmetic, classification, parse, format, and
verification land in subsequent commits. The plan archived at
[`docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`](../docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md)
describes the development arc.

## IEEE 754-2019 §3.5 Decimal64 parameters

| Parameter | Value |
| --- | --- |
| Storage width | 64 bits |
| Coefficient precision | 16 decimal digits (≈ 53.15 bits) |
| Exponent range (unbiased) | -383 to 384 |
| Exponent bias | 398 |
| Biased exponent range | 0 to 767 |
| Maximum normal magnitude | 9.999999999999999 × 10³⁸⁴ |
| Minimum positive normal magnitude | 1 × 10⁻³⁸³ |
| Encoding (arithmetic) | BID (binary integer significand) |
| Encoding (interchange, planned) | DPD (densely packed decimal) |

## Companion crates

- [`ferrodec`](https://crates.io/crates/ferrodec): Decimal128, the sibling at v1.x.
- [`ferrodec-decimal32`](https://crates.io/crates/ferrodec-decimal32): Decimal32, the sibling at v1.x.

## License

MIT OR Apache-2.0, matching the workspace.
