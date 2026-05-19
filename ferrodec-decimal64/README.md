# ferrodec-decimal64

IEEE 754-2019 Decimal64 in pure Rust. `no_std` capable, targeted at
embedded use, with the verification posture established by the
[`ferrodec`](https://crates.io/crates/ferrodec) crate (Decimal128) and
shared with the smaller sibling
[`ferrodec-decimal32`](https://crates.io/crates/ferrodec-decimal32).

## What ferrodec-decimal64 is

A correct, well-tested, alloc-free Decimal64 implementation following
the IEEE 754-2019 binary-integer-significand encoding. Every value
returns from arithmetic with an explicit `(Decimal64, Status)` pair
so callers can compose IEEE 754 exception flags across a sequence of
operations without consulting any thread-local state.

The type is the middle sibling of the family: 64 bits of storage, 16
decimal digits of precision, exponent range `10⁻³⁸³..=10⁺³⁸⁴`. The
natural sweet spot for financial general ledgers (16 digits cover
ten quadrillion units to the cent), telemetry aggregates, and any
setting where Decimal32's 7 digits run out but Decimal128's 128
bits are storage-expensive.

## Quick start

```rust
use ferrodec_decimal64::{Decimal64, RoundingMode};

let a = Decimal64::parse_str("1.23456789012345", RoundingMode::NearestEven).unwrap().0;
let b = Decimal64::try_new(45_678_901_234, -10).unwrap(); // 4.5678901234
let (sum, status) = a.add(b, RoundingMode::NearestEven);
assert!(status.is_ok());
```

For the IEEE 754 §9 transcendental kernels, the explicit
`(Decimal64, Status)` shape lets you propagate `INEXACT` cleanly:

```rust,ignore
let (e, s) = Decimal64::ONE.exp(RoundingMode::NearestEven);
assert!(s.inexact()); // exp(1) is irrational; rounded to 16 digits.
```

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

Form A (coefficient < 2⁵³) and Form B (coefficient ∈ [2⁵³, 10¹⁶))
are both canonical for BID-64 — unlike BID-128 where Form B is
non-canonical. Form B encodings of coefficients ≥ 10¹⁶ canonicalise
to ±0 with the encoded sign and biased exponent, per IEEE 754-2019
§3.5.2.

IEEE 754-2019 §6.3 exponent clamping is honoured: a result whose
biased exponent exceeds `BIASED_EXP_MAX` but whose adjusted exponent
is in range gets its coefficient padded with trailing zeros to fit
the encoding (the "Clamped" condition). The conformance suite
exercises this path heavily for Decimal64 even though Decimal32's
narrower exponent range rarely triggers it.

## Feature surface

| Feature | What it adds |
| --- | --- |
| `fmt` (default) | `parse_str`, `Display`, `LowerExp`, `UpperExp`, `Engineering`. Alloc-free; uses `core::fmt::Write`. |
| `binary-float` | `Decimal64::to_f64`, `Decimal64::from_f64`. Auto-enabled by every transcendental feature. |
| `exp-log` | `exp`, `ln`, `exp2`, `log2`, `log10`. Faithfully rounded via the shared `ferrodec-transcend` Extended-precision kernel (pure Rust, no FFI). |
| `trig` | `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`. Same shared faithful kernel, Payne-Hanek argument reduction. |
| `hyperbolic` | `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`. Same shared faithful kernel. Auto-pulls `exp-log`. |
| `pow` | `pow`, `cbrt`. Same shared faithful kernel. Auto-pulls `exp-log`. |
| `transcendentals` | Convenience meta-feature: enables all four clusters. No transcendental routes through `f64`; `libm` is not a dependency. |
| `ops` | `core::ops` overloads (`Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg`, plus `*Assign`). Defaults to `RoundingMode::NearestEven`; drops `Status`. |
| `serde` | `Serialize` / `Deserialize` via the canonical decimal string. The `serde_bid` helper module serialises the raw 64-bit BID pattern in binary formats. |
| `num-traits` | `Zero`, `One`, `Bounded`, `Signed`, `Num`, `From\|To Primitive`. Auto-pulls `ops` + `binary-float` + `fmt`. |

## What you can call

- **Constructors**: `parse_str(s, rm)`, `try_new(coef: i64, exp: i32)`,
  `try_new_unsigned`, `from_bits(u64)`, `to_bits()`, `from_f64`,
  `to_f64`. Distinguished constants: `ZERO`, `NEG_ZERO`, `ONE`,
  `NEG_ONE`, `TEN`, `MAX`, `MIN`, `MIN_POSITIVE`, `MIN_POSITIVE_NORMAL`,
  `INFINITY`, `NEG_INFINITY`, `NAN`, `SIGNALING_NAN`.
- **§5 arithmetic**: `add`, `sub`, `mul`, `div`, `rem`, `sqrt`, `fma`.
- **§5 comparison**: `partial_cmp`, `total_cmp`,
  `compare_total_magnitude`.
- **§9.6 selection**: `min`, `max` (`minimumNumber` /
  `maximumNumber`), `min_magnitude`, `max_magnitude` (the magnitude
  variants, deferring to `min` / `max` on an equal-magnitude tie;
  ADR-0028).
- **§5 quantum**: `quantize`, `scaleb`, `logb`, `next_up`, `next_down`.
- **§5 classification**: `is_nan`, `is_infinite`, `is_finite`,
  `is_zero`, `is_normal`, `is_subnormal`, `is_sign_positive`,
  `is_sign_negative`, `is_signaling_nan`, `is_quiet_nan`, `classify`,
  `ieee_class`.
- **§5 sign**: `abs`, `neg`, `copysign`, `abs_with_status`,
  `neg_with_status`.
- **§5 canonical**: `is_canonical`, `canonicalize`.
- **§9.2 transcendental**: `exp`, `ln`, `sin`, `cos`, `tan`, `asin`,
  `acos`, `atan`, `atan2`, `sinh`, `cosh`, `tanh`, `asinh`, `acosh`,
  `atanh`, `pow`, `cbrt`.

Every operation that can lose precision returns
`(Decimal64, Status)`. The `Status` flags follow IEEE 754-2019 §7:
`INVALID`, `DIV_BY_ZERO`, `OVERFLOW`, `UNDERFLOW`, `INEXACT`. Compose
with `|=` / `Status::merge` across a sequence of operations.

## Accuracy

All §5 mandatory operations are correctly rounded per the active
[`RoundingMode`].

The whole §9.2 transcendental surface (`exp`, `ln`, `exp2`, `log2`,
`log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`,
`sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `pow`, `cbrt`)
is faithfully rounded (≤ 1 ULP at 16 digits, every IEEE 754-2019
rounding direction) via the shared `ferrodec-transcend`
Extended-precision kernel, at exact parity with the `ferrodec`
(Decimal128) parent. No transcendental routes through `f64` any
more, so the pre-fd-r0l ~10⁻¹⁵ f64-round-trip cap is lifted and
`libm` is no longer a dependency. The forward trig functions use
Payne-Hanek argument reduction (faithful across the full Decimal64
magnitude range, not capped at the old `|x| < 2^53` limit); `pow`
evaluates `exp(y · ln(|x|))` at Extended precision.

The contract is faithful rounding (the returned value is one of the
two representable values bracketing the exact result), not
correct rounding (always the single nearest); ADR-0021 records it.

## Supported targets

Tested in CI on:

- Linux x86_64 (Ubuntu)
- macOS aarch64 (M-series)
- `thumbv6m-none-eabi` (Cortex-M0+ floor) — cross-compiled with
  `--no-default-features` through `--all-features`.

`#![no_std]`, no allocation. Embeddable on any target Rust supports.
MSRV: Rust 1.84.

## Verification

| Pillar | Coverage |
| --- | --- |
| Conformance vectors | The runner consumes every `dd*.decTest` file from the IBM / Speleotrove suite. Pass and skip counts move as dispatch arms are wired in (add/sub/mul/div/fma, the comparison and quantum surface, `rem` / `rem_near`, roundToIntegral, the copy family, and the §9.6 magnitude operations are dispatched); the invariant is zero failures. |
| Unit tests | Hand-derived expected values for every operation, special cases, sign rules, and rounding boundaries. |
| Property tests | Round-trip `parse_str → Display`. |
| Kani harnesses | Per-operation modules (addsub, mul, div, sqrt, fma, cmp) prove no-panic and IEEE 754 special-case propagation over a bounded operand set. Run via `cargo kani --package ferrodec-decimal64 --features=transcendentals`. |
| Fuzz | Four cargo-fuzz targets (parse, arith, transcendentals, total_cmp) covering panic-freedom and algebraic-identity invariants over arbitrary u64 bit patterns. |

## Why no `core::ops` (and how to opt in)

By default, `Decimal64` does *not* implement `+`, `-`, `*`, `/`, `%`.
Every operation in IEEE 754 has a *rounding mode* and a *status flag
set* — values arithmetic operators don't carry. The explicit method
form (`a.add(b, rm)` returning `(Decimal64, Status)`) makes both
visible at the call site.

For callers migrating from `f64` or `rust_decimal` who prefer the
operator surface, enable the `ops` feature: it implements `Add`,
`Sub`, `Mul`, `Div`, `Rem`, `Neg`, and the `*Assign` variants.
Default rounding is `NearestEven`; the `Status` is dropped. Mix and
match — `let (sum, st) = a.add(b, mode);` and `let sum = a + b;`
both compile when `ops` is on.

## Choosing between ferrodec / ferrodec-decimal64 / `rust_decimal`

| Scenario | Pick |
| --- | --- |
| 7-digit precision is enough, embedded / no_std target | [`ferrodec-decimal32`](https://crates.io/crates/ferrodec-decimal32) |
| 16-digit precision (financial general ledger, scientific aggregates), embedded / no_std friendly | **ferrodec-decimal64** |
| 34-digit precision, IEEE 754 Decimal128 surface | [`ferrodec`](https://crates.io/crates/ferrodec) |
| Variable / arbitrary precision, no IEEE 754 conformance needed | [`rust_decimal`](https://crates.io/crates/rust_decimal) |

## Porting between the ferrodec formats

The numeric value is portable across `ferrodec` (Decimal128), `ferrodec-decimal64`, and `ferrodec-decimal32`; the surface around it is not. Bare `rem` and `%` are the truncated remainder here but the nearest-even remainder on Decimal128, so use the explicit `rem_near` / `rem_trunc` (ADR-0027; a 2.0 rename is planned). The cohort exponent, the `Display` rendering (ADR-0014), and the transcendental feature gating also differ per format. Pin what you serialize or compare with `quantize`. The [`ferrodec`](https://crates.io/crates/ferrodec) crate README carries the full cross-format table.

## Internals worth knowing

- The Decimal64 BID layout: 1 sign bit + 5-bit type field + 8-bit
  exponent continuation + 50-bit trailing significand. Form A and
  Form B both carry canonical values; the canonicalisation rules
  are documented in `src/bid.rs`.
- All arithmetic routes through `round_and_pack_finite` in
  `src/ops/round.rs` — a single source of truth for digit drop with
  guard / sticky tracking, IEEE 754 rounding-direction application,
  IEEE 754-2019 §6.3 exponent clamping, and `INEXACT` / `OVERFLOW` /
  `UNDERFLOW` flag emission.
- Working precision for arithmetic uses `u128` (Decimal64's 16-digit
  coefficients can overflow `u64` after alignment shifts). A small
  `round_and_pack_into_u64` helper compresses the u128 working
  value back to the canonical u64 with sticky tracking before
  routing through `round_and_pack_finite`.
- The transcendental kernels route through the shared
  `ferrodec-transcend` Extended-precision kernel (no `f64` / `libm`
  detour). Each short-circuits the IEEE 754 §9.2 special cases
  (sNaN propagation, zero / infinity boundaries, domain errors) in a
  byte-stable `*_special_cases` routine before entering the kernel,
  so the spec-mandated flags and the ADR-0016 Kani special-case
  proofs stay byte-identical. The result is faithfully rounded
  (≤ 1 ULP at 16 digits); see the accuracy note above.

## MSRV policy

Rust 1.84. The MSRV will move forward only when a compelling reason
arises (a Rust feature that materially simplifies the
implementation); each change is documented in CHANGELOG and
released as a minor version bump per
[Cargo's MSRV semver guidelines](https://blog.rust-lang.org/2023/10/05/msrv-semver-policy.html).

## License

MIT OR Apache-2.0, at your option. Same dual-license shape as the
rest of the ferrodec family.

## Reading list

- IEEE 754-2019: the source-of-truth specification.
- General Decimal Arithmetic Specification (Mike Cowlishaw):
  detailed semantics for the `toSci` / `toEng` formats and the
  decTest conformance suite this crate's `tests/vectors/` consumes.
- ferrodec's README: the verification methodology and ADR archive
  this crate inherits.
