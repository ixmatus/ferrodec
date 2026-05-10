# ferrodec-decimal32

IEEE 754-2019 Decimal32 in pure Rust. `no_std` capable, targeted at
embedded use, with the verification posture established by the
[`ferrodec`](https://crates.io/crates/ferrodec) crate (Decimal128).

This crate is in early development. The currently exposed surface is
only the type wrapper; arithmetic, classification, parse, format, and
verification land in subsequent commits. The plan archived at
[`docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`](../docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md)
describes the development arc.

## IEEE 754-2019 §3.5 Decimal32 parameters

| Parameter | Value |
| --- | --- |
| Storage width | 32 bits |
| Coefficient precision | 7 decimal digits (≈ 23.25 bits) |
| Exponent range (unbiased) | -101 to 96 |
| Exponent bias | 101 |
| Biased exponent range | 0 to 191 |
| Maximum normal magnitude | 9.999999 × 10⁹⁶ |
| Minimum positive normal magnitude | 1 × 10⁻⁹⁵ |
| Encoding (arithmetic) | BID (binary integer significand) |
| Encoding (interchange, planned) | DPD (densely packed decimal) |

## Companion crates

- [`ferrodec`](https://crates.io/crates/ferrodec): Decimal128, the sibling at v1.x.
- `ferrodec-decimal64`: Decimal64, in development.

## License

MIT OR Apache-2.0, matching the workspace.
