//! IEEE 754-2019 Decimal64 in pure Rust. `no_std` capable, targeted at
//! embedded use, with the verification posture established by the
//! `ferrodec` crate (Decimal128) and the smaller sibling
//! `ferrodec-decimal32`.
//!
//! # IEEE 754-2019 §3.5 Decimal64 parameters
//!
//! - Storage width: 64 bits.
//! - Coefficient precision: 16 decimal digits (≈ 53.15 bits).
//! - Exponent range: -383 to 384 unbiased; 0 to 767 biased; bias 398.
//! - Maximum normal magnitude: 9.999999999999999 × 10³⁸⁴.
//! - Minimum positive normal magnitude: 1 × 10⁻³⁸³.
//! - Encoding: BID (binary integer significand) for arithmetic; DPD
//!   (densely packed decimal) planned for IEEE byte-pattern interchange.
//!
//! # Companion crates
//!
//! - [`ferrodec`](https://crates.io/crates/ferrodec): Decimal128, the
//!   sibling at v1.x.
//! - [`ferrodec-decimal32`](https://crates.io/crates/ferrodec-decimal32):
//!   Decimal32, the sibling at v1.x.

#![no_std]

mod bid;
mod classify;
mod classify_types;
mod cmp;
mod convert;
mod decimal;
mod ops;
mod status;
#[cfg(kani)]
mod verify;

pub use classify_types::IeeeClass;
#[cfg(feature = "fmt")]
pub use convert::{Engineering, ParseDecimalError};
pub use decimal::{Decimal64, Decimal64BuildError};
pub use status::{RoundingMode, Status};
