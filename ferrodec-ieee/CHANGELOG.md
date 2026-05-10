# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.2] - 2026-05-10

### Added

- `decimal_digit_count_u128(n: u128) -> u32` — the decimal digit
  count for a u128 coefficient, lifted from three byte-identical
  copies in the sibling crates (`fma.rs` in both, plus
  Decimal64's `sqrt.rs`). `const fn`; returns 1 for `n == 0` per
  the GDA convention. 4 unit tests cover the boundary
  (zero / powers-of-ten / one-below / u128::MAX = 39 digits).

## [0.1.1] - 2026-05-10

### Added

- `should_round_up(rm, sign, last_kept_lsb, round_digit, sticky)
  -> bool` — the rounding-decision function shared across every
  precision's `round_and_pack_finite`. Lifted from four byte-
  identical copies in the three sibling crates' `ops/round.rs` /
  `ops/quantum.rs`; ferrodec (Decimal128) also adopts the shared
  function. Pure function of the five inputs; const fn; covered
  by five unit tests inside this crate.

## [0.1.0] - 2026-05-10

Initial release. Shared IEEE 754-2019 metadata types extracted from
the three ferrodec sibling crates (`ferrodec` Decimal128,
`ferrodec-decimal32`, `ferrodec-decimal64`) once three concrete
consumers existed, per the principle "stand alone first; resist
framework abstraction until 3 concrete uses exist."

### Added

- `Status` — IEEE 754-2019 §7 exception flags (`INVALID`,
  `DIV_BY_ZERO`, `OVERFLOW`, `UNDERFLOW`, `INEXACT`) packed in a
  single byte. Returned by every operation that can lose precision.
  Includes `is_ok` / `invalid` / `div_by_zero` / `overflow` /
  `underflow` / `inexact` predicates, `merge` (and `BitOr` /
  `BitOrAssign` for `|`), `from_bits_truncate` for raw construction,
  and `bits` for the underlying byte.
- `RoundingMode` — the five IEEE 754-2019 §4.3.3 rounding-direction
  attributes: `NearestEven` (default, banker's rounding),
  `NearestAway`, `TowardZero`, `TowardPositive`, `TowardNegative`.
- `IeeeClass` — the IEEE 754-2019 §5.7.2 `class(x)` enum with the
  ten standard classes (`SignalingNaN`, `QuietNaN`, `±Infinity`,
  `±Normal`, `±Subnormal`, `±Zero`).
- 7 unit tests covering the flag predicates, disjointness, merge
  behaviour, BitOrAssign, mask truncation, and the default
  rounding-mode.

The three sibling crates re-export these types via `pub use`, so
`ferrodec::Status` and `ferrodec_decimal32::Status` resolve to the
*same* concrete type — values flow between siblings without
conversion.
