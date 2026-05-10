//! IEEE 754-2019 Decimal32 in pure Rust. `no_std` capable, targeted at
//! embedded use, with the verification posture established by the
//! `ferrodec` crate (Decimal128).
//!
//! This crate is in early development. The currently exposed surface is
//! the [`Decimal32`] type with classification methods (`is_nan`,
//! `is_infinite`, `is_finite`, `is_zero`, `is_normal`, `is_subnormal`,
//! `is_signaling_nan`, `is_quiet_nan`, `is_sign_negative`,
//! `is_sign_positive`, `classify`, `ieee_class`), sign manipulation
//! (`abs`, `neg`, `abs_with_status`, `neg_with_status`, `copysign`),
//! canonicalisation (`is_canonical`, `canonicalize`), constructors
//! (`try_new`, `try_new_unsigned`, `from_bits`, `to_bits`), and a set
//! of distinguished constants (`ZERO`, `ONE`, `MAX`, `INFINITY`,
//! `NAN`, ...). Arithmetic, parse, format, and verification land in
//! subsequent commits per the plan archived at
//! `docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`
//! in the workspace root.
//!
//! # IEEE 754-2019 §3.5 Decimal32 parameters
//!
//! - Storage width: 32 bits.
//! - Coefficient precision: 7 decimal digits (≈ 23.25 bits).
//! - Exponent range: -101 to 96 unbiased; 0 to 191 biased; bias 101.
//! - Maximum normal magnitude: 9.999999 × 10⁹⁶.
//! - Minimum positive normal magnitude: 1 × 10⁻⁹⁵.
//! - Encoding: BID (binary integer significand) for arithmetic; DPD
//!   (densely packed decimal) planned for IEEE byte-pattern interchange.
//!
//! # Companion crates
//!
//! - [`ferrodec`](https://crates.io/crates/ferrodec): Decimal128, the
//!   sibling at v1.x.
//! - `ferrodec-decimal64`: Decimal64, in development.

#![no_std]

mod bid;
mod classify;
mod classify_types;
mod convert;
mod decimal;
mod ops;
mod status;

pub use classify_types::IeeeClass;
#[cfg(feature = "fmt")]
pub use convert::{Engineering, ParseDecimalError};
pub use decimal::{Decimal32, Decimal32BuildError};
pub use status::{RoundingMode, Status};
