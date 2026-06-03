# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-06-03

### Added

- The General Decimal Arithmetic miscellaneous, comparison, and predicate
  surface, completing the specification's operation set. Logical `and`, `or`,
  `xor`, `invert`; positioning `shift`, `rotate`; exponent `scaleb`, `logb`;
  next-value `next_plus`, `next_minus`, `next_toward`; comparison
  `compare_signal`, `compare_total_mag`, `max_magnitude`, `min_magnitude`,
  `same_quantum`; and `class`, plain `copy`, `is_normal`, `is_subnormal`,
  `is_canonical`, `is_signed`, `radix`. The logical operations reject every NaN
  (a quiet NaN does not propagate); `next_toward` signals like an arithmetic
  step while `next_plus` / `next_minus` are silent but for a signaling NaN.
  See ADR-0041.
- Convenience integer constructors `Decimal::from_i64` / `from_i128` /
  `from_u64` / `from_u128` (exact, at exponent zero).
- `TryFrom<f64>` and `TryFrom<f32>`, behind the new `binary-float` feature. The
  conversion is lossless: the float's exact value as a decimal, never rounded,
  so `0.1f64` yields its full binary value rather than the shortest decimal.
  This diverges deliberately from the fixed-width siblings, which must round.
  NaN and the infinities are rejected with `DecimalFromFloatError`.
- Eighteen further general decTest vector files (`and`, `or`, `xor`, `invert`,
  `shift`, `rotate`, `scaleb`, `logb`, `nextplus`, `nextminus`, `nexttoward`,
  `comparesig`, `comparetotmag`, `maxmag`, `minmag`, `samequantum`, `class`,
  `copy`), pinned at 27492 pass, 0 fail, 99 skip across 50 files. The libmpdec
  differential is extended with the new value-returning operations.

## [0.2.0] - 2026-06-02

### Added

- The four numerical transcendentals of the General Decimal Arithmetic
  specification, completing the operation surface: `Decimal::exp`,
  `Decimal::ln`, `Decimal::log10`, and `Decimal::power`. `exp` / `ln` / `log10`
  are correctly rounded half-even (like `squareRoot`); `power` is correctly
  rounded with the context's rounding mode, with the full IEEE 754-2019 section
  9.2.1 special-case table and an exact integer-exponent fast path. They are
  built on a private variable-precision float and a bounded Ziv strategy, with
  `ln 2` / `ln 10` computed on demand by an `atanh` series (no stored table).
  See ADR-0040.
- `Decimal::to_eng_string`, the to-engineering-string rendering (the shown
  exponent a multiple of three, one to three digits before the point), behind
  the `fmt` feature. See ADR-0039.
- The static general decTest conformance suite, vendored and wired as an
  independent cross-check of the whole operation surface (the four
  transcendentals included), standing at 22938 pass, 0 fail, 99 skip across 32
  files. The randomized libmpdec differential is extended to the four
  transcendentals. See ADR-0039 and ADR-0040.

### Fixed

- Sign-of-zero and zero-exponent clamping in `max` / `min` / `reduce` and on
  division by an infinity and subnormal round-to-zero, found by the decTest
  suite (these were outside the randomized differential's distribution). See
  ADR-0039.
- `power(1, y)` for a non-integer or infinite `y` no longer rounds the (exact)
  one up to two under a round-away rounding mode.

## [0.1.0]

### Added

- Initial release: a `no_std` + `alloc` arbitrary-precision implementation of
  the General Decimal Arithmetic core arithmetic, validated cohort-exact against
  CPython libmpdec. The coefficient backend is `ferrodec_multiword::DecBig`, a
  growable base-`10^9` decimal-limb integer. See ADR-0038.
