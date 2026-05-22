# ferrodec-decimal32

IEEE 754-2019 Decimal32 in pure Rust. `no_std` capable, targeted at
embedded use, with the verification posture established by the
[`ferrodec`](https://crates.io/crates/ferrodec) crate (Decimal128).

## What ferrodec-decimal32 is

A correct, well-tested, alloc-free Decimal32 implementation following
the IEEE 754-2019 binary-integer-significand encoding. Every value
returns from arithmetic with an explicit `(Decimal32, Status)` pair
so callers can compose IEEE 754 exception flags across a sequence of
operations without consulting any thread-local state.

The type is the smaller sibling of ferrodec's Decimal128: 32 bits of
storage, 7 decimal digits of precision, exponent range
`10⁻¹⁰¹..=10⁹⁶`. Sized for embedded telemetry, small-ledger
reporting, and any setting where you need exact decimal arithmetic
without paying the storage cost of a 128-bit format.

## Quick start

```rust
use ferrodec_decimal32::{Decimal32, RoundingMode};

let a: Decimal32 = "1.23".parse().unwrap();
let b = Decimal32::try_new(456, -2).unwrap(); // 4.56
let (sum, status) = a.add(b, RoundingMode::NearestEven);
assert_eq!(format!("{sum}"), "5.79");
assert!(status.is_ok());
```

For the IEEE 754 §9 transcendental kernels, the explicit
`(Decimal32, Status)` shape lets you propagate `INEXACT` cleanly:

```rust,ignore
let (e, s) = Decimal32::ONE.exp(RoundingMode::NearestEven);
assert!(s.inexact()); // exp(1) is irrational; rounded to 7 digits.
```

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

Form A (coefficient < 2²³) and Form B (coefficient ∈ [2²³, 10⁷))
are both canonical for BID-32, unlike BID-128, where Form B is
non-canonical. Form B encodings of coefficients ≥ 10⁷ canonicalise
to ±0 with the encoded sign and biased exponent, per IEEE 754-2019
§3.5.2.

## Feature surface

| Feature | What it adds |
| --- | --- |
| `fmt` (default) | `parse_str`, `Display`, `LowerExp`, `UpperExp`, `Engineering`. Alloc-free; uses `core::fmt::Write`. |
| `binary-float` | `Decimal32::to_f64`, `Decimal32::from_f64`. Auto-enabled by every transcendental feature. |
| `exp-log` | `exp`, `ln`, `exp2`, `log2`, `log10`. Correctly rounded via the shared `ferrodec-transcend` Extended-precision kernel (ADR-0032) (pure Rust, no FFI). |
| `trig` | `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`. Same shared correctly rounded kernel, Payne-Hanek argument reduction. |
| `hyperbolic` | `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`. Same shared correctly rounded kernel. Auto-pulls `exp-log`. |
| `pow` | `pow`, `cbrt`. Same shared correctly rounded kernel. Auto-pulls `exp-log`. |
| `transcendentals` | Convenience meta-feature: enables all four clusters. No transcendental routes through `f64`; `libm` is not a dependency. |
| `ops` | `core::ops` overloads (`Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg`, plus `*Assign`). Defaults to `RoundingMode::NearestEven`; drops `Status`. |
| `serde` | `Serialize` / `Deserialize` via the canonical decimal string. The `serde_bid` helper module serialises the raw 32-bit BID pattern in binary formats. |
| `num-traits` | `Zero`, `One`, `Bounded`, `Signed`, `Num`, `From\|To Primitive`. Auto-pulls `ops` + `binary-float` + `fmt`. |

## What you can call

- **Constructors**: `parse_str(s, rm)`, `try_new(coef: i32, exp: i32)`,
  `try_new_unsigned`, `from_bits(u32)`, `to_bits()`. From integers:
  `from_i32`, `from_u32`, `from_i64`, `from_u64`, `from_i128`,
  `from_u128`, all taking a `RoundingMode` and returning `(Decimal32,
  Status)` because Decimal32's 7 digits are narrower than every
  standard integer type. No `impl From<intN>` impls are provided
  (lossless `From` requires the integer type to fit, which none do).
  With `binary-float`: `from_f64`, `to_f64`, plus `impl TryFrom<f64>` /
  `impl TryFrom<f32>` (NaN and ±∞ reject through
  `Decimal32FromFloatError`; finite values flow through `from_f64`
  with `RoundingMode::NearestEven`, and very large finite `f64`
  magnitudes saturate to ±∞ at the decimal end of the conversion).
  Distinguished constants: `ZERO`, `NEG_ZERO`, `ONE`,
  `NEG_ONE`, `TEN`, `MAX`, `MIN`, `MIN_POSITIVE`, `MIN_POSITIVE_NORMAL`,
  `INFINITY`, `NEG_INFINITY`, `NAN`, `SIGNALING_NAN`.
- **§5 arithmetic**: `add`, `sub`, `mul`, `div`, `rem_near` / `rem_trunc`, `sqrt`, `fma`.
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
`(Decimal32, Status)`. The `Status` flags follow IEEE 754-2019 §7:
`INVALID`, `DIV_BY_ZERO`, `OVERFLOW`, `UNDERFLOW`, `INEXACT`. Compose
with `|=` / `Status::merge` across a sequence of operations.

## Accuracy

All §5 mandatory operations are correctly rounded per the active
[`RoundingMode`].

The whole §9.2 transcendental surface (`exp`, `ln`, `exp2`, `log2`,
`log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`,
`sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `pow`, `cbrt`)
is correctly rounded (ADR-0032; supersedes ADR-0024's faithful
contract) at every IEEE 754-2019 rounding direction through the
shared `ferrodec-transcend` Extended precision kernel, at exact
parity with the `ferrodec` (Decimal128) parent and the
`ferrodec-decimal64` sibling. No transcendental routes through
`f64` any more, so the pre-fd-r0l f64-round-trip detour is gone and
`libm` is no longer a dependency. The forward trig functions use
Payne-Hanek argument reduction (correctly rounded across the full
Decimal32 magnitude range); `pow` evaluates `exp(y · ln(|x|))` at
Extended precision.

The contract is correctly rounded (the returned value is the
single nearest representable result, ties to even at
`NearestEven` and the directed grid point at the four directed
modes), proved on every committed Arb worst-case vector and
empirically corroborated by MPFR with zero disagreements
(ADR-0026, ADR-0032).

## Supported targets

Tested in CI on:

- Linux `x86_64` (Ubuntu)
- macOS aarch64 (M-series)
- `thumbv6m-none-eabi` (Cortex-M0+ floor), cross-compiled with
  `--no-default-features` through `--all-features`.

`#![no_std]`, no allocation. Embeddable on any target Rust supports.
MSRV: Rust 1.84.

## Verification

| Pillar | Coverage |
| --- | --- |
| Conformance vectors | The runner consumes the vendored `ds*.decTest` files (`dsBase`, `dsEncode`). Pass and skip counts move as dispatch arms are wired in; the invariant is zero failures. Residual skips are extreme-exponent inputs and the non-IEEE rounding directives `half_down` / `05up` (will-not-fix per ferrodec ADR-0005). |
| Unit tests | Hand-derived expected values for every operation, special cases, sign rules, and rounding boundaries. |
| Property tests | Round-trip `parse_str → Display`. |
| Kani harnesses | Per-operation modules (addsub, mul, div, sqrt, fma, cmp) prove no-panic and IEEE 754 special-case propagation over a bounded operand set. Run via `cargo kani --package ferrodec-decimal32 --features=fmt`. |
| Fuzz | Four cargo-fuzz targets (parse, arith, transcendentals, `total_cmp`) covering panic-freedom and algebraic-identity invariants over arbitrary bit patterns. |

## Why no `core::ops` (and how to opt in)

By default, `Decimal32` does *not* implement `+`, `-`, `*`, `/`, `%`.
Every operation in IEEE 754 has a *rounding mode* and a *status flag
set*. Values arithmetic operators don't carry. The explicit method
form (`a.add(b, rm)` returning `(Decimal32, Status)`) makes both
visible at the call site.

For callers migrating from `f64` or `rust_decimal` who prefer the
operator surface, enable the `ops` feature: it implements `Add`,
`Sub`, `Mul`, `Div`, `Rem`, `Neg`, and the `*Assign` variants.
Default rounding is `NearestEven`; the `Status` is dropped. Mix and
match: `let (sum, st) = a.add(b, mode);` and `let sum = a + b;`
both compile when `ops` is on.

## Choosing between ferrodec / ferrodec-decimal32 / `rust_decimal`

| Scenario | Pick |
| --- | --- |
| 7-digit precision is enough, embedded / `no_std` target | **ferrodec-decimal32** |
| 16-digit precision (financial general ledger, scientific aggregates), the sweet spot between Decimal32 and Decimal128 | [`ferrodec-decimal64`](https://crates.io/crates/ferrodec-decimal64) |
| 34-digit precision, IEEE 754 Decimal128 surface | [`ferrodec`](https://crates.io/crates/ferrodec) |
| Variable / arbitrary precision, no IEEE 754 conformance needed | [`rust_decimal`](https://crates.io/crates/rust_decimal) |

## Porting between the ferrodec formats

The numeric value is portable across `ferrodec` (Decimal128), `ferrodec-decimal64`, and `ferrodec-decimal32`; the surface around it is not. `%` is the truncated remainder here but the nearest-even remainder on Decimal128, so prefer the explicit `rem_near` / `rem_trunc` for rule-stable code (the 1.x bare `rem` spelling was retired in 2.0 per ADR-0027). The cohort exponent, the `Display` rendering (ADR-0014), and the transcendental feature gating also differ per format. Pin what you serialize or compare with `quantize`. The [`ferrodec`](https://crates.io/crates/ferrodec) crate README carries the full cross-format table.

## Internals worth knowing

- The Decimal32 BID layout: 1 sign bit + 5-bit type field + 6-bit
  exponent continuation + 20-bit trailing significand. Form A and
  Form B both carry canonical values; the canonicalisation rules
  are documented in `src/bid.rs`.
- All arithmetic routes through `round_and_pack_finite` in
  `src/ops/round.rs`: a single source of truth for digit drop with
  guard / sticky tracking, IEEE 754 rounding-direction application,
  and `INEXACT` / `OVERFLOW` / `UNDERFLOW` flag emission.
- Working precision for arithmetic fits in `u64` (no multiword
  needed). FMA uses `u128` for the exact-product alignment with
  `c`, but compresses back to `u64` via sticky tracking before
  routing through the standard rounding path.
- The transcendental kernels route through the shared
  `ferrodec-transcend` Extended-precision kernel (no `f64` / `libm`
  detour). Each short-circuits the IEEE 754 §9.2 special cases (sNaN
  propagation, zero / infinity boundaries, domain errors) in a
  byte-stable `*_special_cases` routine before entering the kernel,
  so the spec-mandated flags and the ADR-0016 Kani special-case
  proofs stay byte-identical. The result is correctly rounded
  (ADR-0032); see the accuracy note above.

## MSRV policy

Rust 1.84. The MSRV will move forward only when a compelling reason
arises (a Rust feature that materially simplifies the
implementation); each change is documented in CHANGELOG and
released as a minor version bump per
[Cargo's MSRV semver guidelines](https://blog.rust-lang.org/2023/10/05/msrv-semver-policy.html).

## License

MIT OR Apache-2.0, at your option. Same dual-license shape as
ferrodec.

## Reading list

- IEEE 754-2019: the source-of-truth specification.
- General Decimal Arithmetic Specification (Mike Cowlishaw):
  detailed semantics for the `toSci` / `toEng` formats and the
  decTest conformance suite this crate's `tests/vectors/`
  consumes.
- ferrodec's README: the verification methodology and ADR archive
  this crate inherits.
