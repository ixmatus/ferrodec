//! IEEE 754-2019 Decimal64 in pure Rust. `no_std` capable, targeted
//! at embedded use, with the verification posture established by
//! the `ferrodec` crate (Decimal128) and the smaller sibling
//! `ferrodec-decimal32`.
//!
//! # Surface
//!
//! Every IEEE 754-2019 §5 mandatory operation on [`Decimal64`] plus
//! the §9.2 transcendentals under feature flags. Each operation
//! returns `(Decimal64, Status)` so callers compose IEEE 754
//! exception flags across a sequence of operations without
//! consulting any thread-local state. See the
//! [README](https://github.com/ixmatus/ferrodec/blob/main/ferrodec-decimal64/README.md)
//! for the full list.
//!
//! # IEEE 754-2019 §3.5 Decimal64 parameters
//!
//! - Storage width: 64 bits.
//! - Coefficient precision: 16 decimal digits (≈ 53.15 bits).
//! - Exponent range: -383 to 384 unbiased; 0 to 767 biased; bias 398.
//! - Maximum normal magnitude: 9.999999999999999 × 10³⁸⁴.
//! - Minimum positive normal magnitude: 1 × 10⁻³⁸³.
//! - Encoding: BID (binary integer significand) for arithmetic; DPD
//!   (densely packed decimal) planned for IEEE byte-pattern
//!   interchange.
//! - IEEE 754-2019 §6.3 exponent clamping is honoured.
//!
//! # Family-wide conventions
//!
//! - `Status`, `RoundingMode`, `IeeeClass` re-export from
//!   [`ferrodec-ieee`](https://crates.io/crates/ferrodec-ieee), so
//!   `ferrodec_decimal64::Status` and `ferrodec::Status` resolve to
//!   the *same* concrete type and flow across precisions without
//!   conversion (ADR-0012).
//! - `min` / `max` follow IEEE 754-2019 §9.6 `minimumNumber` /
//!   `maximumNumber` (quiet NaN is "missing value"), matching
//!   Decimal128, GDA, and decTest.
//! - Default `Display` uses the General Decimal Arithmetic `toSci`
//!   rule. This diverges from Decimal128's `f64::Display`-style
//!   boundary; ADR-0014 records the rationale and the v2.0
//!   harmonization plan.
//! - §9.2 transcendentals (`exp`, `ln`, trig, hyperbolic, pow, cbrt)
//!   route through `f64` via `libm`. Decimal64 carries 16 digits
//!   while `f64` carries ~15.95, so the f64 round-trip caps achievable
//!   precision at ~10⁻¹⁵ relative — one digit below Decimal64's
//!   nominal 16. v1.x ships this baseline; a future pure-decimal
//!   kernel will close the gap (the public surface is drop-in
//!   compatible).
//!
//! # Companion crates
//!
//! - [`ferrodec`](https://crates.io/crates/ferrodec): Decimal128.
//! - [`ferrodec-decimal32`](https://crates.io/crates/ferrodec-decimal32):
//!   Decimal32.
//! - [`ferrodec-ieee`](https://crates.io/crates/ferrodec-ieee):
//!   the shared IEEE 754 metadata types.

#![no_std]

mod bid;
mod classify;
mod cmp;
mod convert;
mod decimal;
#[cfg(feature = "num-traits")]
mod num_traits_impls;
mod ops;
#[cfg(feature = "ops")]
mod ops_traits;
#[cfg(feature = "serde")]
mod serde_impls;
#[cfg(kani)]
mod verify;

#[cfg(feature = "fmt")]
pub use convert::{Engineering, ParseDecimalError};
pub use decimal::{Decimal64, Decimal64BuildError};
pub use ferrodec_ieee::{IeeeClass, RoundingMode, Status};
#[cfg(feature = "serde")]
pub use serde_impls::serde_bid;
