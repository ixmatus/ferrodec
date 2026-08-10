//! IEEE 754-2019 Decimal32 in pure Rust. `no_std` capable, targeted
//! at embedded use, with the verification posture established by
//! the `ferrodec` crate (Decimal128).
//!
//! # Surface
//!
//! Every IEEE 754-2019 §5 mandatory operation on [`Decimal32`] plus
//! the §9.2 transcendentals under feature flags. Each operation
//! returns `(Decimal32, Status)` so callers compose IEEE 754
//! exception flags across a sequence of operations without
//! consulting any thread-local state. See the
//! [README](https://github.com/ixmatus/ferrodec/blob/main/ferrodec-decimal32/README.md)
//! for the full list.
//!
//! The §9.2 transcendentals are correctly rounded (ADR-0032;
//! supersedes ADR-0024's faithful contract) at every IEEE 754-2019
//! rounding direction through the shared `ferrodec-transcend`
//! Extended precision kernel, at exact parity with the Decimal64
//! sibling and the `ferrodec` (Decimal128) parent.
//!
//! # IEEE 754-2019 §3.5 Decimal32 parameters
//!
//! - Storage width: 32 bits.
//! - Coefficient precision: 7 decimal digits (≈ 23.25 bits).
//! - Exponent range: -101 to 96 unbiased; 0 to 191 biased; bias 101.
//! - Maximum normal magnitude: 9.999999 × 10⁹⁶.
//! - Minimum positive normal magnitude: 1 × 10⁻⁹⁵.
//! - Encoding: BID (binary integer significand) for arithmetic; DPD
//!   (densely packed decimal) for IEEE byte-pattern interchange via
#![cfg_attr(
    feature = "dpd",
    doc = "  [`Decimal32::to_dpd_bytes`] / [`Decimal32::from_dpd_bytes`]"
)]
#![cfg_attr(
    not(feature = "dpd"),
    doc = "  `Decimal32::to_dpd_bytes` / `Decimal32::from_dpd_bytes`"
)]
//!   behind the off-by-default `dpd` feature (ADR-0009).
//!
//! # Family-wide conventions
//!
//! - `Status`, `RoundingMode`, `IeeeClass` re-export from
//!   [`ferrodec-ieee`](https://crates.io/crates/ferrodec-ieee), so
//!   `ferrodec_decimal32::Status` and `ferrodec::Status` resolve to
//!   the *same* concrete type and flow across precisions without
//!   conversion (ADR-0012).
//! - `min` / `max` follow IEEE 754-2019 §9.6 `minimumNumber` /
//!   `maximumNumber` (quiet NaN is "missing value"), matching
//!   Decimal128, GDA, and decTest.
//! - Default `Display` uses the General Decimal Arithmetic `toSci`
//!   rule, matching Decimal128 (whose 1.x `f64::Display`-style boundary
//!   was retired in 2.0 per ADR-0014). Callers that need the 1.x
//!   integer-style rendering can wrap a value in
//!   [`Decimal32::fixed_preferred`].
//!
//! # Cohort stability
//!
//! A finite decimal value has many encodings: `1.5`, `1.50`, and
//! `1.500` are one number at different exponents, its *cohort*.
//! `ferrodec-decimal32` preserves the numeric value of every
//! operation exactly as IEEE 754-2019 and the General Decimal
//! Arithmetic specify, and that value is stable across the ferrodec
//! formats and against any conforming GDA implementation. The
//! exponent selected within the cohort is not guaranteed to match
//! another implementation, and a future version may select a
//! different member. Code that serializes or renders the encoding
//! (quantize-then-serialize, fixed-point money display, golden-file
//! comparison) must pin the exponent with `quantize` rather than
//! rely on the default. The same principle, a stable value behind a
//! surface that names its divergences, applied to the 1.x bare `rem`
//! spelling: the 2.0 release retired it in favour of explicit
//! `rem_near` (IEEE 754-2019 §5.3.1 nearest-even) and `rem_trunc`
//! (GDA truncated) on all three formats. The `%` operator routes to
//! `rem_trunc` on this format (the rule the bare `rem` carried in
//! 1.x) and to `rem_near` on the Decimal128 parent, with the
//! per-format choice documented under ADR-0027.
//!
//! # Companion crates
//!
//! - [`ferrodec`](https://crates.io/crates/ferrodec): Decimal128.
//! - [`ferrodec-decimal64`](https://crates.io/crates/ferrodec-decimal64):
//!   Decimal64.
//! - [`ferrodec-decimal`](https://crates.io/crates/ferrodec-decimal):
//!   arbitrary precision General Decimal Arithmetic (needs an allocator).
//! - [`ferrodec-ieee`](https://crates.io/crates/ferrodec-ieee):
//!   the shared IEEE 754 metadata types.
//!
//! # Porting between the formats
//!
//! The numeric value is portable across the three formats; the
//! surface is not. `%` is GDA truncated here and IEEE nearest-even on
//! Decimal128, so prefer the explicit `rem_near` / `rem_trunc` for
//! rule-stable code (1.x retired the ambiguous bare `rem` per
//! ADR-0027). The cohort exponent, `Display`, and the transcendental
//! feature gating also differ per format. Pin what you serialize or
//! compare with `quantize`. ADR-0027 and ADR-0014 give the
//! rationale; the `ferrodec` crate README carries the full
//! cross-format table.

#![no_std]
#![doc = include_str!("../README.md")]

mod bid;
mod classify;
mod cmp;
mod convert;
mod decimal;
mod digits;
#[cfg(feature = "dpd")]
mod dpd;
mod iter;
#[cfg(any(feature = "trig", feature = "exp-log"))]
mod math;
#[cfg(feature = "num-traits")]
mod num_traits_impls;
mod ops;
#[cfg(feature = "ops")]
mod ops_traits;
#[cfg(feature = "serde")]
mod serde_impls;
// `impl DecimalFormat for Decimal32`, the seam every shared kernel is
// instantiated through. Every feature that delegates to
// `ferrodec-transcend` needs it, not just `exp-log`: `trig` reaches it
// through `ops::trig` and `trig-pi` through `ops::trig_pi`.
#[cfg(any(feature = "exp-log", feature = "trig", feature = "trig-pi"))]
mod transcend_impl;
#[cfg(kani)]
mod verify;

#[cfg(feature = "binary-float")]
pub use convert::Decimal32FromFloatError;
#[cfg(feature = "fmt")]
pub use convert::{Engineering, FixedPreferred, ParseDecimalError};
pub use decimal::{Decimal32, Decimal32BuildError, Decimal32Parts};
pub use ferrodec_ieee::{IeeeClass, RoundingMode, Status};
#[cfg(any(feature = "trig", feature = "exp-log"))]
pub use math::{e, ln10, ln2, pi};
#[cfg(feature = "num-traits")]
pub use num_traits_impls::FromStrRadixError;
#[cfg(feature = "serde")]
pub use serde_impls::serde_bid;
