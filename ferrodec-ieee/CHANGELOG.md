# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.4] - 2026-05-17

### Added

- `IeeeDecodedClass` — the decoded BID bit-pattern form (sign,
  biased exponent, coefficient, or NaN payload) that a sibling's
  `classify_bits` produces and that the shared arithmetic and
  transcendental kernels consume. Distinct from `IeeeClass` (the
  §5.7.2 ten-class observation): this enum carries the reconstructed
  numeric components, not a classification label. The shape is
  precision-agnostic (`u128` coefficient / payload, `u32` biased
  exponent), so all three siblings and the forthcoming shared
  transcendental crate see one definition rather than a per-crate
  copy. Purely additive: `ferrodec` now aliases its private
  `bid::Class` to this type via a `pub(crate) use`, so existing
  call sites and behaviour are unchanged.

Introduced to underpin the `ferrodec-transcend` extraction
(faithful Extended-kernel transcendentals shared across the decimal
siblings).

## [0.1.3] - 2026-05-11

### Added

- `Status::CLAMPED` — the IEEE 754-2019 §7.4 informational
  `Clamped` condition, raised when the result's preferred quantum is
  outside the format's representable range and gets clamped to the
  nearest representable quantum. The conformance harness already
  filters this token as informational (`decode_conditions` in
  `ferrodec-test-support`), so this addition is purely additive on
  the consumer side: callers may now emit `CLAMPED` where the spec
  requires it without changing existing conformance expectations.
  Accompanying `clamped()` predicate and updated `from_bits_truncate`
  mask. Two new unit tests cover the flag (`clamped_flag_round_trips`
  plus `clamped` line added to `ok_is_zero`); the
  `each_flag_is_disjoint` table extends to six entries.

Driven by the Phase 1 findings on the decimal64 correctness slice
(`docs/decisions/plans/2026-05-11-decimal64-correctness-findings.md`),
which named multiple H tier cases across addsub, mul, div, rem,
fma where the spec marks the result `Clamped` and the current ops
silently omit the flag. Closure of those findings ships in
ferrodec-decimal64 1.4.0.

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
